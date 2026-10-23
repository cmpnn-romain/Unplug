// Copyright (C) 2026 the Unplug contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
//! The small pieces the window uses: buttons, chips and cards (the Cairn launcher's, as they are there).

use super::theme;
use eframe::egui::{self, Color32, CornerRadius, Response, Sense, Stroke, Ui, Vec2};

/// A filled button in the accent colour (the main action).
pub fn primary_button(ui: &mut Ui, text: &str, accent: Color32, enabled: bool) -> Response {
    let font = egui::FontId::proportional(13.5);
    let text = text.to_uppercase();
    let galley = ui.painter().layout_no_wrap(text.clone(), font.clone(), Color32::BLACK);
    let size = Vec2::new(galley.size().x + 32.0, 36.0);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let fill = if !enabled {
        theme::with_alpha(accent, 0.25)
    } else if resp.hovered() {
        theme::mix(accent, Color32::WHITE, 0.15)
    } else {
        accent
    };
    ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
    let color = if enabled {
        Color32::from_rgb(0x14, 0x12, 0x10)
    } else {
        theme::with_alpha(Color32::WHITE, 0.45)
    };
    let galley = ui.painter().layout_no_wrap(text, font, color);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// A quiet button: a hairline outline.
pub fn secondary_button(ui: &mut Ui, text: &str, enabled: bool) -> Response {
    let font = egui::FontId::proportional(13.5);
    let color = if enabled { theme::PARCHMENT } else { theme::MUTED_DIM };
    let galley = ui.painter().layout_no_wrap(text.to_uppercase(), font.clone(), color);
    let size = Vec2::new(galley.size().x + 32.0, 36.0);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let hovered = enabled && resp.hovered();
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(10),
        if hovered {
            theme::OAK_LIT
        } else {
            theme::with_alpha(Color32::WHITE, 0.04)
        },
    );
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(10),
        Stroke::new(1.0, theme::with_alpha(Color32::WHITE, if hovered { 0.22 } else { 0.12 })),
        egui::StrokeKind::Inside,
    );
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// A small pill of text (a state, a count).
pub fn chip(ui: &mut Ui, text: &str, color: Color32) {
    let font = egui::FontId::proportional(12.0);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font.clone(), color);
    let (rect, _) = ui.allocate_exact_size(galley.size() + Vec2::new(18.0, 10.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(255), theme::with_alpha(color, 0.14));
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
}

/// A page's column: at most `max` wide, centred, with `pad` at least on each side and `vertical` above and below.
pub fn column<R>(ui: &mut Ui, max: f32, pad: f32, vertical: i8, add: impl FnOnce(&mut Ui) -> R) -> R {
    let avail = ui.available_width();
    let width = (avail - 2.0 * pad).min(max).max(0.0);
    let left = ((avail - width) / 2.0).max(pad);
    let spacing = ui.spacing().item_spacing;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(left);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = spacing;
            ui.set_width(width);
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(0, vertical))
                .show(ui, add)
                .inner
        })
        .inner
    })
    .inner
}

/// A card: a near-black box with a white hairline.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let inner_w = (ui.available_width() - 38.0).max(0.0);
    egui::Frame::new()
        .fill(theme::INK)
        .stroke(Stroke::new(1.0, theme::with_alpha(Color32::WHITE, 0.08)))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(egui::Margin::same(18))
        .show(ui, |ui| {
            ui.set_min_width(inner_w);
            add(ui)
        })
        .inner
}
