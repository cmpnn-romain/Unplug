// Copyright (C) 2026 the Unplug contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
//! The few things that are the operating system's own: the notification-area icon with its menu (drawn like the Cairn launcher's), bringing the
//! window back from the tray, and keeping a second copy from starting. Windows-only; elsewhere these do nothing, so the rest builds and tests.

use std::sync::Arc;

/// What the tray icon asks of the tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    Open,
    CheckUpdates,
    UnblockAll,
    Exit,
}

/// Called on the tray's own thread.
pub type TrayHandler = Arc<dyn Fn(TrayEvent) + Send + Sync>;

/// The title of the window: how the tray finds it to show it again.
pub const WINDOW_TITLE: &str = "Unplug";

#[cfg(windows)]
pub use imp::*;
#[cfg(not(windows))]
pub use other::*;

#[cfg(not(windows))]
mod other {
    use super::TrayHandler;

    pub struct Tray;
    impl Tray {
        pub fn start(_handler: TrayHandler) -> Option<Self> {
            None
        }
    }
    pub fn show_window() {}
    /// False: nothing was hidden (the caller hides the window itself).
    pub fn hide_window() -> bool {
        false
    }
    pub fn already_running(_wait: bool) -> bool {
        false
    }
}

#[cfg(windows)]
mod imp {
    use super::{TrayEvent, TrayHandler, WINDOW_TITLE};
    use std::ptr::{null, null_mut};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND};
    use windows_sys::Win32::Graphics::Gdi::{
        CreateFontW, CreateRoundRectRgn, CreateSolidBrush, DeleteObject, DrawTextW, FillRect, GetDC, GetDeviceCaps, GetStockObject,
        ReleaseDC, RoundRect, SelectObject, SetBkMode, SetTextColor, SetWindowRgn, DT_LEFT, DT_SINGLELINE, DT_VCENTER, LOGPIXELSY,
        NULL_PEN,
    };
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Threading::{CreateMutexW, GetCurrentThreadId};
    use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
    use windows_sys::Win32::UI::Controls::{DRAWITEMSTRUCT, MEASUREITEMSTRUCT};
    use windows_sys::Win32::UI::Shell::{Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW};
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn copy_into(dst: &mut [u16], s: &str) {
        let n = dst.len().saturating_sub(1);
        for (i, c) in s.encode_utf16().take(n).enumerate() {
            dst[i] = c;
        }
    }

    /// The tool's window: the toolkit's window class and the title, so that a folder or a page that happens to be called Unplug is not taken for it.
    unsafe fn find_window() -> HWND {
        FindWindowW(wide("Window Class").as_ptr(), wide(WINDOW_TITLE).as_ptr())
    }

    /// Brings the window back from the tray (it was hidden when it was closed) and to the front.
    pub fn show_window() {
        unsafe {
            let hwnd = find_window();
            if !hwnd.is_null() {
                ShowWindow(hwnd, SW_SHOW);
                ShowWindow(hwnd, SW_RESTORE);
                SetForegroundWindow(hwnd);
            }
        }
    }

    /// Hides the window to the tray. It is hidden the way it is shown again: through Windows and not through the toolkit, which would go on thinking
    /// the window is hidden after the tray showed it, and ignore the next time it is asked to hide it.
    pub fn hide_window() -> bool {
        unsafe {
            let hwnd = find_window();
            !hwnd.is_null() && ShowWindow(hwnd, SW_HIDE) != -1
        }
    }

    /// True when another copy is running: it is brought to the front and this one should end. `wait` is for a copy started by an update: the one that
    /// started it is still ending, so it is given a few seconds to let go before the other is taken for a second copy.
    pub fn already_running(wait: bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            unsafe {
                let handle = CreateMutexW(null(), 0, wide("Local\\Unplug-single-instance").as_ptr());
                if GetLastError() != ERROR_ALREADY_EXISTS {
                    // the handle stays open until the process ends, which is what makes the name taken
                    return false;
                }
                CloseHandle(handle);
                if !wait || Instant::now() >= deadline {
                    show_window();
                    return true;
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    const CALLBACK: u32 = WM_APP + 1;
    const CMD_OPEN: usize = 1;
    const CMD_UNBLOCK: usize = 2;
    const CMD_UPDATES: usize = 3;
    const CMD_EXIT: usize = 4;

    /// How a menu row is drawn: the menu is owner-drawn so it wears the launcher's colours, not the system's.
    #[derive(Clone, Copy, PartialEq)]
    enum Row {
        Item,
        Danger,
        Line,
    }

    /// The tray menu, top to bottom: (command, text, how it is drawn).
    const MENU: [(usize, &str, Row); 5] = [
        (CMD_OPEN, "Open Unplug", Row::Item),
        (CMD_UNBLOCK, "Unblock all", Row::Item),
        (CMD_UPDATES, "Check for updates", Row::Item),
        (0, "", Row::Line),
        (CMD_EXIT, "Exit", Row::Danger),
    ];

    const ODS_SELECTED: u32 = 1;
    const MENU_WIDTH: i32 = 184;

    /// A colour as the GDI wants it (blue, green, red).
    const fn rgb(r: u8, g: u8, b: u8) -> u32 {
        (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
    }
    const MENU_BG: u32 = rgb(0x1D, 0x21, 0x25);
    const MENU_HOVER: u32 = rgb(0x2B, 0x30, 0x36);
    const MENU_TEXT: u32 = rgb(0xFB, 0xFB, 0xFA);
    const MENU_DANGER: u32 = rgb(0xF8, 0x71, 0x71);
    const MENU_LINE: u32 = rgb(0x34, 0x39, 0x3F);

    unsafe fn dpi_scale() -> f32 {
        let dc = GetDC(null_mut());
        let dpi = GetDeviceCaps(dc, LOGPIXELSY as i32);
        ReleaseDC(null_mut(), dc);
        if dpi > 0 {
            dpi as f32 / 96.0
        } else {
            1.0
        }
    }

    unsafe fn measure_row(m: &mut MEASUREITEMSTRUCT) {
        let scale = dpi_scale();
        let height = match MENU.get(m.itemData).map(|r| r.2) {
            Some(Row::Line) => 9,
            _ => 30,
        };
        m.itemWidth = (MENU_WIDTH as f32 * scale) as u32;
        m.itemHeight = (height as f32 * scale) as u32;
    }

    unsafe fn draw_row(d: &DRAWITEMSTRUCT) {
        let Some(&(_, text, row)) = MENU.get(d.itemData) else { return };
        let scale = dpi_scale();
        let px = |v: f32| (v * scale).round() as i32;
        let hot = d.itemState & ODS_SELECTED != 0 && matches!(row, Row::Item | Row::Danger);
        let back = CreateSolidBrush(MENU_BG);
        FillRect(d.hDC, &d.rcItem, back);
        DeleteObject(back);
        if row == Row::Line {
            let line = CreateSolidBrush(MENU_LINE);
            let y = (d.rcItem.top + d.rcItem.bottom) / 2;
            let r = RECT {
                left: d.rcItem.left + px(10.0),
                top: y,
                right: d.rcItem.right - px(10.0),
                bottom: y + 1.max(px(1.0)),
            };
            FillRect(d.hDC, &r, line);
            DeleteObject(line);
            return;
        }
        if hot {
            // a soft rounded highlight, inset from the edges
            let hover = CreateSolidBrush(MENU_HOVER);
            let old_brush = SelectObject(d.hDC, hover);
            let old_pen = SelectObject(d.hDC, GetStockObject(NULL_PEN));
            RoundRect(
                d.hDC,
                d.rcItem.left + px(4.0),
                d.rcItem.top + px(1.0),
                d.rcItem.right - px(4.0),
                d.rcItem.bottom - px(1.0),
                px(8.0),
                px(8.0),
            );
            SelectObject(d.hDC, old_pen);
            SelectObject(d.hDC, old_brush);
            DeleteObject(hover);
        }
        let font = CreateFontW(-px(14.0), 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, wide("Segoe UI").as_ptr());
        let old = SelectObject(d.hDC, font);
        SetBkMode(d.hDC, 1);
        SetTextColor(
            d.hDC,
            match row {
                Row::Danger => MENU_DANGER,
                _ => MENU_TEXT,
            },
        );
        let mut r = RECT {
            left: d.rcItem.left + px(16.0),
            ..d.rcItem
        };
        let w = wide(text);
        DrawTextW(d.hDC, w.as_ptr(), -1, &mut r, DT_SINGLELINE | DT_VCENTER | DT_LEFT);
        SelectObject(d.hDC, old);
        DeleteObject(font);
    }

    /// Rounds the corners of the window Windows made for the menu: the corner preference of Windows 11, or, where that is not known (Windows 10), a
    /// window region with rounded corners.
    unsafe fn round_menu_window(hwnd: HWND) {
        let mut name = [0u16; 16];
        let n = GetClassNameW(hwnd, name.as_mut_ptr(), name.len() as i32);
        if String::from_utf16_lossy(&name[..n.max(0) as usize]) != "#32768" {
            return;
        }
        let preference = DWMWCP_ROUND;
        let hr = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE as u32, &preference as *const _ as *const _, 4);
        if hr != 0 {
            let mut r: RECT = std::mem::zeroed();
            GetWindowRect(hwnd, &mut r);
            let radius = (12.0 * dpi_scale()).round() as i32;
            let region = CreateRoundRectRgn(0, 0, r.right - r.left + 1, r.bottom - r.top + 1, radius, radius);
            SetWindowRgn(hwnd, region, 1);
        }
    }

    unsafe extern "system" fn menu_shown(
        _hook: HWINEVENTHOOK,
        _event: u32,
        hwnd: HWND,
        _object: i32,
        _child: i32,
        _thread: u32,
        _time: u32,
    ) {
        if !hwnd.is_null() {
            round_menu_window(hwnd);
        }
    }

    unsafe fn tray_menu() -> HMENU {
        let menu = CreatePopupMenu();
        for (i, (cmd, _, _)) in MENU.iter().enumerate() {
            let flags = MF_OWNERDRAW | if *cmd == 0 { MF_GRAYED } else { 0 };
            AppendMenuW(menu, flags, *cmd, i as *const u16);
        }
        let back = CreateSolidBrush(MENU_BG);
        let info = MENUINFO {
            cbSize: std::mem::size_of::<MENUINFO>() as u32,
            fMask: MIM_BACKGROUND,
            dwStyle: 0,
            cyMax: 0,
            hbrBack: back,
            dwContextHelpID: 0,
            dwMenuData: 0,
        };
        SetMenuInfo(menu, &info);
        menu
    }

    static HANDLER: Mutex<Option<TrayHandler>> = Mutex::new(None);
    static TRAY_THREAD: AtomicU32 = AtomicU32::new(0);

    fn send(e: TrayEvent) {
        let handler = HANDLER.lock().ok().and_then(|g| g.clone());
        if let Some(h) = handler {
            h(e);
        }
    }

    fn data(hwnd: HWND) -> NOTIFYICONDATAW {
        let mut d: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        d.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        d.hWnd = hwnd;
        d.uID = 1;
        d
    }

    unsafe extern "system" fn tray_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if msg == CALLBACK {
            match (lparam & 0xFFFF) as u32 {
                WM_LBUTTONUP => send(TrayEvent::Open),
                WM_RBUTTONUP => {
                    let menu = tray_menu();
                    let mut at = POINT { x: 0, y: 0 };
                    GetCursorPos(&mut at);
                    SetForegroundWindow(hwnd);
                    // told when Windows has made the menu's window, to round its corners (the menu loop below runs until it closes)
                    let hook = SetWinEventHook(
                        EVENT_SYSTEM_MENUPOPUPSTART,
                        EVENT_SYSTEM_MENUPOPUPSTART,
                        null_mut(),
                        Some(menu_shown),
                        0,
                        GetCurrentThreadId(),
                        WINEVENT_OUTOFCONTEXT,
                    );
                    let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON, at.x, at.y, 0, hwnd, null());
                    if !hook.is_null() {
                        UnhookWinEvent(hook);
                    }
                    DestroyMenu(menu);
                    match cmd as usize {
                        CMD_OPEN => send(TrayEvent::Open),
                        CMD_UNBLOCK => send(TrayEvent::UnblockAll),
                        CMD_UPDATES => send(TrayEvent::CheckUpdates),
                        CMD_EXIT => {
                            // the icon goes first, or it stays in the tray until the mouse passes over it
                            Shell_NotifyIconW(NIM_DELETE, &data(hwnd));
                            send(TrayEvent::Exit);
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            return 0;
        }
        match msg {
            WM_MEASUREITEM => {
                measure_row(&mut *(lparam as *mut MEASUREITEMSTRUCT));
                return 1;
            }
            WM_DRAWITEM => {
                draw_row(&*(lparam as *const DRAWITEMSTRUCT));
                return 1;
            }
            _ => {}
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }

    pub struct Tray;

    impl Tray {
        /// Adds the icon from a thread of its own; `None` when there is no tray (a restricted shell): the tool works without it.
        pub fn start(handler: TrayHandler) -> Option<Self> {
            *HANDLER.lock().ok()? = Some(handler);
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || unsafe {
                let hinst = GetModuleHandleW(null());
                let class = wide("UnplugTrayWindow");
                let wc = WNDCLASSW {
                    style: 0,
                    lpfnWndProc: Some(tray_proc),
                    cbClsExtra: 0,
                    cbWndExtra: 0,
                    hInstance: hinst,
                    hIcon: null_mut(),
                    hCursor: null_mut(),
                    hbrBackground: null_mut(),
                    lpszMenuName: null(),
                    lpszClassName: class.as_ptr(),
                };
                RegisterClassW(&wc);
                let hwnd = CreateWindowExW(
                    0,
                    class.as_ptr(),
                    wide("Unplug tray").as_ptr(),
                    0,
                    0,
                    0,
                    0,
                    0,
                    HWND_MESSAGE,
                    null_mut(),
                    hinst,
                    null(),
                );
                if hwnd.is_null() {
                    let _ = tx.send(false);
                    return;
                }
                // the executable's own icon (resource 1) when it has one, else the system's
                let mut icon = LoadIconW(hinst, std::ptr::without_provenance::<u16>(1));
                if icon.is_null() {
                    icon = LoadIconW(null_mut(), IDI_APPLICATION);
                }
                let mut d = data(hwnd);
                d.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
                d.uCallbackMessage = CALLBACK;
                d.hIcon = icon;
                copy_into(&mut d.szTip, "Unplug");
                let ok = Shell_NotifyIconW(NIM_ADD, &d) != 0;
                TRAY_THREAD.store(GetCurrentThreadId(), Ordering::SeqCst);
                let _ = tx.send(ok);
                if !ok {
                    return;
                }
                let mut msg: MSG = std::mem::zeroed();
                while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                Shell_NotifyIconW(NIM_DELETE, &d);
                DestroyWindow(hwnd);
            });
            rx.recv().ok().filter(|ok| *ok).map(|_| Self)
        }
    }

    impl Drop for Tray {
        fn drop(&mut self) {
            let t = TRAY_THREAD.load(Ordering::SeqCst);
            if t != 0 {
                unsafe {
                    PostThreadMessageW(t, WM_QUIT, 0, 0);
                }
            }
        }
    }
}
