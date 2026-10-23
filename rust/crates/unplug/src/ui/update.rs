// Copyright (C) 2026 the Unplug contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
//! Unplug's own update, in a window (the launcher's): what is new, then the download's progress, then (if it fails) why.

use super::theme;
use super::widgets::{centered_buttons, dialog, dialog_heading, primary_button, secondary_button};
use crate::app::{Msg, UpdateModal, UsbApp};
use crate::update_check::{self, Check};
use eframe::egui::{self, Align, Layout, RichText};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

impl UsbApp {
    pub fn open_update_modal(&mut self, stage: &str, text: &str) {
        self.update_modal = Some(UpdateModal {
            stage: stage.to_string(),
            text: text.to_string(),
            progress: None,
            cancel: Arc::new(AtomicBool::new(false)),
        });
    }

    pub fn update_failed(&mut self, text: &str) {
        match self.update_modal.as_mut() {
            Some(m) => {
                m.stage = "error".into();
                m.text = text.to_string();
                m.progress = None;
            }
            None => self.error = Some(text.to_string()),
        }
    }

    /// Looks for a newer release on a thread of its own. `open` is for the player's own request: "up to date" and "can't check" are then said out
    /// loud, where at start only a newer release is.
    pub fn check_for_update(&mut self, open: bool) {
        if self.update_checking || self.update_modal.as_ref().is_some_and(|m| m.stage == "busy") {
            return;
        }
        self.update_checking = true;
        let (tx, http, ctx) = (self.tx.clone(), self.http.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(Msg::UpdateChecked(update_check::check(http.as_ref(), crate::VERSION), open));
            ctx.request_repaint();
        });
    }

    /// Takes in the answer of a look for an update.
    pub fn update_checked(&mut self, check: Check, open: bool) {
        self.update_checking = false;
        match check {
            Check::Newer(info) => {
                self.update = Some(info);
                self.open_update_modal("offer", "");
            }
            Check::Current => {
                self.update = None;
                if open {
                    self.open_update_modal("current", "");
                }
            }
            Check::Unreachable => {
                if open {
                    self.open_update_modal("unreachable", "");
                }
            }
        }
    }

    /// Downloads the exe on a worker thread; when it is whole and checked Unplug hands over to it.
    fn run_update(&mut self) {
        let Some(info) = self.update.clone() else { return };
        if info.download_url.is_none() || info.sha256.is_none() {
            self.update_failed("This release has no exe file Unplug can check. Download it from the release page.");
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.update_modal = Some(UpdateModal {
            stage: "busy".into(),
            text: "Starting...".into(),
            progress: None,
            cancel: cancel.clone(),
        });
        let (tx, http, ctx) = (self.tx.clone(), self.http.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let (tx2, ctx2) = (tx.clone(), ctx.clone());
            let result = update_check::download(
                &info,
                http.as_ref(),
                &mut |p| {
                    let _ = tx2.send(Msg::UpdateProgress(p));
                    ctx2.request_repaint();
                },
                &cancel,
            );
            let _ = tx.send(Msg::UpdateDownloaded(result));
            ctx.request_repaint();
        });
    }

    pub fn update_window(&mut self, ctx: &egui::Context) {
        let Some(modal) = self.update_modal.clone() else { return };
        let tag = self.update.as_ref().map(|u| u.tag.clone()).unwrap_or_default();
        let (title, line) = match modal.stage.as_str() {
            "busy" => ("Updating".to_string(), String::new()),
            "error" => ("Update failed".to_string(), modal.text.clone()),
            "current" => (
                "Unplug is up to date".to_string(),
                format!("You have the newest version, {}.", crate::VERSION),
            ),
            "unreachable" => (
                "Can't check for updates".to_string(),
                "GitHub could not be reached. Check your connection and try again.".to_string(),
            ),
            _ => (format!("Unplug {tag} is out"), String::new()),
        };
        let accent = theme::ACCENT;
        let width = 380.0;
        let (mut close, mut update, mut page) = (false, false, false);
        let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        dialog(ctx, "update", width, |ui| {
            dialog_heading(ui, &title, &line);
            match modal.stage.as_str() {
                "current" | "unreachable" => {
                    ui.add_space(16.0);
                    centered_buttons(ui, width, &["Close"], |ui| {
                        if primary_button(ui, "Close", accent, true).clicked() || enter {
                            close = true;
                        }
                    });
                }
                "offer" => {
                    if let Some(u) = &self.update {
                        if !u.notes.is_empty() {
                            ui.add_space(12.0);
                            egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                                    ui.add(egui::Label::new(RichText::new(&u.notes).size(14.0)).wrap());
                                });
                            });
                        }
                    }
                    ui.add_space(10.0);
                    ui.add(
                        egui::Label::new(
                            RichText::new(
                                "Unplug closes, puts the new version in its place and opens again. The devices you blocked stay blocked.",
                            )
                            .size(12.5)
                            .color(theme::MUTED),
                        )
                        .wrap(),
                    );
                    ui.add_space(16.0);
                    centered_buttons(ui, width, &["Update now", "Later"], |ui| {
                        if primary_button(ui, "Update now", accent, true).clicked() || enter {
                            update = true;
                        }
                        if secondary_button(ui, "Later", true).clicked() {
                            close = true;
                        }
                    });
                }
                "busy" => {
                    ui.add_space(8.0);
                    ui.label(RichText::new(&modal.text).color(theme::PARCHMENT_DIM));
                    ui.add_space(10.0);
                    ui.add(egui::ProgressBar::new(modal.progress.unwrap_or(0.0) as f32).fill(accent));
                    ui.add_space(8.0);
                    ui.label(RichText::new("Please do not close Unplug.").color(theme::MUTED).size(12.5));
                    ui.add_space(12.0);
                    centered_buttons(ui, width, &["Cancel"], |ui| {
                        if secondary_button(ui, "Cancel", true).clicked() {
                            modal.cancel.store(true, Ordering::Relaxed);
                        }
                    });
                }
                _ => {
                    ui.add_space(16.0);
                    centered_buttons(ui, width, &["Open the release page", "Close"], |ui| {
                        if primary_button(ui, "Open the release page", accent, true).clicked() {
                            page = true;
                        }
                        if secondary_button(ui, "Close", true).clicked() {
                            close = true;
                        }
                    });
                }
            }
        });
        if escape && modal.stage != "busy" {
            close = true;
        }
        if update {
            self.run_update();
        }
        if page {
            if let Some(u) = &self.update {
                open_url(&u.page_url);
            }
        }
        if close {
            self.update_modal = None;
        }
    }
}

/// Opens an address in the browser.
fn open_url(url: &str) {
    if !url.starts_with("https://") {
        return;
    }
    #[cfg(windows)]
    let _ = std::process::Command::new("cmd").args(["/c", "start", "", url]).spawn();
    #[cfg(not(windows))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}
