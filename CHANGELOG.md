# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/), versions follow Semver.

## [Unreleased]

### Changed

- Larger default popup sizes: JSON viewer 900x700 → 1100x700, text viewer 700x500 → 1100x680
- Shortcuts/About dialogs now use the same popup frame (shadow/border) as the viewer windows

### Fixed

- Popups now open centered: eframe persists window positions across sessions and old saved positions (right-hugging) overrode `default_size`; the viewer windows now set an explicit centered `default_pos` and use a new id salt to drop the stale memory
- Popup layering is visible in dark mode: the black shadow blended into dark backgrounds, so dark mode now uses a light border plus a faint white glow instead
- Drag-selecting text now keeps auto-scrolling when the pointer is held at (or beyond) the edge of the scroll area; previously scrolling stopped as soon as the pointer stopped moving (egui only scrolls on selection-change frames). Applies to all editable text views: detail Pretty tab, JSON viewer Pretty tab, text viewer
