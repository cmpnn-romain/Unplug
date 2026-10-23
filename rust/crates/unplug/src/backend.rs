// Copyright (C) 2026 the Unplug contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
//! Where the devices come from and how one is turned off: Windows' device installer (the same call as Device Manager's "Disable device"),
//! or a demo list for trying the window out on a machine without USB devices to lose.

use crate::device::UsbDevice;
use std::sync::Mutex;

pub trait Backend: Send + Sync {
    /// Every USB device that is plugged in, turned off or not.
    fn list(&self) -> Result<Vec<UsbDevice>, String>;
    /// Turns the device off (`blocked`) or back on.
    fn set_blocked(&self, instance_id: &str, blocked: bool) -> Result<(), String>;
}

/// The machine's own devices: Windows only.
pub fn system() -> Box<dyn Backend> {
    #[cfg(windows)]
    {
        Box::new(windows::Setup)
    }
    #[cfg(not(windows))]
    {
        Box::new(Unsupported)
    }
}

#[cfg(not(windows))]
struct Unsupported;

#[cfg(not(windows))]
impl Backend for Unsupported {
    fn list(&self) -> Result<Vec<UsbDevice>, String> {
        Err("Unplug works on Windows only. Run it with --demo to try the window.".into())
    }
    fn set_blocked(&self, _: &str, _: bool) -> Result<(), String> {
        Err("Unplug works on Windows only.".into())
    }
}

/// A made-up list that changes in memory only.
pub struct Demo(Mutex<Vec<UsbDevice>>);

impl Demo {
    pub fn new() -> Self {
        let d = |id: &str, name: &str, maker: &str, ids: &[&str]| UsbDevice {
            instance_id: id.into(),
            name: name.into(),
            manufacturer: maker.into(),
            ids: ids.iter().map(|s| s.to_string()).collect(),
            blocked: false,
        };
        let mut devices = vec![
            d(r"USB\VID_044F&PID_B10A\6&1A2B&0&1", "T.16000M FCS Joystick", "Thrustmaster", &[]),
            d(r"USB\VID_044F&PID_B687\6&1A2B&0&2", "TWCS Throttle", "Thrustmaster", &[]),
            d(
                r"USB\VID_045E&PID_028E\6&1A2B&0&3",
                "Controller (XBOX 360 For Windows)",
                "Microsoft",
                &[],
            ),
            d(r"USB\VID_231D&PID_0136\6&1A2B&0&4", "VKB Gladiator NXT EVO", "VKB-Sim", &[]),
            d(
                r"USB\VID_044F&PID_0402\6&1A2B&0&0",
                "Thrustmaster HOTAS Warthog Dual Throttle and Joystick Combination Set",
                "Thrustmaster, Inc.",
                &[],
            ),
            // a keyboard that reports itself as a composite device; only its interface says it is a keyboard
            d(
                r"USB\VID_1B1C&PID_2B01\6&1A2B&0&5",
                "USB Composite Device",
                "Corsair",
                &["USB\\COMPOSITE"],
            ),
            d(
                r"USB\VID_1B1C&PID_2B01&MI_00\7&1&0&0000",
                "USB Input Device",
                "",
                &["USB\\CLASS_03&SUBCLASS_01&PROT_01"],
            ),
            d(
                r"USB\VID_046D&PID_C077\6&1A2B&0&6",
                "USB Optical Mouse",
                "Logitech",
                &["USB\\CLASS_03&SUBCLASS_01&PROT_02"],
            ),
            d(
                r"USB\VID_0781&PID_5581\6&1A2B&0&7",
                "SanDisk Ultra",
                "SanDisk",
                &["USB\\CLASS_08&SUBCLASS_06"],
            ),
            d(r"USB\VID_0C45&PID_6366\6&1A2B&0&8", "Integrated Webcam", "Sonix", &[]),
            d(
                r"USB\VID_1532&PID_0521\6&1A2B&0&9",
                "USB Headset",
                "Razer",
                &["USB\\CLASS_01&SUBCLASS_01"],
            ),
        ];
        // one is already blocked, to show how that looks
        if let Some(stick) = devices.iter_mut().find(|d| d.name.starts_with("VKB")) {
            stick.blocked = true;
        }
        Demo(Mutex::new(devices))
    }
}

impl Default for Demo {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for Demo {
    fn list(&self) -> Result<Vec<UsbDevice>, String> {
        Ok(self.0.lock().map_err(|e| e.to_string())?.clone())
    }
    fn set_blocked(&self, instance_id: &str, blocked: bool) -> Result<(), String> {
        std::thread::sleep(std::time::Duration::from_millis(400));
        let mut all = self.0.lock().map_err(|e| e.to_string())?;
        let d = all
            .iter_mut()
            .find(|d| d.instance_id == instance_id)
            .ok_or("That device is gone.")?;
        d.blocked = blocked;
        Ok(())
    }
}

#[cfg(windows)]
mod windows {
    use super::Backend;
    use crate::device::UsbDevice;
    use std::mem::{size_of, zeroed};
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::*;
    use windows_sys::Win32::Devices::Properties::{DEVPKEY_Device_BusReportedDeviceDesc, DEVPROPKEY};
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_ACCESS_DENIED, ERROR_NO_MORE_ITEMS, INVALID_HANDLE_VALUE};

    pub struct Setup;

    /// A device information set that is destroyed when it goes out of scope.
    struct InfoSet(HDEVINFO);

    impl Drop for InfoSet {
        fn drop(&mut self) {
            unsafe { SetupDiDestroyDeviceInfoList(self.0) };
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn text(buf: &[u16]) -> String {
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..end])
    }

    /// A list of strings ended by an empty one, as the registry stores several.
    fn multi(buf: &[u16]) -> Vec<String> {
        buf.split(|&c| c == 0)
            .filter(|s| !s.is_empty())
            .map(String::from_utf16_lossy)
            .collect()
    }

    fn last_error(what: &str) -> String {
        let code = unsafe { GetLastError() };
        if code == ERROR_ACCESS_DENIED {
            format!("{what}: access denied. Run Unplug as administrator.")
        } else {
            format!("{what} (Windows error {code}).")
        }
    }

    fn registry_property(set: HDEVINFO, data: &SP_DEVINFO_DATA, property: u32) -> Vec<u16> {
        let mut buf = vec![0u16; 1024];
        let mut kind = 0u32;
        let mut needed = 0u32;
        let ok = unsafe {
            SetupDiGetDeviceRegistryPropertyW(
                set,
                data,
                property,
                &mut kind,
                buf.as_mut_ptr() as *mut u8,
                (buf.len() * 2) as u32,
                &mut needed,
            )
        };
        if ok == 0 {
            buf.clear();
        }
        buf
    }

    fn bus_reported_name(set: HDEVINFO, data: &SP_DEVINFO_DATA) -> String {
        let mut buf = vec![0u16; 512];
        let mut kind = 0u32;
        let mut needed = 0u32;
        let key: &DEVPROPKEY = &DEVPKEY_Device_BusReportedDeviceDesc;
        let ok = unsafe {
            SetupDiGetDevicePropertyW(
                set,
                data,
                key,
                &mut kind,
                buf.as_mut_ptr() as *mut u8,
                (buf.len() * 2) as u32,
                &mut needed,
                0,
            )
        };
        if ok == 0 {
            String::new()
        } else {
            text(&buf)
        }
    }

    fn is_blocked(data: &SP_DEVINFO_DATA) -> bool {
        let mut status = 0u32;
        let mut problem = 0u32;
        let r = unsafe { CM_Get_DevNode_Status(&mut status, &mut problem, data.DevInst, 0) };
        r == 0 && status & DN_HAS_PROBLEM != 0 && problem == CM_PROB_DISABLED
    }

    impl Backend for Setup {
        fn list(&self) -> Result<Vec<UsbDevice>, String> {
            let enumerator = wide("USB");
            let raw = unsafe { SetupDiGetClassDevsW(null(), enumerator.as_ptr(), null_mut(), DIGCF_PRESENT | DIGCF_ALLCLASSES) };
            if raw as isize == INVALID_HANDLE_VALUE as isize {
                return Err(last_error("The USB devices could not be listed"));
            }
            let set = InfoSet(raw);
            let mut found = Vec::new();
            let mut index = 0;
            loop {
                let mut data: SP_DEVINFO_DATA = unsafe { zeroed() };
                data.cbSize = size_of::<SP_DEVINFO_DATA>() as u32;
                if unsafe { SetupDiEnumDeviceInfo(set.0, index, &mut data) } == 0 {
                    if unsafe { GetLastError() } == ERROR_NO_MORE_ITEMS {
                        break;
                    }
                    return Err(last_error("The USB devices could not be listed"));
                }
                index += 1;
                let mut id = vec![0u16; 512];
                let mut needed = 0u32;
                if unsafe { SetupDiGetDeviceInstanceIdW(set.0, &data, id.as_mut_ptr(), id.len() as u32, &mut needed) } == 0 {
                    continue;
                }
                let name = [
                    bus_reported_name(set.0, &data),
                    text(&registry_property(set.0, &data, SPDRP_FRIENDLYNAME)),
                    text(&registry_property(set.0, &data, SPDRP_DEVICEDESC)),
                ]
                .into_iter()
                .find(|n| !n.is_empty())
                .unwrap_or_else(|| "USB device".into());
                // "@oem12.inf,%mfg%;Logitech": what follows the last semicolon is the plain name
                let manufacturer = text(&registry_property(set.0, &data, SPDRP_MFG));
                let manufacturer = manufacturer.rsplit(';').next().unwrap_or("").to_string();
                let mut ids = multi(&registry_property(set.0, &data, SPDRP_HARDWAREID));
                ids.extend(multi(&registry_property(set.0, &data, SPDRP_COMPATIBLEIDS)));
                found.push(UsbDevice {
                    instance_id: text(&id),
                    name,
                    manufacturer,
                    ids: ids.into_iter().map(|i| i.to_ascii_uppercase()).collect(),
                    blocked: is_blocked(&data),
                });
            }
            Ok(found)
        }

        fn set_blocked(&self, instance_id: &str, blocked: bool) -> Result<(), String> {
            let raw = unsafe { SetupDiCreateDeviceInfoList(null(), null_mut()) };
            if raw as isize == INVALID_HANDLE_VALUE as isize {
                return Err(last_error("The device could not be opened"));
            }
            let set = InfoSet(raw);
            let mut data: SP_DEVINFO_DATA = unsafe { zeroed() };
            data.cbSize = size_of::<SP_DEVINFO_DATA>() as u32;
            let id = wide(instance_id);
            if unsafe { SetupDiOpenDeviceInfoW(set.0, id.as_ptr(), null_mut(), 0, &mut data) } == 0 {
                return Err(last_error("The device could not be opened"));
            }
            let mut params: SP_PROPCHANGE_PARAMS = unsafe { zeroed() };
            params.ClassInstallHeader.cbSize = size_of::<SP_CLASSINSTALL_HEADER>() as u32;
            params.ClassInstallHeader.InstallFunction = DIF_PROPERTYCHANGE;
            params.StateChange = if blocked { DICS_DISABLE } else { DICS_ENABLE };
            params.Scope = DICS_FLAG_GLOBAL;
            params.HwProfile = 0;
            let set_params = unsafe {
                SetupDiSetClassInstallParamsW(set.0, &data, &params.ClassInstallHeader, size_of::<SP_PROPCHANGE_PARAMS>() as u32)
            };
            if set_params == 0 {
                return Err(last_error("The device could not be changed"));
            }
            if unsafe { SetupDiCallClassInstaller(DIF_PROPERTYCHANGE, set.0, &data) } == 0 {
                return Err(last_error(if blocked {
                    "The device could not be blocked"
                } else {
                    "The device could not be unblocked"
                }));
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_demo_blocks_and_unblocks_a_device() {
        let demo = Demo::new();
        let id = demo.list().unwrap()[0].instance_id.clone();
        demo.set_blocked(&id, true).unwrap();
        assert!(demo.list().unwrap()[0].blocked);
        demo.set_blocked(&id, false).unwrap();
        assert!(!demo.list().unwrap()[0].blocked);
        assert!(demo.set_blocked("nope", true).is_err());
    }
}
