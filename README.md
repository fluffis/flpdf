# flpdf

A fast, minimal PDF viewer for Linux built with Rust, MuPDF, and egui.

## Features

- Fast rendering via MuPDF with DPI-aware scaling
- Text selection (flow-based, not rectangle) and copy
- Search with match highlighting and navigation
- Continuous vertical scroll with lazy page rendering
- Keyboard-driven: all actions have shortcuts
- Drag & drop PDF files
- Recent files and last-directory memory
- Lightweight — 17 MB stripped binary

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| Ctrl+O | Open file |
| Ctrl+W | Close |
| Ctrl+F | Search |
| Ctrl+G | Go to page |
| Ctrl+C | Copy selected text |
| Ctrl+A | Select all text on current page |
| Ctrl+Scroll | Zoom in/out |
| Ctrl++/- | Zoom in/out |
| Page Up/Down | Previous/next page |
| Ctrl+Home/End | First/last page |
| Escape | Close search / clear selection |

## Build

### Dependencies

Ubuntu/Debian:

```bash
sudo apt install libclang-dev libfontconfig1-dev
```

### Compile

```bash
cargo build --release
```

The binary is at `target/release/flpdf`.

## Usage

```bash
flpdf document.pdf
```

Or launch without arguments and open a file with Ctrl+O or drag & drop.

## Install as Default PDF Viewer

```bash
# Copy binary
sudo cp target/release/flpdf /usr/local/bin/

# Create desktop entry
cat > ~/.local/share/applications/flpdf.desktop << 'EOF'
[Desktop Entry]
Name=flpdf
Comment=Fast PDF viewer
Exec=flpdf %f
Icon=application-pdf
Terminal=false
Type=Application
MimeType=application/pdf;
Categories=Office;Viewer;
EOF

# Set as default
update-desktop-database ~/.local/share/applications/
xdg-mime default flpdf.desktop application/pdf
```

## License

MIT
