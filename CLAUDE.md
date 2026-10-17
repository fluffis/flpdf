# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
cargo build                    # Debug build (mupdf/mupdf-sys optimized via per-package profile)
cargo build --release          # Release build (fat LTO, single codegen unit, stripped)
cargo run -- /path/to/file.pdf # Run debug build with a PDF
```

Build dependencies (Ubuntu/Debian): `libclang-dev libfontconfig1-dev`

First build is slow (~2-5 min) because mupdf-sys vendors and compiles the MuPDF C library. Subsequent builds are fast (~1-2s).

## Architecture

**flpdf** is a fast, minimal PDF viewer for Linux using MuPDF for rendering and egui for the GUI.

### Core modules

- **`main.rs`** — CLI arg parsing (clap), eframe window setup
- **`app.rs`** — Main `FlpdfApp` struct implementing `eframe::App`. Contains all UI layout (toolbar, search bar, status bar, scroll area), keyboard/mouse handling, and rendering orchestration. This is the largest file.
- **`pdf_document.rs`** — Wrapper around `mupdf::pdf::PdfDocument`. Handles page rendering to RGBA pixmaps, text extraction (char-level with bounding boxes via stext), and search.
- **`text_selection.rs`** — Flow-based text selection. Maps screen coordinates to char indices using closest-char lookup, selects contiguous ranges in reading order.
- **`state.rs`** — Persistent state (last directory, recent files) saved as JSON to `~/.config/flpdf/state.json`.

### Key rendering pipeline

1. MuPDF renders pages at physical pixel resolution: `pixels_per_point * (96/72) * zoom`
2. Textures are displayed at logical point size so GPU maps 1:1 to physical pixels
3. Only pages near the viewport are rendered (±2 page buffer); distant textures are evicted (max 10 cached)
4. Text extraction is deferred until the user starts selecting text
5. Search/highlight coordinates are in render-pixel space and converted to logical points via `px_to_pt` ratio

### Platform notes

- egui on Linux reports `ctrl` as both `Modifiers::CTRL` and `Modifiers::COMMAND`. Use `Modifiers::COMMAND` for shortcuts. Ctrl+C specifically needs to also check `Modifiers::CTRL` and `Event::Copy` as fallbacks.
- Clipboard (`arboard`) must be kept alive in the app struct — dropping it clears the clipboard on Wayland.
- `consume_key` via `ctx.input_mut()` is required for global shortcuts to work regardless of widget focus. Plain `ctx.input(|i| i.key_pressed(...))` does not reliably fire.
