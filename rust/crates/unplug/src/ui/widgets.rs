// Copyright (C) 2026 the Unplug contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
//! The small pieces the window uses: the buttons are the Cairn launcher's as they are there; the tab, the switch and the quiet link are drawn in the same way.

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

/// A tab: the one that is on has the accent colour and a line under it (the launcher's top-bar button).
pub fn tab(ui: &mut Ui, text: &str, active: bool, accent: Color32) -> Response {
    let font = egui::FontId::proportional(13.5);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE);
    let size = Vec2::new(galley.size().x + 20.0, 32.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let color = if active {
        accent
    } else if resp.hovered() {
        theme::PARCHMENT
    } else {
        theme::PARCHMENT_DIM
    };
    if active || resp.hovered() {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(8),
            theme::with_alpha(if active { accent } else { Color32::WHITE }, if active { 0.12 } else { 0.05 }),
        );
    }
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, color);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
    if active {
        let y = rect.bottom() - 1.0;
        ui.painter().line_segment(
            [egui::pos2(rect.left() + 10.0, y), egui::pos2(rect.right() - 10.0, y)],
            Stroke::new(2.0, accent),
        );
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A switch: on is the accent colour. Not clickable when `enabled` is false.
pub fn switch(ui: &mut Ui, on: bool, enabled: bool, accent: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(42.0, 24.0), if enabled { Sense::click() } else { Sense::hover() });
    let fade = if enabled { 1.0 } else { 0.4 };
    let track = if on {
        theme::with_alpha(accent, fade)
    } else {
        theme::with_alpha(theme::OAK_LIT, fade)
    };
    ui.painter().rect_filled(rect, CornerRadius::same(255), track);
    let x = ui.ctx().animate_value_with_time(resp.id, if on { 1.0 } else { 0.0 }, 0.12);
    let knob = egui::pos2(rect.left() + 12.0 + x * (rect.width() - 24.0), rect.center().y);
    let knob_color = if on {
        Color32::from_rgb(0x14, 0x12, 0x10)
    } else {
        theme::with_alpha(theme::PARCHMENT, fade)
    };
    ui.painter().circle_filled(knob, 8.5, knob_color);
    if enabled && resp.hovered() {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(255),
            Stroke::new(1.0, theme::with_alpha(Color32::WHITE, 0.25)),
            egui::StrokeKind::Inside,
        );
    }
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// Three dots: opens a menu.
pub fn dots(ui: &mut Ui) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(8), theme::with_alpha(Color32::WHITE, 0.06));
    }
    let color = if resp.hovered() { theme::PARCHMENT } else { theme::MUTED };
    for dx in [-6.0, 0.0, 6.0] {
        ui.painter().circle_filled(rect.center() + Vec2::new(dx, 0.0), 1.8, color);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Text that is a button without being drawn as one.
pub fn quiet_link(ui: &mut Ui, text: &str, enabled: bool) -> Response {
    let font = egui::FontId::proportional(13.5);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE);
    let (rect, resp) = ui.allocate_exact_size(
        galley.size() + Vec2::new(16.0, 10.0),
        if enabled { Sense::click() } else { Sense::hover() },
    );
    let color = if !enabled {
        theme::MUTED_DIM
    } else if resp.hovered() {
        theme::PARCHMENT
    } else {
        theme::PARCHMENT_DIM
    };
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, color);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}
