// Copyright (C) 2026 the Unplug contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
//! The window: the USB devices by category, one switch per device. Windows is asked on a thread of its own, so the window never waits for a
//! driver. Closing the window hides it: Unplug stays in the tray until Exit.

use crate::backend::Backend;
use crate::device::{self, Category, UsbDevice};
use crate::prefs::Prefs;
use crate::ui::theme;
use crate::ui::widgets::{dots, primary_button, quiet_link, secondary_button, switch, tab};
use eframe::egui::{self, Align, CornerRadius, Layout, RichText, Stroke};
use std::collections::HashSet;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How often the list is read again, so that a device plugged in later shows up.
const REFRESH: Duration = Duration::from_secs(3);

/// The size of the window, which never changes.
pub const WINDOW: [f32; 2] = [520.0, 600.0];

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
    prefs: Prefs,
    /// Kept so that the tray icon is taken away when the window's program ends.
    _tray: Option<crate::winapi::Tray>,
    devices: Vec<UsbDevice>,
    loaded: bool,
    listing: bool,
    last_list: Instant,
    /// What went wrong the last time, shown above the list.
    error: Option<String>,
    /// The category the player is looking at.
    tab: Category,
    /// Devices Windows is turning on or off right now.
    working: HashSet<String>,
    /// The device waiting for the player to confirm that it may be blocked.
    confirm: Option<String>,
}

impl UsbApp {
    pub fn new(ctx: egui::Context, backend: Arc<dyn Backend>, prefs: Prefs, tray: Option<crate::winapi::Tray>) -> Self {
        let (tx, rx) = channel();
        let mut app = UsbApp {
            backend,
            ctx,
            tx,
            rx,
            prefs,
            _tray: tray,
            devices: Vec::new(),
            loaded: false,
            listing: false,
            last_list: Instant::now(),
            error: None,
            tab: crate::dev::start_tab(),
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
                    self.devices = device::group(devices);
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

    /// The category of a device: the one the player chose, or the one it looks like.
    pub fn category_of(&self, d: &UsbDevice) -> Category {
        self.prefs.category(&d.key()).unwrap_or_else(|| d.category())
    }

    /// Whether any device is turned off.
    fn blocked_count(&self) -> usize {
        self.devices.iter().filter(|d| d.blocked).count()
    }

    /// The tabs to show: the categories that have a device, and the one being looked at. The one being looked at falls back to the first with a
    /// device when its own has none left.
    fn tabs(&self) -> (Vec<Category>, Category) {
        let used: Vec<Category> = Category::ALL
            .into_iter()
            .filter(|c| self.devices.iter().any(|d| self.category_of(d) == *c))
            .collect();
        let active = if used.contains(&self.tab) || used.is_empty() {
            self.tab
        } else {
            used[0]
        };
        let mut tabs = used;
        if !tabs.contains(&active) {
            tabs.insert(0, active);
        }
        (tabs, active)
    }

    fn page(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("foot")
            .exact_size(34.0)
            .frame(egui::Frame::new().fill(theme::GROUND).inner_margin(egui::Margin::symmetric(24, 8)))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("Closing the window keeps Unplug in the tray.")
                        .size(12.0)
                        .color(theme::MUTED_DIM),
                );
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::GROUND).inner_margin(egui::Margin::symmetric(24, 20)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Unplug").size(24.0).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let n = self.blocked_count();
                        if quiet_link(ui, if n > 0 { "Unblock all" } else { "Nothing blocked" }, n > 0).clicked() {
                            let ids: Vec<String> = self.devices.iter().filter(|d| d.blocked).map(|d| d.instance_id.clone()).collect();
                            for id in ids {
                                self.set_blocked(&id, false);
                            }
                        }
                    });
                });
                ui.add_space(4.0);
                if let Some(e) = self.error.clone() {
                    ui.add(egui::Label::new(RichText::new(e).size(12.5).color(theme::WARN)).wrap());
                    ui.add_space(4.0);
                }
                if !self.loaded {
                    ui.label(RichText::new("Looking for USB devices...").color(theme::MUTED));
                    return;
                }
                let (tabs, active) = self.tabs();
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for c in &tabs {
                        if tab(ui, c.label(), *c == active, theme::ACCENT).clicked() {
                            self.tab = *c;
                        }
                    }
                });
                ui.add_space(6.0);
                let shown: Vec<UsbDevice> = self.devices.iter().filter(|d| self.category_of(d) == active).cloned().collect();
                if shown.is_empty() {
                    ui.add_space(24.0);
                    ui.label(RichText::new("No USB device found. Plug it in and it shows up here.").color(theme::MUTED));
                    return;
                }
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for d in &shown {
                        self.row(ui, d, active);
                    }
                });
            });
    }

    fn row(&mut self, ui: &mut egui::Ui, d: &UsbDevice, category: Category) {
        let working = self.working.contains(&d.instance_id);
        let protected = category.protected();
        let mut chosen: Option<Option<Category>> = None;
        ui.horizontal(|ui| {
            ui.set_min_height(58.0);
            ui.vertical(|ui| {
                ui.add_space(9.0);
                ui.label(RichText::new(&d.name).size(15.0).strong());
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    if d.blocked {
                        ui.label(RichText::new("Blocked").size(12.5).color(theme::DANGER_BRIGHT));
                    }
                    let mut sub = Vec::new();
                    if !d.manufacturer.is_empty() && !d.manufacturer.starts_with('(') {
                        sub.push(d.manufacturer.clone());
                    }
                    if !d.vid_pid_text().is_empty() {
                        sub.push(d.vid_pid_text());
                    }
                    if !sub.is_empty() {
                        let sep = if d.blocked { "  ·  " } else { "" };
                        ui.label(RichText::new(format!("{sep}{}", sub.join("  ·  "))).size(12.5).color(theme::MUTED));
                    }
                });
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let clickable = !working && !(protected && !d.blocked);
                let resp = switch(ui, d.blocked, clickable, theme::ACCENT);
                if protected && !d.blocked {
                    resp.on_hover_text("A keyboard or a mouse cannot be blocked: you would be left without it.");
                } else if resp.clicked() {
                    if d.blocked || category == Category::Controller {
                        self.set_blocked(&d.instance_id, !d.blocked);
                    } else {
                        self.confirm = Some(d.instance_id.clone());
                    }
                }
                ui.add_space(4.0);
                let menu = dots(ui);
                egui::Popup::menu(&menu).show(|ui| {
                    ui.set_min_width(150.0);
                    ui.label(RichText::new("Move to").size(12.0).color(theme::MUTED));
                    for c in Category::ALL {
                        if ui.selectable_label(c == category, c.label()).clicked() {
                            chosen = Some(Some(c));
                            ui.close();
                        }
                    }
                    if self.prefs.category(&d.key()).is_some() {
                        ui.separator();
                        if ui.button("Automatic").clicked() {
                            chosen = Some(None);
                            ui.close();
                        }
                    }
                });
            });
        });
        let y = ui.cursor().top();
        ui.painter().hline(
            ui.max_rect().x_range(),
            y,
            Stroke::new(1.0, theme::with_alpha(egui::Color32::WHITE, 0.06)),
        );
        ui.add_space(1.0);
        if let Some(c) = chosen {
            self.prefs.set_category(&d.key(), c);
        }
    }

    /// Asks before a device that is not a controller is blocked.
    fn confirm_window(&mut self, ctx: &egui::Context) {
        let Some(id) = self.confirm.clone() else { return };
        let Some(d) = self.devices.iter().find(|d| d.instance_id == id).cloned() else {
            self.confirm = None;
            return;
        };
        let mut block = false;
        let mut cancel = false;
        egui::Window::new("Block this device?")
            .title_bar(false)
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
                ui.set_width(340.0);
                ui.label(RichText::new(&d.name).size(16.0).strong());
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
                    cancel = secondary_button(ui, "Cancel", true).clicked();
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
        let ctx = ui.ctx().clone();
        // the close button hides the window: Unplug goes on in the tray (the tray's Exit ends it)
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        self.receive();
        if self.last_list.elapsed() >= REFRESH && self.working.is_empty() {
            self.refresh();
        }
        self.page(ui);
        self.confirm_window(&ctx);
        crate::dev::screenshot_tick(&ctx);
        ctx.request_repaint_after(REFRESH);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Demo;

    fn demo_app() -> UsbApp {
        UsbApp::new(egui::Context::default(), Arc::new(Demo::new()), Prefs::in_memory(), None)
    }

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
        let mut app = demo_app();
        wait_for(&mut app, |a| a.loaded);
        let id = app.devices[0].instance_id.clone();
        app.set_blocked(&id, true);
        wait_for(&mut app, |a| {
            a.working.is_empty() && !a.listing && a.devices.iter().any(|d| d.blocked)
        });
        assert_eq!(app.blocked_count(), 1);
        app.set_blocked(&id, false);
        wait_for(&mut app, |a| a.working.is_empty() && !a.listing && a.blocked_count() == 0);
    }

    #[test]
    fn the_keyboard_that_calls_itself_a_composite_device_is_in_keyboards() {
        let mut app = demo_app();
        wait_for(&mut app, |a| a.loaded);
        let composite = app.devices.iter().find(|d| d.name == "USB Composite Device").unwrap().clone();
        assert_eq!(app.category_of(&composite), Category::Keyboard);
        // the interfaces are not listed on their own
        assert!(app.devices.iter().all(|d| !d.is_interface()));
        // a choice of the player wins, and "Automatic" gives the automatic one back
        app.prefs.set_category(&composite.key(), Some(Category::Other));
        assert_eq!(app.category_of(&composite), Category::Other);
        app.prefs.set_category(&composite.key(), None);
        assert_eq!(app.category_of(&composite), Category::Keyboard);
    }

    #[test]
    fn the_tabs_are_the_categories_that_have_a_device() {
        let mut app = demo_app();
        wait_for(&mut app, |a| a.loaded);
        let (tabs, active) = app.tabs();
        assert_eq!(active, Category::Controller);
        assert_eq!(
            tabs,
            [
                Category::Controller,
                Category::Keyboard,
                Category::Mouse,
                Category::Storage,
                Category::Audio,
                Category::Camera
            ]
        );
        app.tab = Category::Other; // nothing there: the first one with a device is shown instead
        assert_eq!(app.tabs().1, Category::Controller);
    }
}
