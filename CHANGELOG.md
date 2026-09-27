# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/), versions follow Semver.

## [Unreleased]

### Changed

- Larger default popup sizes: JSON viewer 900x700 → 1100x700, text viewer 700x500 → 1100x680
- Shortcuts/About dialogs now use the same popup frame (shadow/border) as the viewer windows

### Fixed

- Crash ("有几率卡死退出") when toggling a viewer window's maximize: egui 0.31's AccessKit tree diffing emits an update for a node missing from the old tree and `accesskit_consumer` panics on `unwrap()` (UI-thread panic, silent with `windows_subsystem = "windows"`). It only fired when a UI Automation client was attached (IMEs, screen readers, automation tools). The `accesskit` feature is now disabled in the eframe dependency; trade-off is losing screen-reader support
- Main window no longer starts slightly taller than the screen on scaled displays (e.g. 1080p at 125%/150%): the default 1280x800 is in logical points and could overflow the work area; at startup the window is now clamped to the monitor's logical size and centered (slightly above center)
- Popups now open centered: eframe persists window positions across sessions and old saved positions (right-hugging) overrode `default_size`; the viewer windows now set an explicit centered `default_pos` and use a new id salt to drop the stale memory
- Popup layering is visible in dark mode: the black shadow blended into dark backgrounds, so dark mode now uses a light border plus a faint white glow instead
- Drag-selecting text now keeps auto-scrolling when the pointer is held at (or beyond) the edge of the scroll area; previously scrolling stopped as soon as the pointer stopped moving (egui only scrolls on selection-change frames). Applies to all editable text views: detail Pretty tab, JSON viewer Pretty tab, text viewer

### Added

- Panic hook that appends panic info and a backtrace to `crash.log` next to the config file (`%APPDATA%/jsonl-v/crash.log`); with `windows_subsystem = "windows"` UI-thread panics previously vanished silently
- Headless regression tests for the maximize/restore flow of the JSON viewer window (direct toggle and simulated button clicks)
