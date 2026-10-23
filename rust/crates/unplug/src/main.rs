// Copyright (C) 2026 the Unplug contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use eframe::egui;
use std::sync::Arc;
use unplug::app::{UsbApp, WINDOW};
use unplug::backend::{self, Backend, Demo};
use unplug::prefs::Prefs;
use unplug::ui::theme;
use unplug::winapi::{self, Tray, TrayEvent, WINDOW_TITLE};

fn main() -> eframe::Result {
    // --demo: a made-up list that changes in memory only, to try the window without risking a real device
    let demo = std::env::args().any(|a| a == "--demo");
    if !demo && winapi::already_running() {
        return Ok(());
    }
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(WINDOW_TITLE)
        .with_inner_size(WINDOW)
        .with_resizable(false)
        .with_maximize_button(false);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/app.png")) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    let backend: Arc<dyn Backend> = if demo {
        Arc::new(Demo::new())
    } else {
        Arc::from(backend::system())
    };
    let prefs = match (demo, Prefs::default_path()) {
        (false, Some(path)) => Prefs::load(path),
        _ => Prefs::in_memory(),
    };
    eframe::run_native(
        WINDOW_TITLE,
        options,
        Box::new(move |cc| {
            theme::install_fonts(&cc.egui_ctx);
            theme::install(&cc.egui_ctx, theme::ACCENT);
            let ctx = cc.egui_ctx.clone();
            let tray_backend = backend.clone();
            let tray = Tray::start(Arc::new(move |event| match event {
                TrayEvent::Open => {
                    winapi::show_window();
                    ctx.request_repaint();
                }
                TrayEvent::UnblockAll => {
                    let backend = tray_backend.clone();
                    std::thread::spawn(move || {
                        for d in backend.list().unwrap_or_default().into_iter().filter(|d| d.blocked) {
                            let _ = backend.set_blocked(&d.instance_id, false);
                        }
                    });
                }
                TrayEvent::Exit => std::process::exit(0),
            }));
            Ok(Box::new(UsbApp::new(cc.egui_ctx.clone(), backend, prefs, tray)))
        }),
    )
}
