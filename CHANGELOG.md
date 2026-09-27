# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/), versions follow Semver.

## [Unreleased]

## [0.1.5] - 2026-09-27

### Changed

- Startup main window height now reserves an extra taskbar's height at the bottom (~48 logical px more), so with the Windows taskbar visible the window no longer hugs the bottom edge
- Larger default popup sizes: JSON viewer 900x700 → 1100x700, text viewer 700x500 → 1100x680
- Shortcuts/About dialogs now use the same popup frame (shadow/border) as the viewer windows

### Fixed

- Clicks inside popup windows or the rebuilt-context search box no longer leak through and toggle the reading marker in the Full Context (Rebuilt) tab; row clicks now go through egui's hit testing (a row widget is registered with the previous frame's rect before its children are drawn, so child buttons and windows on top win naturally)
- Clicking on row text now toggles the reading marker again: egui 0.31 labels are selectable by default (click_and_drag sense) and were winning the click hit-test over the row widget, so only blank areas marked the row; the row now also treats a click on any same-layer widget within its rect as a row click (drag-selecting text still doesn't mark). Row rects are intersected with the scroll viewport for this test, so scrolled-out rows no longer grab clicks meant for the search box/toolbar above
- The rebuilt-context search prev/next buttons are now full-size with a wider gap, so they are harder to misclick
- Crash ("有几率卡死退出") when toggling a viewer window's maximize: egui 0.31's AccessKit tree diffing emits an update for a node missing from the old tree and `accesskit_consumer` panics on `unwrap()` (UI-thread panic, silent with `windows_subsystem = "windows"`). It only fired when a UI Automation client was attached (IMEs, screen readers, automation tools). The `accesskit` feature is now disabled in the eframe dependency; trade-off is losing screen-reader support
- Main window no longer starts slightly taller than the screen on scaled displays (e.g. 1080p at 125%/150%): the default 1280x800 is in logical points and could overflow the work area; at startup the window is now clamped to the monitor's logical size and centered (slightly above center)
- Popups now open centered: eframe persists window positions across sessions and old saved positions (right-hugging) overrode `default_size`; the viewer windows now set an explicit centered `default_pos` and use a new id salt to drop the stale memory
- Popup layering is visible in dark mode: the black shadow blended into dark backgrounds, so dark mode now uses a light border plus a faint white glow instead
- Drag-selecting text now keeps auto-scrolling when the pointer is held at (or beyond) the edge of the scroll area; previously scrolling stopped as soon as the pointer stopped moving (egui only scrolls on selection-change frames). Applies to all editable text views: detail Pretty tab, JSON viewer Pretty tab, text viewer

### Added

- Full Context (Rebuilt) tab: items are numbered; clicking an item toggles a reading marker (single-row cursor, remembered per request line, survives opening/closing text popups); the tab has its own search box (with a × clear button) scoped to the text visible on screen (long items contribute only their truncated preview) with in-place hit highlighting, title-hit background, and Enter/Shift+Enter or ↑/↓ navigation with wraparound scrolling; while the pointer hovers the rebuilt view, arrow/page keys scroll it instead of moving the line-list selection
- Panic hook that appends panic info and a backtrace to `crash.log` next to the config file (`%APPDATA%/jsonl-v/crash.log`); with `windows_subsystem = "windows"` UI-thread panics previously vanished silently
- Headless regression tests for the maximize/restore flow of the JSON viewer window (direct toggle and simulated button clicks)
