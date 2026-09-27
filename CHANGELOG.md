# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/), versions follow Semver.

## [Unreleased]

### Changed

- Larger default popup sizes: JSON viewer 900x700 → 1100x800, text viewer 700x500 → 960x680 (still centered)

### Fixed

- Drag-selecting text now keeps auto-scrolling when the pointer is held at (or beyond) the edge of the scroll area; previously scrolling stopped as soon as the pointer stopped moving (egui only scrolls on selection-change frames). Applies to all editable text views: detail Pretty tab, JSON viewer Pretty tab, text viewer
