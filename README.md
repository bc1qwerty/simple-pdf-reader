# Simple PDF Reader

A fast, lightweight, portable PDF viewer built with Rust + [egui](https://github.com/emilk/egui) + [MuPDF](https://mupdf.com/).

**12 MB single exe. No install. No cloud. No bloat.**

[![Download](https://img.shields.io/badge/Download-Windows%20exe-blue?style=for-the-badge)](https://github.com/bc1qwerty/simple-pdf-reader/releases/latest/download/SimplePDFReader.exe)

## Features

### Viewing
- **3 View Modes** — Single page, continuous scroll, dual page (side-by-side)
- **Zoom** — Pinch-to-zoom, Ctrl+scroll, presets (50%–300%), Fit Width, Fit Page
- **Cursor-centered zoom** — Zooms into where your mouse points
- **Page rotation** — 90° clockwise / counterclockwise
- **Fullscreen** — F11
- **Dark / Light mode** — Ctrl+D

### Navigation
- **Page navigation** — Prev/Next, page number input, PageUp/Down, Home/End, Space
- **Table of Contents** — Sidebar with collapsible outline
- **Bookmarks** — Mark pages, persisted across sessions
- **Search** — Ctrl+F, with highlighted results and F3/Shift+F3 navigation
- **Go to Page** — Ctrl+G dialog
- **PDF Links** — Click internal links to jump, external links open in browser
- **Mouse back/forward buttons** — Navigate pages
- **Last reading position** — Remembers where you left off per file

### Editing
- **Text selection** — Click and drag to select text in reading order, auto-copies to clipboard
- **Extract Pages** — Ctrl+E, type "1, 3, 5-8" to extract pages in any order (duplicates and reverse ranges supported)
- **Copy** — Ctrl+C, Ctrl+A (select all text on page)

### Compatibility
- **Password-protected PDFs** — Password dialog on open
- **Scanned PDFs** — Image-based PDFs render correctly (MuPDF)
- **Multi-language filenames** — Korean, Japanese, Chinese, Arabic, Thai, Hindi all display correctly
- **Any PDF** — MuPDF handles complex layouts, embedded fonts, forms

### Portable
- **Single .exe** — No install, no dependencies, no runtime
- **Config next to exe** — Settings saved as `simple-pdf-reader-config.json` beside the exe
- **Set as default** — File → "Set as Default PDF Viewer" (registers in Windows Settings)
- **Right-click context menu** — Copy, Select All, Fit Width/Page, Properties
- **Print** — Ctrl+P (opens system print dialog)

## Download

**[Download SimplePDFReader.exe](https://github.com/bc1qwerty/simple-pdf-reader/releases/latest/download/SimplePDFReader.exe)** (~12 MB)

Windows 10 / 11. Portable. No install needed.

> First run: Windows SmartScreen may warn — click "More info" → "Run anyway".

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+O` | Open file |
| `Ctrl+W` | Close document |
| `Ctrl+F` | Search |
| `Ctrl+G` | Go to page |
| `Ctrl+E` | Extract pages |
| `Ctrl+H` | Toggle Hand / Text select mode |
| `Ctrl+M` | Cycle view modes (Single → Scroll → Dual) |
| `Ctrl+R` | Rotate clockwise |
| `Ctrl+Shift+R` | Rotate counterclockwise |
| `Ctrl+B` | Toggle bookmark |
| `Ctrl+D` | Toggle dark / light mode |
| `Ctrl+P` | Print |
| `Ctrl+I` | Document properties |
| `Ctrl+C` | Copy selected text |
| `Ctrl+A` | Select all text on page |
| `Space` | Next page |
| `Shift+Space` | Previous page |
| `F3` / `Shift+F3` | Next / previous search result |
| `F11` | Fullscreen |
| `+` / `-` | Zoom in / out |
| `Ctrl+Scroll` | Zoom at cursor |

## Build from Source

### Requirements
- [Rust](https://rustup.rs/) (stable)
- [LLVM](https://releases.llvm.org/) (for MuPDF bindgen)
- Windows: Visual Studio Build Tools with C++ workload

### Build

```bash
# Linux
cargo build --release

# Windows (PowerShell)
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\lib"
cargo build --release
```

Or use the build script on Windows:
```
build-windows.bat
```

Output: `target/release/SimplePDFReader.exe` (~12 MB)

### Command Line

```
SimplePDFReader.exe                  # Open viewer
SimplePDFReader.exe document.pdf     # Open a specific PDF
SimplePDFReader.exe --register       # Set as default PDF viewer
SimplePDFReader.exe --unregister     # Remove file association
```

## Tech Stack

| Component | Technology |
|-----------|-----------|
| Language | Rust |
| GUI | [egui](https://github.com/emilk/egui) 0.33 + glow (OpenGL) |
| PDF Engine | [MuPDF](https://mupdf.com/) 0.6 |
| Clipboard | arboard |
| File Dialog | rfd |

## Architecture

```
src/
├── main.rs              # Entry point, window setup, file association
├── app.rs               # Main app state machine (eframe::App)
├── backend/
│   ├── mod.rs           # PdfBackend / PdfDocument traits
│   └── mupdf_backend.rs # MuPDF implementation
├── render/
│   ├── mod.rs           # Pixmap → egui texture conversion
│   ├── cache.rs         # LRU texture cache (30 entries)
│   └── pipeline.rs      # Async worker thread + queue drain
├── state/
│   ├── mod.rs           # ViewportState, TextSelection, InertialScroll
│   └── settings.rs      # JSON config (bookmarks, recent files, window size)
└── ui/
    ├── toolbar.rs        # Menu bar + compact toolbar
    ├── sidebar.rs        # TOC / Bookmarks panel
    └── search_bar.rs     # Search input + navigation
```

**Key design decisions:**
- **Trait abstraction** — `PdfBackend` trait allows swapping MuPDF for PDFium later
- **Async rendering** — Worker thread renders pages, UI shows scaled fallback instantly
- **Queue drain** — Multiple rapid zoom requests are collapsed to only render the latest
- **Binary search visibility** — Only visible pages are processed (O(log n) instead of O(n))
- **No debounce** — SumatraPDF approach: show old texture scaled, replace when ready

## License

AGPL-3.0 (due to MuPDF dependency)

## Links

- **Website**: [apps.txid.uk](https://apps.txid.uk/ko/simple-pdf-reader/)
- **Issues**: [GitHub Issues](https://github.com/bc1qwerty/simple-pdf-reader/issues)
