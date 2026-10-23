// Copyright (C) 2026 the Unplug contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
//! Unplug: lists the USB devices and turns them off (and back on) at the driver level, so that no application can read them.

pub mod app;
pub mod backend;
pub mod dev;
pub mod device;
pub mod http;
pub mod ui;
pub mod update_check;
pub mod winapi;

/// The release number, read from the repository's `VERSION` file at build time.
pub const VERSION: &str = env!("UNPLUG_VERSION");
