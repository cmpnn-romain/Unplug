// Copyright (C) 2026 the Unplug contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
//! What a USB device is to the tool, and the rules that decide how it is shown: the name, the kind, whether it may be blocked.

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

/// What kind of device it looks like.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A joystick, pad, wheel, throttle or pedals.
    Controller,
    /// A keyboard or a mouse: blocking it would lock the player out of the computer.
    Input,
    Other,
}

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

const INPUT_WORDS: &[&str] = &["keyboard", "mouse", "trackpad", "touchpad", "touch pad"];

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

    pub fn kind(&self) -> Kind {
        let name = self.name.to_lowercase();
        // a boot keyboard or mouse is HID class 03, subclass 01, protocol 01 (keyboard) or 02 (mouse)
        let boot_input = self
            .ids
            .iter()
            .any(|i| i.contains("CLASS_03&SUBCLASS_01&PROT_01") || i.contains("CLASS_03&SUBCLASS_01&PROT_02"));
        if boot_input || INPUT_WORDS.iter().any(|w| name.contains(w)) {
            Kind::Input
        } else if CONTROLLER_WORDS.iter().any(|w| name.contains(w)) {
            Kind::Controller
        } else {
            Kind::Other
        }
    }

    /// Whether the device is worth listing: not a hub and not an interface of another device.
    pub fn is_listed(&self) -> bool {
        !self.is_hub() && !self.is_interface()
    }
}

/// The devices to show: controllers first, then the rest by name; `query` keeps those whose name, maker or ids contain it, and
/// `controllers_only` drops everything that does not look like a controller.
pub fn visible<'a>(devices: &'a [UsbDevice], query: &str, controllers_only: bool) -> Vec<&'a UsbDevice> {
    let q = query.trim().to_lowercase();
    let mut shown: Vec<&UsbDevice> = devices
        .iter()
        .filter(|d| d.is_listed())
        .filter(|d| !controllers_only || d.kind() == Kind::Controller)
        .filter(|d| {
            q.is_empty()
                || d.name.to_lowercase().contains(&q)
                || d.manufacturer.to_lowercase().contains(&q)
                || d.vid_pid_text().to_lowercase().contains(&q)
        })
        .collect();
    shown.sort_by_key(|d| (d.kind() != Kind::Controller, d.name.to_lowercase(), d.instance_id.clone()));
    shown
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
        assert_eq!(device(r"USB\VID_04", "Short", &[]).vid_pid(), None);
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
    fn a_keyboard_or_a_mouse_is_never_taken_for_a_controller() {
        let boot = device(r"USB\VID_1&PID_2\1", "USB Input Device", &["USB\\CLASS_03&SUBCLASS_01&PROT_01"]);
        assert_eq!(boot.kind(), Kind::Input);
        assert_eq!(device(r"USB\VID_1&PID_2\1", "Gaming Mouse", &[]).kind(), Kind::Input);
        assert_eq!(
            device(r"USB\VID_1&PID_2\1", "Thrustmaster T.16000M FCS", &[]).kind(),
            Kind::Controller
        );
        assert_eq!(
            device(r"USB\VID_1&PID_2\1", "Controller (XBOX 360 For Windows)", &[]).kind(),
            Kind::Controller
        );
        assert_eq!(device(r"USB\VID_1&PID_2\1", "USB Mass Storage", &[]).kind(), Kind::Other);
    }

    #[test]
    fn controllers_come_first_and_the_search_narrows_the_list() {
        let list = vec![
            device(r"USB\VID_0001&PID_0001\1", "Webcam", &[]),
            device(r"USB\VID_0002&PID_0002\1", "Flight Stick", &[]),
            device(r"USB\ROOT_HUB30\1", "USB Root Hub", &[]),
            device(r"USB\VID_0003&PID_0003&MI_00\1", "Flight Stick (interface)", &[]),
        ];
        let names = |v: Vec<&UsbDevice>| v.iter().map(|d| d.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(visible(&list, "", false)), ["Flight Stick", "Webcam"]);
        assert_eq!(names(visible(&list, "", true)), ["Flight Stick"]);
        assert_eq!(names(visible(&list, "cam", false)), ["Webcam"]);
        assert_eq!(names(visible(&list, "0002:0002", false)), ["Flight Stick"]);
    }
}
