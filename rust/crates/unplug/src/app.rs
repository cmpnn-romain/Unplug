// Copyright (C) 2026 the Unplug contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
//! The window: the USB devices by category, one switch per device. Windows is asked on a thread of its own, so the window never waits for a
//! driver. Closing the window hides it: Unplug stays in the tray until Exit.

use crate::backend::Backend;
use crate::device::{self, Category, UsbDevice};
use crate::http::Http;
use crate::ui::theme;
use crate::ui::widgets::{primary_button, quiet_link, secondary_button, switch, tab};
use crate::update_check::{Check, UpdateInfo};
use crate::winapi::TrayEvent;
use eframe::egui::{self, Align, CornerRadius, Layout, RichText, Stroke};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How often the list is read again, so that a device plugged in later shows up.
const REFRESH: Duration = Duration::from_secs(3);

/// The size of the window, which never changes.
pub const WINDOW: [f32; 2] = [520.0, 600.0];

pub enum Msg {
    Listed(Result<Vec<UsbDevice>, String>),
    Changed {
        id: String,
        blocked: bool,
        result: Result<(), String>,
    },
    UpdateChecked(Check, bool),
    UpdateProgress(f64),
    UpdateDownloaded(Result<PathBuf, String>),
}

/// The window that offers Unplug's own update.
#[derive(Clone)]
pub struct UpdateModal {
    /// `offer`, `busy`, `error`, `current` or `unreachable`.
    pub stage: String,
    pub text: String,
    pub progress: Option<f64>,
    pub cancel: Arc<AtomicBool>,
}

pub struct UsbApp {
    backend: Arc<dyn Backend>,
    pub(crate) http: Arc<dyn Http>,
    pub(crate) ctx: egui::Context,
    pub(crate) tx: Sender<Msg>,
    rx: Receiver<Msg>,
    /// What the tray's menu asks for.
    tray_events: Option<Receiver<TrayEvent>>,
    /// Kept so that the tray icon is taken away when the window's program ends.
    tray: Option<crate::winapi::Tray>,
    devices: Vec<UsbDevice>,
    loaded: bool,
    listing: bool,
    last_list: Instant,
    /// What went wrong the last time, shown above the list.
    pub(crate) error: Option<String>,
    /// The category the player is looking at, and the device the keyboard has chosen in it.
    tab: Category,
    selected: Option<String>,
    /// Set when the keyboard moved the choice: the list scrolls to it once.
    scroll_to_selected: bool,
    pub(crate) update: Option<UpdateInfo>,
    pub(crate) update_checking: bool,
    pub(crate) update_modal: Option<UpdateModal>,
    /// Devices Windows is turning on or off right now.
    working: HashSet<String>,
    /// The device waiting for the player to confirm that it may be blocked.
    confirm: Option<String>,
}

impl UsbApp {
    pub fn new(ctx: egui::Context, backend: Arc<dyn Backend>, http: Arc<dyn Http>, tray: Option<crate::winapi::Tray>) -> Self {
        let (tx, rx) = channel();
        let mut app = UsbApp {
            backend,
            http,
            ctx,
            tx,
            rx,
            tray_events: None,
            tray,
            devices: Vec::new(),
            loaded: false,
            listing: false,
            last_list: Instant::now(),
            error: None,
            tab: crate::dev::start_tab(),
            selected: None,
            scroll_to_selected: false,
            update: None,
            update_checking: false,
            update_modal: None,
            working: HashSet::new(),
            confirm: None,
        };
        app.refresh();
        app
    }

    /// Listens to what the tray's menu asks for.
    pub fn listen(mut self, events: Receiver<TrayEvent>) -> Self {
        self.tray_events = Some(events);
        self
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
                Msg::UpdateChecked(check, open) => self.update_checked(check, open),
                Msg::UpdateProgress(p) => {
                    if let Some(m) = self.update_modal.as_mut() {
                        m.progress = Some(p);
                        m.text = format!("Downloading {}...", self.update.as_ref().map(|u| u.tag.as_str()).unwrap_or(""));
                    }
                }
                Msg::UpdateDownloaded(result) => match result {
                    Ok(path) => {
                        if let Some(m) = self.update_modal.as_mut() {
                            m.text = "Installing...".into();
                        }
                        match crate::update_check::install(&path) {
                            Ok(()) => {
                                // the tray icon goes first, or it stays until the mouse passes over it
                                self.tray = None;
                                std::thread::sleep(Duration::from_millis(200));
                                std::process::exit(0);
                            }
                            Err(e) => self.update_failed(&format!(
                                "The update could not be installed: {e}. You can download it by hand from the release page."
                            )),
                        }
                    }
                    Err(why) if why == "cancelled" => self.update_modal = None,
                    Err(why) => self.update_failed(&format!(
                        "The update could not be downloaded: {why}. You can download it by hand from the release page."
                    )),
                },
            }
        }
    }

    /// What the tray's menu asked for while the window was away.
    fn tray_requests(&mut self) {
        let events: Vec<TrayEvent> = self.tray_events.as_ref().map(|rx| rx.try_iter().collect()).unwrap_or_default();
        for e in events {
            if e == TrayEvent::CheckUpdates {
                self.check_for_update(true);
            }
        }
    }

    /// The category of a device: the one it looks like.
    pub fn category_of(&self, d: &UsbDevice) -> Category {
        d.category()
    }

    /// Turns a device off, or back on. A keyboard or a mouse is never turned off, and a device that does not look like a controller asks first.
    fn toggle(&mut self, d: &UsbDevice) {
        if self.working.contains(&d.instance_id) {
            return;
        }
        let category = self.category_of(d);
        if d.blocked {
            self.set_blocked(&d.instance_id, false);
        } else if category == Category::Controller {
            self.set_blocked(&d.instance_id, true);
        } else if !category.protected() {
            self.confirm = Some(d.instance_id.clone());
        }
    }

    fn unblock_all(&mut self) {
        let ids: Vec<String> = self.devices.iter().filter(|d| d.blocked).map(|d| d.instance_id.clone()).collect();
        for id in ids {
            self.set_blocked(&id, false);
        }
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

    /// The keys, for when the mouse is not there to click: Left and Right choose the category, Up and Down the device, Space or Enter turns it
    /// off or on, Ctrl+U unblocks everything, Escape hides the window.
    fn keys(&mut self, ctx: &egui::Context, tabs: &[Category], active: Category, shown: &[UsbDevice]) {
        use egui::{Key, Modifiers};
        let (left, right, up, down, act, unblock, hide) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowLeft),
                i.consume_key(Modifiers::NONE, Key::ArrowRight),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::Space) || i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::CTRL, Key::U),
                i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        let at = tabs.iter().position(|c| *c == active).unwrap_or(0);
        if (left && at > 0) || (right && at + 1 < tabs.len()) {
            self.tab = tabs[if left { at - 1 } else { at + 1 }];
            self.selected = None;
        }
        let current = self
            .selected
            .as_ref()
            .and_then(|id| shown.iter().position(|d| &d.instance_id == id));
        let moved = match (up, down, current) {
            (_, true, None) => Some(0),
            (_, true, Some(i)) => Some((i + 1).min(shown.len().saturating_sub(1))),
            (true, _, None) => Some(shown.len().saturating_sub(1)),
            (true, _, Some(i)) => Some(i.saturating_sub(1)),
            _ => None,
        };
        if let Some(d) = moved.and_then(|i| shown.get(i)) {
            self.selected = Some(d.instance_id.clone());
            self.scroll_to_selected = true;
        }
        if act {
            if let Some(d) = current.and_then(|i| shown.get(i)) {
                self.toggle(d);
            }
        }
        if unblock {
            self.unblock_all();
        }
        if hide {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
    }

    fn page(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("foot")
            .exact_size(34.0)
            .frame(egui::Frame::new().fill(theme::GROUND).inner_margin(egui::Margin::symmetric(24, 8)))
            .show(ui, |ui| {
                ui.label(RichText::new("Ctrl+U  unblock all").size(12.0).color(theme::MUTED_DIM));
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::GROUND).inner_margin(egui::Margin::symmetric(24, 20)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Unplug").size(24.0).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.blocked_count() > 0 && quiet_link(ui, "Unblock all", true).clicked() {
                            self.unblock_all();
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
                let shown: Vec<UsbDevice> = self.devices.iter().filter(|d| self.category_of(d) == active).cloned().collect();
                if self.confirm.is_none() && self.update_modal.is_none() {
                    self.keys(ui.ctx(), &tabs, active, &shown);
                }
                let (tabs, active) = self.tabs();
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for c in &tabs {
                        if tab(ui, c.label(), *c == active, theme::ACCENT).clicked() {
                            self.tab = *c;
                            self.selected = None;
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
                        let selected = self.selected.as_deref() == Some(d.instance_id.as_str());
                        self.row(ui, d, selected);
                    }
                });
                self.scroll_to_selected = false;
            });
    }

    /// One device: its name and what it is on one line each (cut with "..." when too long), and the switch.
    fn row(&mut self, ui: &mut egui::Ui, d: &UsbDevice, selected: bool) {
        const HEIGHT: f32 = 58.0;
        let category = self.category_of(d);
        let working = self.working.contains(&d.instance_id);
        let protected = category.protected();
        let left_w = (ui.available_width() - 42.0 - 20.0).max(60.0);
        let top = ui.cursor().top();
        let band = egui::Rect::from_min_size(
            egui::pos2(ui.max_rect().left() - 8.0, top),
            egui::vec2(ui.max_rect().width() + 16.0, HEIGHT),
        );
        if selected && self.scroll_to_selected {
            ui.scroll_to_rect(band, None);
        }
        ui.horizontal(|ui| {
            ui.set_min_height(HEIGHT);
            ui.allocate_ui_with_layout(egui::vec2(left_w, HEIGHT), Layout::top_down(Align::Min), |ui| {
                ui.set_max_width(left_w);
                ui.add_space(9.0);
                ui.add(
                    egui::Label::new(RichText::new(&d.name).size(15.0).strong())
                        .truncate()
                        .sense(egui::Sense::hover()),
                )
                .on_hover_text(&d.name);
                let mut sub = Vec::new();
                if !d.manufacturer.is_empty() && !d.manufacturer.starts_with('(') {
                    sub.push(d.manufacturer.clone());
                }
                if !d.vid_pid_text().is_empty() {
                    sub.push(d.vid_pid_text());
                }
                let font = egui::FontId::proportional(12.5);
                let mut job = egui::text::LayoutJob::default();
                if !sub.is_empty() {
                    job.append(&sub.join("  \u{b7}  "), 0.0, egui::TextFormat::simple(font, theme::MUTED));
                }
                ui.add(egui::Label::new(job).truncate());
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                // on: the device works; off: it is blocked
                let clickable = !working && !(protected && !d.blocked);
                let resp = switch(ui, !d.blocked, clickable, selected, theme::ONLINE, theme::BLOOD);
                if protected && !d.blocked {
                    resp.on_hover_text("A keyboard or a mouse cannot be blocked: you would be left without it.");
                } else if resp.clicked() {
                    self.selected = None;
                    self.toggle(d);
                }
            });
        });
        let y = ui.cursor().top();
        ui.painter().hline(
            ui.max_rect().x_range(),
            y,
            Stroke::new(1.0, theme::with_alpha(egui::Color32::WHITE, 0.06)),
        );
        ui.add_space(1.0);
    }

    /// Asks before a device that is not a controller is blocked.
    fn confirm_window(&mut self, ctx: &egui::Context) {
        let Some(id) = self.confirm.clone() else { return };
        let Some(d) = self.devices.iter().find(|d| d.instance_id == id).cloned() else {
            self.confirm = None;
            return;
        };
        let mut block = ctx.input(|i| i.key_pressed(egui::Key::Enter));
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
                    block |= primary_button(ui, "Block it", theme::ACCENT, true).clicked();
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
        self.tray_requests();
        if self.last_list.elapsed() >= REFRESH && self.working.is_empty() {
            self.refresh();
        }
        self.page(ui);
        self.confirm_window(&ctx);
        self.update_window(&ctx);
        crate::dev::screenshot_tick(&ctx);
        ctx.request_repaint_after(REFRESH);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Demo;
    use crate::http::fake::FakeHttp;

    fn demo_app() -> UsbApp {
        UsbApp::new(egui::Context::default(), Arc::new(Demo::new()), Arc::new(FakeHttp::default()), None)
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
        let d = app.devices.iter().find(|d| d.name.contains("Joystick")).unwrap().clone();
        let before = app.blocked_count();
        app.toggle(&d);
        wait_for(&mut app, |a| a.working.is_empty() && !a.listing && a.blocked_count() == before + 1);
        let d = app.devices.iter().find(|x| x.instance_id == d.instance_id).unwrap().clone();
        assert!(d.blocked);
        app.toggle(&d);
        wait_for(&mut app, |a| a.working.is_empty() && !a.listing && a.blocked_count() == before);
    }

    #[test]
    fn a_keyboard_cannot_be_blocked_and_another_device_asks_first() {
        let mut app = demo_app();
        wait_for(&mut app, |a| a.loaded);
        let keyboard = app.devices.iter().find(|d| d.name == "USB Composite Device").unwrap().clone();
        assert_eq!(app.category_of(&keyboard), Category::Keyboard);
        app.toggle(&keyboard);
        assert!(app.working.is_empty() && app.confirm.is_none());
        let stick = app.devices.iter().find(|d| d.name == "SanDisk Ultra").unwrap().clone();
        app.toggle(&stick);
        assert_eq!(app.confirm.as_deref(), Some(stick.instance_id.as_str()));
        assert!(app.working.is_empty());
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

    #[test]
    fn a_newer_release_is_offered_and_the_answer_is_said_only_when_asked_for() {
        let mut app = demo_app();
        app.update_checked(Check::Current, false);
        assert!(app.update_modal.is_none());
        app.update_checked(Check::Unreachable, true);
        assert_eq!(app.update_modal.as_ref().map(|m| m.stage.as_str()), Some("unreachable"));
        let info = UpdateInfo {
            tag: "9.0.0".into(),
            page_url: String::new(),
            download_url: None,
            sha256: None,
            size: 0,
            notes: String::new(),
        };
        app.update_checked(Check::Newer(info), false);
        assert_eq!(app.update_modal.as_ref().map(|m| m.stage.as_str()), Some("offer"));
    }
}
