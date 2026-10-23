// Copyright (C) 2026 the Unplug contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
//! Development aids: with `UNPLUG_SCREENSHOT=<file>` the window saves a picture of itself after a few seconds and closes, so the window can be
//! looked at without a person; `UNPLUG_TAB=<category>` opens that tab, `UNPLUG_MODAL=<stage>` an update window. Nothing happens without the variables.

use crate::device::Category;
use eframe::egui::{self, ViewportCommand};
use std::sync::atomic::{AtomicBool, Ordering};

static ASKED: AtomicBool = AtomicBool::new(false);

/// Call once per frame.
pub fn screenshot_tick(ctx: &egui::Context) {
    let Ok(path) = std::env::var("UNPLUG_SCREENSHOT") else {
        return;
    };
    let after_secs: f64 = std::env::var("UNPLUG_SCREENSHOT_AFTER")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3.0);
    ctx.request_repaint_after(std::time::Duration::from_millis(200));
    if ctx.input(|i| i.time) > after_secs && !ASKED.swap(true, Ordering::Relaxed) {
        let done = ctx.clone();
        ctx.send_viewport_cmd(ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| {
            let (w, h) = (img.width() as u32, img.height() as u32);
            if let Some(buf) = image::RgbaImage::from_raw(w, h, img.as_raw().to_vec()) {
                let _ = buf.save(&path);
            }
            done.send_viewport_cmd(ViewportCommand::Close);
            done.request_repaint();
        })));
    }
}

/// The tab to start on: the one `UNPLUG_TAB` names (`essentials`, `storage`, ...), else the controllers.
pub fn start_tab() -> Category {
    let wanted = std::env::var("UNPLUG_TAB").unwrap_or_default().to_lowercase();
    Category::ALL
        .into_iter()
        .find(|c| !wanted.is_empty() && c.label().to_lowercase().starts_with(&wanted))
        .unwrap_or(Category::Controller)
}

/// With `UNPLUG_MODAL=current|offer|busy|error` the window starts with that update window open (a made-up release for `offer`).
pub fn start_modal() -> Option<(String, Option<crate::update_check::UpdateInfo>)> {
    let stage = std::env::var("UNPLUG_MODAL").ok()?;
    let info = crate::update_check::UpdateInfo {
        tag: "0.2.0".into(),
        page_url: String::new(),
        download_url: None,
        sha256: None,
        size: 0,
        notes: "- A list of the USB devices, controllers first\n- Block and Unblock per device".into(),
    };
    Some((stage, Some(info)))
}
