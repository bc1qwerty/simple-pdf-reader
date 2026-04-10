# Simple PDF Reader

## Language
- Respond in Korean (한국어로 응답)

## Description
Portable, lightweight PDF reader built with Rust. Uses MuPDF for PDF rendering and egui for the native GUI. Cross-platform (Linux X11/Wayland, Windows).

## Tech Stack
- **Language**: Rust (edition 2021)
- **GUI**: eframe 0.33 + egui 0.33 (glow backend)
- **PDF**: mupdf 0.6
- **File dialog**: rfd 0.17
- **Clipboard**: arboard 3
- **Logging**: tracing + tracing-subscriber
- **Error handling**: anyhow
- **License**: AGPL-3.0

## Key Files
- `src/main.rs` -- Application entry point (binary: SimplePDFReader)
- `src/app.rs` -- Main app logic
- `src/backend/` -- PDF backend (MuPDF integration)
- `src/render/` -- Page rendering
- `src/state/` -- Application state management
- `src/ui/` -- UI components
- `build.rs` -- Build script (Windows icon resource)
- `assets/` -- Application icons
- `Cargo.toml` -- Dependencies and release profile

## Build & Run
```bash
cargo run                    # Debug build
cargo build --release        # Optimized release (LTO, stripped)
```

## Release Profile
- LTO enabled, stripped, single codegen unit, panic=abort for minimal binary size

## Status
- Development complete
