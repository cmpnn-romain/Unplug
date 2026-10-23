# Unplug

A small Windows tool that lists USB devices and turns them off (and on) so no application can read them (joysticks that drive games on their
own). Same look, build and release system as the Cairn launcher (`~/Launcher`, <https://github.com/Cairn-client/Launcher>).

## Layout

| Path | What it is |
| --- | --- |
| `rust/crates/unplug/src/device.rs` | the device model and the rules: categories, hubs, composite devices (`group` adds the interfaces' ids to their device) |
| `rust/crates/unplug/src/prefs.rs` | the categories the player chose, in `%APPDATA%\Unplug\categories.txt` |
| `rust/crates/unplug/src/winapi.rs` | the tray icon and its menu, showing the hidden window, one copy at a time |
| `rust/crates/unplug/src/backend.rs` | `Backend` trait: Windows' SetupAPI (`DIF_PROPERTYCHANGE`, `DICS_DISABLE` / `DICS_ENABLE`), and a `Demo` list |
| `rust/crates/unplug/src/app.rs` | the window; Windows is called on threads, results come back through a channel |
| `rust/crates/unplug/src/ui/` | `theme.rs` and `widgets.rs`, copied from the launcher: keep them in step with it |
| `tools/` | `Build-Release.ps1`, `Test-VersionPolicy.ps1`, `New-ReleaseNotes.ps1` |

## Build and test

```bash
cd rust && cargo build --release && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check
```

On Linux the tests and `--demo` run; the real backend compiles only for Windows (`cargo check --target x86_64-pc-windows-gnu`).
`UNPLUG_SCREENSHOT=<file.png>` makes the window save a picture of itself and close (`UNPLUG_TAB=<category>` picks the tab).

## Releases and versions

`VERSION` is the single source of truth (strict `MAJOR.MINOR.PATCH`). Tag `X.Y.Z` on `main` and push the tag; CI builds and publishes
`Unplug-X.Y.Z.exe`. Do not upload by hand. Commits follow Conventional Commits; the release notes are generated from `feat`/`fix`/`perf`/`refactor`
titles. See `docs/VERSIONING.md`.

## Pitfalls

- The exe requests administrator rights (manifest in `build.rs`); without them SetupAPI answers access denied.
- A keyboard or a mouse must never be blockable (lock-out): the category decides, and the player can only move a device out of it by hand. Hubs and `&MI_` interfaces are never listed.
- The window has a fixed size, and its close button hides it (the tray's Exit ends the program).
- Text files are LF; no BOM.
