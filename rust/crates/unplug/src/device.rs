// Copyright (C) 2026 the Unplug contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
//! What a USB device is to the tool, and the rules that decide how it is shown: its name, its category, whether it may be blocked.

/// One USB device as Windows reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsbDevice {
    /// The device instance id (`USB\VID_046D&PID_C215\5&2A1B3C&0&2`): what Windows calls the device by.
    pub instance_id: String,
    pub name: String,
    pub manufacturer: String,
    /// The hardware and compatible ids, upper-cased: they say what class of device it is.
    pub ids: Vec<String>,
    /// True while the device is turned off.
    pub blocked: bool,
}

/// What a device is for, found from what it says about itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    /// A joystick, pad, wheel, throttle or pedals.
    Controller,
    Keyboard,
    Mouse,
    Storage,
    Audio,
    Camera,
    Other,
}

impl Category {
    pub const ALL: [Category; 7] = [
        Category::Controller,
        Category::Keyboard,
        Category::Mouse,
        Category::Storage,
        Category::Audio,
        Category::Camera,
        Category::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::Controller => "Controllers",
            Category::Keyboard => "Keyboards",
            Category::Mouse => "Mice",
            Category::Storage => "Storage",
            Category::Audio => "Audio",
            Category::Camera => "Cameras",
            Category::Other => "Other",
        }
    }

    /// A keyboard or a mouse cannot be blocked: it would lock the player out of the computer.
    pub fn protected(self) -> bool {
        matches!(self, Category::Keyboard | Category::Mouse)
    }
}

const KEYBOARD_WORDS: &[&str] = &["keyboard", "galleon", "keypad"];
const MOUSE_WORDS: &[&str] = &["mouse", "trackpad", "touchpad", "touch pad", "trackball"];
const CONTROLLER_WORDS: &[&str] = &[
    "joystick",
    "gamepad",
    "game pad",
    "controller",
    "throttle",
    "hotas",
    "flight",
    "rudder",
    "pedal",
    "wheel",
    "xbox",
    "dualshock",
    "dualsense",
    "thrustmaster",
    "saitek",
    "vkb",
    "virpil",
    "winwing",
    "t.16000",
    "extreme 3d",
    "warthog",
    "stick",
];
const STORAGE_WORDS: &[&str] = &["storage", "flash", "disk", "drive", "ssd", "card reader"];
const AUDIO_WORDS: &[&str] = &["audio", "headset", "headphone", "speaker", "microphone", "sound"];
const CAMERA_WORDS: &[&str] = &["webcam", "camera"];

impl UsbDevice {
    /// `(vendor, product)` read from the instance id, when it has the usual `VID_xxxx&PID_xxxx` form.
    pub fn vid_pid(&self) -> Option<(u16, u16)> {
        let id = self.instance_id.to_ascii_uppercase();
        let vid = u16::from_str_radix(id.split("VID_").nth(1)?.get(..4)?, 16).ok()?;
        let pid = u16::from_str_radix(id.split("PID_").nth(1)?.get(..4)?, 16).ok()?;
        Some((vid, pid))
    }

    /// `046D:C215`, or an empty string for a device without a vendor and a product.
    pub fn vid_pid_text(&self) -> String {
        self.vid_pid().map(|(v, p)| format!("{v:04X}:{p:04X}")).unwrap_or_default()
    }

    /// A hub is the plumbing of the bus, not a device to block.
    pub fn is_hub(&self) -> bool {
        let id = self.instance_id.to_ascii_uppercase();
        id.contains("ROOT_HUB") || self.ids.iter().any(|i| i.starts_with("USB\\CLASS_09")) || self.name.to_lowercase().contains(" hub")
    }

    /// An interface of a composite device (`...&MI_00`): the device it belongs to is the one to block.
    pub fn is_interface(&self) -> bool {
        self.instance_id.to_ascii_uppercase().contains("&MI_")
    }

    /// Whether the device is worth listing: not a hub and not an interface of another device.
    pub fn is_listed(&self) -> bool {
        !self.is_hub() && !self.is_interface()
    }

    /// The category the device looks like it belongs to. What it reports about itself (a boot keyboard or mouse, a class) comes before its name.
    pub fn category(&self) -> Category {
        let name = self.name.to_lowercase();
        let has_id = |part: &str| self.ids.iter().any(|i| i.contains(part));
        let named = |words: &[&str]| words.iter().any(|w| name.contains(w));
        // a boot keyboard or mouse is HID class 03, subclass 01, protocol 01 (keyboard) or 02 (mouse)
        if has_id("CLASS_03&SUBCLASS_01&PROT_01") || named(KEYBOARD_WORDS) {
            Category::Keyboard
        } else if has_id("CLASS_03&SUBCLASS_01&PROT_02") || named(MOUSE_WORDS) {
            Category::Mouse
        } else if named(CONTROLLER_WORDS) {
            Category::Controller
        } else if self.ids.iter().any(|i| i.starts_with("USB\\CLASS_08")) || named(STORAGE_WORDS) {
            Category::Storage
        } else if self.ids.iter().any(|i| i.starts_with("USB\\CLASS_01")) || named(AUDIO_WORDS) {
            Category::Audio
        } else if self.ids.iter().any(|i| i.starts_with("USB\\CLASS_0E")) || named(CAMERA_WORDS) {
            Category::Camera
        } else {
            Category::Other
        }
    }
}

/// The devices to list. A composite device (a keyboard with a screen, a headset with buttons) is several interfaces under one device: what the
/// interfaces say about themselves is added to the device, then the interfaces and the hubs are dropped. Sorted by name.
pub fn group(all: Vec<UsbDevice>) -> Vec<UsbDevice> {
    let interfaces: Vec<(String, Vec<String>)> = all
        .iter()
        .filter(|d| d.is_interface())
        .filter_map(|d| {
            let id = d.instance_id.to_ascii_uppercase();
            let parent = id.split("&MI_").next()?.to_string();
            Some((parent, d.ids.clone()))
        })
        .collect();
    let mut listed: Vec<UsbDevice> = all.into_iter().filter(UsbDevice::is_listed).collect();
    for d in &mut listed {
        let id = d.instance_id.to_ascii_uppercase();
        for (parent, ids) in &interfaces {
            if id.starts_with(&format!("{parent}\\")) {
                for i in ids {
                    if !d.ids.contains(i) {
                        d.ids.push(i.clone());
                    }
                }
            }
        }
    }
    listed.sort_by_key(|d| (d.name.to_lowercase(), d.instance_id.clone()));
    listed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: &str, name: &str, ids: &[&str]) -> UsbDevice {
        UsbDevice {
            instance_id: id.into(),
            name: name.into(),
            manufacturer: String::new(),
            ids: ids.iter().map(|s| s.to_string()).collect(),
            blocked: false,
        }
    }

    #[test]
    fn the_vendor_and_the_product_come_from_the_instance_id() {
        let d = device(r"USB\VID_046d&PID_c215\5&2A1B&0&2", "Extreme 3D", &[]);
        assert_eq!(d.vid_pid(), Some((0x046D, 0xC215)));
        assert_eq!(d.vid_pid_text(), "046D:C215");
        assert_eq!(device(r"USB\ROOT_HUB30\4&1", "Root", &[]).vid_pid(), None);
    }

    #[test]
    fn hubs_and_interfaces_are_not_listed() {
        assert!(device(r"USB\ROOT_HUB30\4&1", "USB Root Hub (USB 3.0)", &[]).is_hub());
        assert!(device(r"USB\VID_8087&PID_0024\5&1", "Generic", &["USB\\CLASS_09&SUBCLASS_00"]).is_hub());
        assert!(device(r"USB\VID_1234&PID_0001\1", "Some Hub", &[]).is_hub());
        assert!(device(r"USB\VID_046D&PID_C215&MI_00\6&1", "HID", &[]).is_interface());
        assert!(device(r"USB\VID_046D&PID_C215\5&1", "Extreme 3D", &[]).is_listed());
        assert!(!device(r"USB\VID_046D&PID_C215&MI_00\6&1", "HID", &[]).is_listed());
    }

    #[test]
    fn a_device_is_filed_by_what_it_reports_then_by_its_name() {
        let boot = device(r"USB\VID_1&PID_2\1", "USB Input Device", &["USB\\CLASS_03&SUBCLASS_01&PROT_01"]);
        assert_eq!(boot.category(), Category::Keyboard);
        let mouse = device(r"USB\VID_1&PID_2\1", "USB Input Device", &["USB\\CLASS_03&SUBCLASS_01&PROT_02"]);
        assert_eq!(mouse.category(), Category::Mouse);
        let cat = |name: &str, ids: &[&str]| device(r"USB\VID_1&PID_2\1", name, ids).category();
        assert_eq!(cat("Gaming Mouse", &[]), Category::Mouse);
        assert_eq!(cat("CORSAIR GALLEON 100 SD", &[]), Category::Keyboard);
        assert_eq!(cat("Thrustmaster T.16000M FCS", &[]), Category::Controller);
        assert_eq!(cat("Controller (XBOX 360 For Windows)", &[]), Category::Controller);
        assert_eq!(cat("USB Mass Storage Device", &[]), Category::Storage);
        assert_eq!(cat("USB Device", &["USB\\CLASS_08&SUBCLASS_06"]), Category::Storage);
        assert_eq!(cat("USB Device", &["USB\\CLASS_01&SUBCLASS_01"]), Category::Audio);
        assert_eq!(cat("Integrated Webcam", &[]), Category::Camera);
        assert_eq!(cat("USB Composite Device", &[]), Category::Other);
    }

    #[test]
    fn a_composite_keyboard_is_a_keyboard_whatever_it_is_called() {
        // a keyboard with a screen: the device is "USB Composite Device", its first interface is the keyboard
        let all = vec![
            device(r"USB\VID_1B1C&PID_0001\ABC", "USB Composite Device", &["USB\\COMPOSITE"]),
            device(
                r"USB\VID_1B1C&PID_0001&MI_00\6&1",
                "USB Input Device",
                &["USB\\CLASS_03&SUBCLASS_01&PROT_01"],
            ),
            device(
                r"USB\VID_1B1C&PID_0001&MI_01\6&2",
                "USB Input Device",
                &["USB\\CLASS_03&SUBCLASS_00"],
            ),
            device(r"USB\VID_0002&PID_0002\XYZ", "Flight Stick", &[]),
            device(r"USB\ROOT_HUB30\1", "USB Root Hub", &[]),
        ];
        let listed = group(all);
        let names: Vec<_> = listed.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["Flight Stick", "USB Composite Device"]);
        assert_eq!(listed[1].category(), Category::Keyboard);
        assert!(listed[1].category().protected());
        assert_eq!(listed[0].category(), Category::Controller);
    }

    #[test]
    fn only_a_keyboard_or_a_mouse_is_protected() {
        assert!(Category::Keyboard.protected() && Category::Mouse.protected());
        assert!(!Category::Controller.protected() && !Category::Other.protected());
    }
}
