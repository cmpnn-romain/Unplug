// Copyright (C) 2026 the Unplug contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
//! The window: the list of USB devices, the search, and one Block / Unblock button per device. Windows is asked on a thread of its own, so the
//! window never waits for a driver.

use crate::backend::Backend;
use crate::device::{self, Kind, UsbDevice};
use crate::ui::theme;
use crate::ui::widgets::{card, chip, column, primary_button, secondary_button};
use eframe::egui::{self, Align, Color32, CornerRadius, Layout, RichText, Stroke};
use std::collections::HashSet;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How often the list is read again, so that a device plugged in later shows up.
const REFRESH: Duration = Duration::from_secs(3);

enum Msg {
    Listed(Result<Vec<UsbDevice>, String>),
    Changed {
        id: String,
        blocked: bool,
        result: Result<(), String>,
    },
}

pub struct UsbApp {
    backend: Arc<dyn Backend>,
    ctx: egui::Context,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    devices: Vec<UsbDevice>,
    loaded: bool,
    listing: bool,
    last_list: Instant,
    /// What went wrong the last time, shown above the list.
    error: Option<String>,
    query: String,
    controllers_only: bool,
    /// Devices Windows is turning on or off right now.
    working: HashSet<String>,
    /// The device waiting for the player to confirm that it may be blocked.
    confirm: Option<String>,
}

impl UsbApp {
    pub fn new(ctx: egui::Context, backend: Arc<dyn Backend>) -> Self {
        let (tx, rx) = channel();
        let mut app = UsbApp {
            backend,
            ctx,
            tx,
            rx,
            devices: Vec::new(),
            loaded: false,
            listing: false,
            last_list: Instant::now(),
            error: None,
            query: String::new(),
            controllers_only: false,
            working: HashSet::new(),
            confirm: None,
        };
        app.refresh();
        app
    }

    /// Reads the devices again, unless a read is already running.
    pub fn refresh(&mut self) {
        if self.listing {
            return;
        }
        self.listing = true;
        self.last_list = Instant::now();
        let (backend, tx, ctx) = (self.backend.clone(), self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Listed(backend.list()));
            ctx.request_repaint();
        });
    }

    /// Turns a device off (`blocked`) or back on.
    pub fn set_blocked(&mut self, id: &str, blocked: bool) {
        if !self.working.insert(id.to_string()) {
            return;
        }
        let (backend, tx, ctx, id) = (self.backend.clone(), self.tx.clone(), self.ctx.clone(), id.to_string());
        std::thread::spawn(move || {
            let result = backend.set_blocked(&id, blocked);
            let _ = tx.send(Msg::Changed { id, blocked, result });
            ctx.request_repaint();
        });
    }

    /// Takes in what the threads have found.
    pub fn receive(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Listed(Ok(devices)) => {
                    self.listing = false;
                    self.loaded = true;
                    self.devices = devices;
                }
                Msg::Listed(Err(e)) => {
                    self.listing = false;
                    self.loaded = true;
                    self.error = Some(e);
                }
                Msg::Changed { id, blocked, result } => {
                    self.working.remove(&id);
                    match result {
                        Ok(()) => {
                            if let Some(d) = self.devices.iter_mut().find(|d| d.instance_id == id) {
                                d.blocked = blocked;
                            }
                            self.error = None;
                            // read it back: Windows is the one that knows whether the device really is off
                            self.refresh();
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
            }
        }
    }

    /// Whether any device is turned off.
    fn blocked_count(&self) -> usize {
        self.devices.iter().filter(|d| d.is_listed() && d.blocked).count()
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("topbar")
            .exact_size(56.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::with_alpha(theme::INK, 0.96))
                    .inner_margin(egui::Margin::symmetric(14, 10))
                    .stroke(Stroke::new(1.0, theme::with_alpha(Color32::WHITE, 0.07))),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new("Unplug").size(16.0).strong());
                    chip(ui, &format!("v{}", crate::VERSION), theme::MUTED);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if secondary_button(ui, "Refresh", !self.listing).clicked() {
                            self.refresh();
                        }
                    });
                });
            });
    }

    fn page(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::GROUND)).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                column(ui, 900.0, 40.0, 28, |ui| {
                    ui.label(RichText::new("USB devices").size(32.0).strong());
                    ui.label(
                        RichText::new("Block a joystick or a pad and no application can read it, so it stops moving the game. Unblock it when you want it back.")
                            .color(theme::PARCHMENT_DIM),
                    );
                    ui.add_space(14.0);
                    if let Some(e) = self.error.clone() {
                        card(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.add(egui::Label::new(RichText::new(e).color(theme::WARN)).wrap());
                            });
                        });
                        ui.add_space(8.0);
                    }
                    self.toolbar(ui);
                    ui.add_space(6.0);
                    self.list(ui);
                    ui.add_space(10.0);
                    ui.add(
                        egui::Label::new(
                            RichText::new(
                                "A blocked device stays blocked after you close this window, and after a restart, until you unblock it here (or in Device Manager).",
                            )
                            .size(12.5)
                            .color(theme::MUTED),
                        )
                        .wrap(),
                    );
                });
            });
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let search = egui::TextEdit::singleline(&mut self.query)
                .hint_text("Search devices")
                .desired_width(280.0)
                .margin(egui::Margin::symmetric(12, 7))
                .background_color(theme::TRACK);
            ui.add(search);
            ui.checkbox(&mut self.controllers_only, "Controllers only");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let n = self.blocked_count();
                let label = if n > 0 {
                    format!("Unblock all ({n})")
                } else {
                    "Unblock all".to_string()
                };
                if secondary_button(ui, &label, n > 0).clicked() {
                    let ids: Vec<String> = self
                        .devices
                        .iter()
                        .filter(|d| d.is_listed() && d.blocked)
                        .map(|d| d.instance_id.clone())
                        .collect();
                    for id in ids {
                        self.set_blocked(&id, false);
                    }
                }
            });
        });
    }

    fn list(&mut self, ui: &mut egui::Ui) {
        if !self.loaded {
            ui.label(RichText::new("Looking for USB devices...").color(theme::MUTED));
            return;
        }
        let shown: Vec<UsbDevice> = device::visible(&self.devices, &self.query, self.controllers_only)
            .into_iter()
            .cloned()
            .collect();
        if shown.is_empty() {
            card(ui, |ui| {
                ui.label(RichText::new("No USB device found").size(18.0).strong());
                ui.add(
                    egui::Label::new(
                        RichText::new(if self.devices.is_empty() {
                            "Plug the joystick in, then press Refresh."
                        } else {
                            "Nothing matches the search. Clear it, or untick Controllers only."
                        })
                        .color(theme::PARCHMENT_DIM),
                    )
                    .wrap(),
                );
            });
            return;
        }
        for d in &shown {
            self.device_card(ui, d);
            ui.add_space(2.0);
        }
    }

    fn device_card(&mut self, ui: &mut egui::Ui, d: &UsbDevice) {
        let kind = d.kind();
        let working = self.working.contains(&d.instance_id);
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(&d.name).size(17.0).strong());
                    let mut sub = Vec::new();
                    if !d.manufacturer.is_empty() && !d.manufacturer.starts_with('(') {
                        sub.push(d.manufacturer.clone());
                    }
                    if !d.vid_pid_text().is_empty() {
                        sub.push(d.vid_pid_text());
                    }
                    if !sub.is_empty() {
                        ui.label(RichText::new(sub.join("  ·  ")).size(12.5).color(theme::MUTED));
                    }
                });
                ui.add_space(6.0);
                match kind {
                    Kind::Controller => chip(ui, "Controller", theme::FOCUS),
                    Kind::Input => chip(ui, "Keyboard or mouse", theme::MUTED),
                    Kind::Other => {}
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if working {
                        secondary_button(ui, "Working...", false);
                    } else if d.blocked {
                        if secondary_button(ui, "Unblock", true).clicked() {
                            self.set_blocked(&d.instance_id, false);
                        }
                    } else if kind == Kind::Input {
                        primary_button(ui, "Block", theme::ACCENT, false)
                            .on_disabled_hover_text("Blocking a keyboard or a mouse could leave you unable to use the computer.");
                    } else if primary_button(ui, "Block", theme::ACCENT, true).clicked() {
                        if kind == Kind::Controller {
                            self.set_blocked(&d.instance_id, true);
                        } else {
                            self.confirm = Some(d.instance_id.clone());
                        }
                    }
                    ui.add_space(6.0);
                    if d.blocked {
                        chip(ui, "Blocked", theme::DANGER_BRIGHT);
                    } else {
                        chip(ui, "Available", theme::ONLINE);
                    }
                });
            });
        });
    }

    /// Asks before a device that does not look like a controller is blocked.
    fn confirm_window(&mut self, ctx: &egui::Context) {
        let Some(id) = self.confirm.clone() else { return };
        let Some(d) = self.devices.iter().find(|d| d.instance_id == id).cloned() else {
            self.confirm = None;
            return;
        };
        let mut block = false;
        let mut cancel = false;
        egui::Window::new("Block this device?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .frame(
                egui::Frame::new()
                    .fill(theme::OAK)
                    .stroke(Stroke::new(1.0, theme::OAK_LIT))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(egui::Margin::same(20)),
            )
            .show(ctx, |ui| {
                ui.set_width(380.0);
                ui.label(RichText::new(&d.name).size(17.0).strong());
                ui.add_space(4.0);
                ui.add(
                    egui::Label::new(
                        RichText::new("This does not look like a joystick or a pad. Blocked, it stops working for every application until you unblock it.")
                            .color(theme::PARCHMENT_DIM),
                    )
                    .wrap(),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    block = primary_button(ui, "Block it", theme::ACCENT, true).clicked();
                    if secondary_button(ui, "Cancel", true).clicked() {
                        cancel = true;
                    }
                });
            });
        if block {
            self.set_blocked(&id, true);
            self.confirm = None;
        } else if cancel || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.confirm = None;
        }
    }
}

impl eframe::App for UsbApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.receive();
        if self.last_list.elapsed() >= REFRESH && self.working.is_empty() {
            self.refresh();
        }
        self.top_bar(ui);
        self.page(ui);
        self.confirm_window(ui.ctx());
        crate::dev::screenshot_tick(ui.ctx());
        ui.ctx().request_repaint_after(REFRESH);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Demo;

    fn wait_for(app: &mut UsbApp, done: impl Fn(&UsbApp) -> bool) {
        for _ in 0..200 {
            app.receive();
            if done(app) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("the app did not settle");
    }

    #[test]
    fn a_device_is_blocked_and_read_back() {
        let mut app = UsbApp::new(egui::Context::default(), Arc::new(Demo::new()));
        wait_for(&mut app, |a| a.loaded);
        let id = app.devices[0].instance_id.clone();
        app.set_blocked(&id, true);
        wait_for(&mut app, |a| a.working.is_empty() && !a.listing && a.devices[0].blocked);
        assert_eq!(app.blocked_count(), 1);
        app.set_blocked(&id, false);
        wait_for(&mut app, |a| a.working.is_empty() && !a.listing && !a.devices[0].blocked);
        assert_eq!(app.blocked_count(), 0);
    }
}
