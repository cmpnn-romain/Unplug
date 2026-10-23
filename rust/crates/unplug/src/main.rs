// Copyright (C) 2026 the Unplug contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use eframe::egui;
use std::sync::Arc;
use unplug::app::UsbApp;
use unplug::backend::{self, Backend, Demo};
use unplug::ui::theme;

fn main() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Unplug")
        .with_inner_size([980.0, 700.0])
        .with_min_inner_size([720.0, 520.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/app.png")) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    // --demo: a made-up list that changes in memory only, to try the window without risking a real device
    let backend: Arc<dyn Backend> = if std::env::args().any(|a| a == "--demo") {
        Arc::new(Demo::new())
    } else {
        Arc::from(backend::system())
    };
    eframe::run_native(
        "Unplug",
        options,
        Box::new(move |cc| {
            theme::install_fonts(&cc.egui_ctx);
            theme::install(&cc.egui_ctx, theme::ACCENT);
            Ok(Box::new(UsbApp::new(cc.egui_ctx.clone(), backend)))
        }),
    )
}
