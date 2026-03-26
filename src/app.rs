use anyhow::Result;
use std::path::PathBuf;

use crate::backend::mupdf_backend::MuPdfBackend;
use crate::backend::{PageLink, PdfBackend, PdfDocument};
use crate::render::cache::PageCache;
use crate::render::pipeline::{RenderPipeline, RenderPriority};
use crate::render::pixels_to_texture;
use crate::state::settings::AppSettings;
use crate::state::{
    CursorMode, DocumentState, FitMode, InertialScroll, PasswordState, SearchState,
    SidebarTab, TextSelection, ViewMode, ViewportState,
};
use crate::ui::search_bar::{self, SearchAction};
use crate::ui::sidebar;
use crate::ui::toolbar::{self, ZoomPreset};

pub struct PdfReaderApp {
    document: Option<DocumentState>,
    viewport: ViewportState,

    pipeline: Option<RenderPipeline>,
    cache: PageCache,
    pending_pages: std::collections::HashSet<usize>,

    sidebar_open: bool,
    sidebar_tab: SidebarTab,
    search: SearchState,
    page_input: String,
    scroll_to_page: Option<usize>,
    scroll_offset: egui::Vec2,

    // Text selection
    selection: TextSelection,
    clipboard: Option<arboard::Clipboard>,

    // Password dialog
    password: PasswordState,

    // Page rects for hit testing (page_no -> screen rect)
    page_rects: std::collections::HashMap<usize, egui::Rect>,

    // Central panel origin for coordinate conversion
    panel_origin: egui::Pos2,

    // Inertial scroll
    inertial: InertialScroll,

    // Cached page links per page
    page_links_cache: std::collections::HashMap<usize, Vec<PageLink>>,

    // Document metadata for properties dialog
    doc_file_size: Option<u64>,
    doc_author: Option<String>,

    // Context menu
    context_menu_pos: Option<egui::Pos2>,

    settings: AppSettings,
}

impl PdfReaderApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = AppSettings::load();
        if settings.dark_mode {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
        } else {
            cc.egui_ctx.set_visuals(egui::Visuals::light());
        }

        let clipboard = arboard::Clipboard::new().ok();

        let mut app = Self {
            document: None,
            viewport: ViewportState::default(),
            pipeline: None,
            cache: PageCache::new(30),
            pending_pages: std::collections::HashSet::new(),
            sidebar_open: false,
            sidebar_tab: SidebarTab::default(),
            search: SearchState::default(),
            page_input: "1".into(),
            scroll_to_page: None,
            scroll_offset: egui::Vec2::ZERO,
            selection: TextSelection::default(),
            clipboard,
            password: PasswordState::default(),
            page_rects: std::collections::HashMap::new(),
            panel_origin: egui::Pos2::ZERO,
            inertial: InertialScroll::default(),
            page_links_cache: std::collections::HashMap::new(),
            doc_file_size: None,
            doc_author: None,
            context_menu_pos: None,
            settings,
        };

        if let Some(path) = std::env::args().nth(1) {
            let path = PathBuf::from(&path);
            if path.exists() {
                app.try_open_document(&path, &cc.egui_ctx);
            }
        }

        app
    }

    /// Try to open, handle password-protected PDFs
    fn try_open_document(&mut self, path: &PathBuf, ctx: &egui::Context) {
        let backend = MuPdfBackend;
        match backend.open_file(path) {
            Ok(doc) => {
                if doc.needs_password() {
                    self.password = PasswordState {
                        needed: true,
                        input: String::new(),
                        path: Some(path.clone()),
                        error: false,
                    };
                } else {
                    let _ = self.open_document_inner(path, ctx);
                }
            }
            Err(e) => {
                // MuPDF returns error for password-protected files too
                let err_str = format!("{e}");
                if err_str.contains("password") {
                    self.password = PasswordState {
                        needed: true,
                        input: String::new(),
                        path: Some(path.clone()),
                        error: false,
                    };
                } else {
                    eprintln!("[ERROR] Failed to open: {e}");
                }
            }
        }
    }

    fn open_document_with_password(&mut self, path: &PathBuf, password: &str, ctx: &egui::Context) -> bool {
        let backend = MuPdfBackend;
        match backend.open_file(path) {
            Ok(mut doc) => {
                match doc.authenticate(password) {
                    Ok(true) => {
                        let _ = self.open_document_inner(path, ctx);
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    fn open_document_inner(&mut self, path: &PathBuf, ctx: &egui::Context) -> Result<()> {
        let backend = MuPdfBackend;
        let doc = backend.open_file(path)?;
        let page_count = doc.page_count();
        let outline = doc.outline().unwrap_or_default();
        let metadata = doc.metadata().unwrap_or_default();
        let file_size = doc.file_size_bytes();

        let mut page_sizes = Vec::with_capacity(page_count);
        for i in 0..page_count {
            page_sizes.push(doc.page_size(i).unwrap_or((612.0, 792.0)));
        }

        self.document = Some(DocumentState {
            path: path.clone(), page_count, outline, title: metadata.title, page_sizes,
        });

        self.viewport = ViewportState::default();
        self.page_input = "1".into();
        self.cache.clear();
        self.pending_pages.clear();
        self.search = SearchState::default();
        self.selection = TextSelection::default();
        self.scroll_offset = egui::Vec2::ZERO;
        self.page_rects.clear();
        self.page_links_cache.clear();
        self.doc_file_size = file_size;
        self.doc_author = metadata.author;

        let worker_doc = backend.open_file(path)?;
        self.pipeline = Some(RenderPipeline::new(worker_doc, ctx.clone()));

        self.settings.add_recent_file(path.clone());

        // Restore last reading position
        let file_key = path.to_string_lossy().to_string();
        if let Some(last_page) = self.settings.get_last_page(&file_key) {
            if last_page < page_count {
                self.viewport.go_to_page(last_page, page_count);
                self.scroll_to_page = Some(last_page);
                self.page_input = format!("{}", last_page + 1);
            }
        }

        self.settings.save();

        if let Some(ref doc) = self.document {
            if !doc.outline.is_empty() { self.sidebar_open = true; }
        }

        // Update window title
        let title = self.document.as_ref()
            .and_then(|d| d.title.as_deref()
                .or_else(|| d.path.file_name().and_then(|f| f.to_str())))
            .unwrap_or("Simple PDF Reader");
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!("{} - Simple PDF Reader", title)));

        // Resize window to fit first page (with some padding for toolbar/statusbar)
        if let Some(ref doc) = self.document {
            if !doc.page_sizes.is_empty() {
                let (pw, ph) = doc.page_sizes[0];
                let ratio = pw / ph;
                // Target: page fills the window height (minus ~80px for UI chrome)
                let target_h = 900.0f32.min(ph + 100.0);
                let target_w = ((target_h - 100.0) * ratio + 40.0).max(600.0);
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(
                    egui::Vec2::new(target_w, target_h)
                ));
            }
        }

        self.password = PasswordState::default();
        Ok(())
    }

    fn request_render(&mut self, page_no: usize, scale: f32) {
        let rotation = self.viewport.rotation;
        if self.pending_pages.contains(&page_no) || self.cache.contains(page_no, scale, rotation) {
            return;
        }
        if let Some(ref mut pipeline) = self.pipeline {
            let priority = if page_no == self.viewport.current_page {
                RenderPriority::Current
            } else {
                RenderPriority::Adjacent
            };
            pipeline.request(page_no, scale, rotation, priority);
            self.pending_pages.insert(page_no);
        }
    }

    fn prefetch_pages(&mut self, visible_pages: &[usize]) {
        let page_count = self.document.as_ref().map(|d| d.page_count).unwrap_or(0);
        let zoom = self.viewport.zoom;
        let direction = self.viewport.last_scroll_direction;

        for &p in visible_pages {
            self.request_render(p, zoom);
        }

        // Direction-aware prefetch (Evince-style)
        if direction >= 0.0 {
            // Scrolling down: prefetch more pages after visible range
            if let Some(&last) = visible_pages.last() {
                for p in (last + 1)..(last + 5).min(page_count) {
                    self.request_render(p, zoom);
                }
            }
            if let Some(&first) = visible_pages.first() {
                for i in 1..=1 {
                    if first >= i { self.request_render(first - i, zoom); }
                }
            }
        } else {
            // Scrolling up: prefetch more pages before visible range
            if let Some(&first) = visible_pages.first() {
                for i in 1..=4 {
                    if first >= i { self.request_render(first - i, zoom); }
                }
            }
            if let Some(&last) = visible_pages.last() {
                for p in (last + 1)..(last + 2).min(page_count) {
                    self.request_render(p, zoom);
                }
            }
        }
    }

    fn do_search(&mut self, query: &str) {
        if query.is_empty() { return; }
        let page_count = self.document.as_ref().map(|d| d.page_count).unwrap_or(0);
        let path = match self.document.as_ref() {
            Some(d) => d.path.clone(),
            None => return,
        };
        let backend = MuPdfBackend;
        let doc = match backend.open_file(&path) { Ok(d) => d, Err(_) => return };

        let current = self.viewport.current_page;
        let mut all_hits = Vec::new();
        for offset in 0..page_count {
            let page_no = (current + offset) % page_count;
            if let Ok(hits) = doc.search_page(page_no, query) {
                all_hits.extend(hits);
            }
        }

        self.search.results = all_hits;
        self.search.current_hit_idx = 0;
        if let Some(first) = self.search.results.first() {
            let target = first.page_no;
            self.viewport.go_to_page(target, page_count);
            self.scroll_to_page = Some(target);
        }
    }

    fn navigate_search(&mut self, forward: bool) {
        if self.search.results.is_empty() { return; }
        let page_count = self.document.as_ref().map(|d| d.page_count).unwrap_or(0);
        let len = self.search.results.len();
        self.search.current_hit_idx = if forward {
            (self.search.current_hit_idx + 1) % len
        } else if self.search.current_hit_idx == 0 { len - 1 }
        else { self.search.current_hit_idx - 1 };

        let page = self.search.results[self.search.current_hit_idx].page_no;
        self.viewport.go_to_page(page, page_count);
        self.scroll_to_page = Some(page);
    }

    fn file_key(&self) -> String {
        self.document.as_ref().map(|d| d.path.to_string_lossy().to_string()).unwrap_or_default()
    }

    fn calc_fit_zoom(&self, available_w: f32, available_h: f32) -> Option<f32> {
        let doc = self.document.as_ref()?;
        if self.viewport.current_page >= doc.page_sizes.len() { return None; }
        let (pw, ph) = doc.page_sizes[self.viewport.current_page];
        let (rpw, rph) = self.viewport.rotated_page_size(pw, ph);
        let zoom = match self.viewport.fit_mode {
            FitMode::FitWidth => available_w / rpw,
            FitMode::FitPage => (available_w / rpw).min(available_h / rph),
            FitMode::Custom => return None,
        };
        Some(zoom.clamp(0.25, 5.0))
    }

    /// Get page size accounting for rotation
    fn effective_page_size(&self, page_no: usize) -> (f32, f32) {
        if let Some(ref doc) = self.document {
            if page_no < doc.page_sizes.len() {
                let (pw, ph) = doc.page_sizes[page_no];
                return self.viewport.rotated_page_size(pw, ph);
            }
        }
        (612.0, 792.0)
    }

    /// Get all chars for a page, sorted in reading order (top-to-bottom, left-to-right)
    fn get_page_chars_sorted(&self, page_no: usize) -> Vec<crate::backend::PageChar> {
        let path = match self.document.as_ref() {
            Some(d) => d.path.clone(),
            None => return Vec::new(),
        };
        let backend = MuPdfBackend;
        let doc = match backend.open_file(&path) { Ok(d) => d, Err(_) => return Vec::new() };
        doc.page_chars(page_no).unwrap_or_default()
        // MuPDF already returns chars in reading order (block→line→char)
    }

    /// Find the nearest character index to a point in page coordinates
    fn find_nearest_char(chars: &[crate::backend::PageChar], x: f32, y: f32) -> usize {
        if chars.is_empty() { return 0; }
        let mut best_idx = 0;
        let mut best_dist = f32::MAX;
        for (i, ch) in chars.iter().enumerate() {
            let cx = (ch.x0 + ch.x1) / 2.0;
            let cy = (ch.y0 + ch.y1) / 2.0;
            let dist = (cx - x).powi(2) + (cy - y).powi(2);
            if dist < best_dist {
                best_dist = dist;
                best_idx = i;
            }
        }
        best_idx
    }

    /// Select characters between two positions in reading order (text flow)
    fn select_chars_by_flow(&self, page_no: usize, start_x: f32, start_y: f32, end_x: f32, end_y: f32) -> (Vec<crate::backend::PageChar>, String) {
        let chars = self.get_page_chars_sorted(page_no);
        if chars.is_empty() { return (Vec::new(), String::new()); }

        let start_idx = Self::find_nearest_char(&chars, start_x, start_y);
        let end_idx = Self::find_nearest_char(&chars, end_x, end_y);

        let from = start_idx.min(end_idx);
        let to = start_idx.max(end_idx);

        let mut selected = Vec::new();
        let mut text = String::new();
        let mut last_y: Option<f32> = None;

        for i in from..=to.min(chars.len() - 1) {
            let ch = &chars[i];
            let cy = (ch.y0 + ch.y1) / 2.0;
            // Insert newline when Y changes (new line in PDF)
            if let Some(ly) = last_y {
                if (cy - ly).abs() > 3.0 {
                    text.push('\n');
                }
            }
            last_y = Some(cy);
            text.push(ch.c);
            selected.push(ch.clone());
        }

        (selected, text)
    }

    /// Select all text on a page
    fn select_all_chars(&self, page_no: usize) -> (Vec<crate::backend::PageChar>, String) {
        let chars = self.get_page_chars_sorted(page_no);
        if chars.is_empty() { return (Vec::new(), String::new()); }
        self.select_chars_by_flow(page_no, 0.0, 0.0, 10000.0, 10000.0)
    }

    /// Print the current document
    fn print_document(&self) {
        let path = match self.document.as_ref() {
            Some(d) => &d.path,
            None => return,
        };

        // Try multiple methods in order of preference
        #[cfg(target_os = "linux")]
        {
            // WSL2: use Windows print via powershell
            let wsl_print = std::process::Command::new("powershell.exe")
                .args(["-Command", &format!("Start-Process -FilePath '{}' -Verb Print",
                    path.to_string_lossy())])
                .spawn();

            if wsl_print.is_err() {
                // Native Linux: try lp first, then xdg-open as fallback
                let lp = std::process::Command::new("lp")
                    .arg(path)
                    .spawn();
                if lp.is_err() {
                    let _ = std::process::Command::new("xdg-open")
                        .arg(path)
                        .spawn();
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            // Windows: ShellExecute with "print" verb
            let _ = std::process::Command::new("powershell")
                .args(["-Command", &format!("Start-Process -FilePath '{}' -Verb Print",
                    path.to_string_lossy())])
                .spawn();
        }
    }

    /// Open URL in system browser (WSL2 compatible)
    fn open_url(&self, url: &str) {
        // Try WSL2 method first, then native
        #[cfg(target_os = "linux")]
        {
            let wsl = std::process::Command::new("wslview")
                .arg(url)
                .spawn();
            if wsl.is_err() {
                let cmd2 = std::process::Command::new("cmd.exe")
                    .args(["/C", "start", url])
                    .spawn();
                if cmd2.is_err() {
                    let _ = open::that(url);
                }
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = open::that(url);
        }
    }

    /// Save current reading position
    fn save_reading_position(&mut self) {
        if let Some(ref doc) = self.document {
            let key = doc.path.to_string_lossy().to_string();
            self.settings.save_last_page(&key, self.viewport.current_page);
        }
    }

    /// Close document, go back to welcome screen
    fn close_document(&mut self, ctx: &egui::Context) {
        self.save_reading_position();
        self.document = None;
        self.pipeline = None;
        self.cache.clear();
        self.pending_pages.clear();
        self.search = SearchState::default();
        self.selection = TextSelection::default();
        self.page_rects.clear();
        self.page_links_cache.clear();
        self.settings.save();
        ctx.send_viewport_cmd(egui::ViewportCommand::Title("Simple PDF Reader".into()));
    }

    fn copy_to_clipboard(&mut self, text: &str) {
        if text.is_empty() { return; }
        if let Some(ref mut cb) = self.clipboard {
            let _ = cb.set_text(text);
        }
    }

    /// Get links for a page (cached)
    fn get_page_links(&mut self, page_no: usize) -> Vec<PageLink> {
        if let Some(links) = self.page_links_cache.get(&page_no) {
            return links.clone();
        }
        let path = match self.document.as_ref() {
            Some(d) => d.path.clone(),
            None => return Vec::new(),
        };
        let backend = MuPdfBackend;
        let doc = match backend.open_file(&path) { Ok(d) => d, Err(_) => return Vec::new() };
        let links = doc.page_links(page_no).unwrap_or_default();
        self.page_links_cache.insert(page_no, links.clone());
        links
    }

    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        let page_count = self.document.as_ref().map(|d| d.page_count).unwrap_or(0);

        ctx.input(|i| {
            let ctrl = i.modifiers.ctrl || i.modifiers.mac_cmd;
            let shift = i.modifiers.shift;

            if ctrl && i.key_pressed(egui::Key::O) { self.open_file_dialog(ctx); }
            if ctrl && i.key_pressed(egui::Key::W) { self.close_document(ctx); }
            if ctrl && i.key_pressed(egui::Key::F) { self.search.visible = !self.search.visible; }

            // Escape: exit fullscreen, close dialogs, or clear selection
            if i.key_pressed(egui::Key::Escape) {
                if self.viewport.fullscreen {
                    self.viewport.fullscreen = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                } else if self.viewport.goto_dialog_open {
                    self.viewport.goto_dialog_open = false;
                } else if self.viewport.properties_open {
                    self.viewport.properties_open = false;
                } else {
                    self.search.visible = false;
                    self.selection = TextSelection::default();
                }
            }

            // F11: toggle fullscreen
            if i.key_pressed(egui::Key::F11) {
                self.viewport.fullscreen = !self.viewport.fullscreen;
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.viewport.fullscreen));
            }

            // Ctrl+G: go to page dialog
            if ctrl && i.key_pressed(egui::Key::G) {
                self.viewport.goto_dialog_open = !self.viewport.goto_dialog_open;
                self.viewport.goto_input.clear();
                self.viewport.goto_error.clear();
            }

            // Ctrl+I: properties dialog
            if ctrl && i.key_pressed(egui::Key::I) {
                self.viewport.properties_open = !self.viewport.properties_open;
            }

            // Ctrl+R: rotate CW, Ctrl+Shift+R: rotate CCW
            if ctrl && i.key_pressed(egui::Key::R) {
                if shift {
                    self.viewport.rotate_ccw();
                } else {
                    self.viewport.rotate_cw();
                }
                self.cache.clear();
                self.pending_pages.clear();
            }

            if ctrl && i.key_pressed(egui::Key::B) {
                let key = self.file_key();
                self.settings.toggle_bookmark(&key, self.viewport.current_page);
                self.settings.save();
            }
            if ctrl && i.key_pressed(egui::Key::D) {
                self.settings.dark_mode = !self.settings.dark_mode;
                self.settings.save();
            }
            if ctrl && i.key_pressed(egui::Key::L) { self.sidebar_open = !self.sidebar_open; }
            if ctrl && i.key_pressed(egui::Key::M) { self.viewport.toggle_view_mode(); }
            if ctrl && i.key_pressed(egui::Key::H) { self.viewport.toggle_cursor_mode(); }
            if ctrl && i.key_pressed(egui::Key::P) { self.print_document(); }

            // Ctrl+C: copy selected text
            if ctrl && i.key_pressed(egui::Key::C) && !self.selection.text.is_empty() {
                let text = self.selection.text.clone();
                self.copy_to_clipboard(&text);
            }

            // Ctrl+A: select all text on current page
            if ctrl && i.key_pressed(egui::Key::A) {
                let (chars, text) = self.select_all_chars(self.viewport.current_page);
                if !text.is_empty() {
                    self.selection.selected_chars = chars;
                    self.selection.text = text;
                    self.selection.page_no = self.viewport.current_page;
                }
            }

            // === Cursor-centered zoom ===
            // All zoom methods go through zoom_at_cursor() for consistent behavior
            let cursor_pos = i.pointer.hover_pos(); // screen position of cursor
            let old_zoom = self.viewport.zoom;
            let mut new_zoom = old_zoom;

            // Keyboard zoom
            if ctrl && i.key_pressed(egui::Key::Plus) {
                new_zoom = (old_zoom * 1.15).min(5.0);
            }
            if ctrl && i.key_pressed(egui::Key::Minus) {
                new_zoom = (old_zoom / 1.15).max(0.25);
            }

            // Pinch-to-zoom (trackpad) + Ctrl+scroll
            let pinch = i.zoom_delta();
            if (pinch - 1.0).abs() > 0.001 {
                new_zoom = (old_zoom * pinch).clamp(0.25, 5.0);
            } else if ctrl {
                let scroll = i.raw_scroll_delta.y;
                if scroll > 0.0 { new_zoom = (old_zoom * 1.15).min(5.0); }
                else if scroll < 0.0 { new_zoom = (old_zoom / 1.15).max(0.25); }
            }

            // Apply zoom centered on cursor
            if (new_zoom - old_zoom).abs() > 0.001 {
                if let Some(cursor) = cursor_pos {
                    // Convert screen-absolute cursor to panel-relative coordinates
                    let rel_x = cursor.x - self.panel_origin.x;
                    let rel_y = cursor.y - self.panel_origin.y;
                    // Document position under cursor before zoom
                    let doc_x = (self.scroll_offset.x + rel_x) / old_zoom;
                    let doc_y = (self.scroll_offset.y + rel_y) / old_zoom;
                    // Adjust scroll so same doc position stays under cursor
                    self.scroll_offset.x = (doc_x * new_zoom - rel_x).max(0.0);
                    self.scroll_offset.y = (doc_y * new_zoom - rel_y).max(0.0);
                }
                self.viewport.zoom = new_zoom;
                self.viewport.fit_mode = FitMode::Custom;
                self.pending_pages.clear();
            }

            // Inertial scroll in continuous mode
            if !ctrl && self.viewport.view_mode == ViewMode::Continuous {
                let scroll = i.raw_scroll_delta;
                if scroll.length() > 0.5 {
                    self.inertial.impulse(scroll);
                    if scroll.y.abs() > 0.5 {
                        self.viewport.last_scroll_direction = -scroll.y;
                    }
                }
            }

            // Single/Dual page: scroll changes page ONLY when not zoomed in
            // When zoomed in, ScrollArea handles internal scrolling.
            // Page change happens when scroll reaches the edge (handled after ScrollArea.show()).
            // Here we only handle the simple case: page fits on screen.
            let page_fits_screen = self.viewport.fit_mode == FitMode::FitPage
                || self.viewport.fit_mode == FitMode::FitWidth
                || self.viewport.zoom <= 1.0;

            if !ctrl && self.viewport.view_mode == ViewMode::SinglePage && page_fits_screen {
                let scroll = i.raw_scroll_delta.y;
                if scroll < -30.0 {
                    self.viewport.next_page(page_count);
                    self.scroll_to_page = Some(self.viewport.current_page);
                } else if scroll > 30.0 {
                    self.viewport.prev_page();
                    self.scroll_to_page = Some(self.viewport.current_page);
                }
            }

            if !ctrl && self.viewport.view_mode == ViewMode::DualPage && page_fits_screen {
                let scroll = i.raw_scroll_delta.y;
                let left_page = if self.viewport.current_page % 2 == 0 {
                    self.viewport.current_page
                } else {
                    self.viewport.current_page.saturating_sub(1)
                };
                if scroll < -30.0 {
                    let new_page = if left_page + 2 < page_count { left_page + 2 } else { left_page };
                    self.viewport.go_to_page(new_page, page_count);
                } else if scroll > 30.0 {
                    let new_page = left_page.saturating_sub(2);
                    // Ensure aligned to even
                    let new_page = new_page - (new_page % 2);
                    self.viewport.go_to_page(new_page, page_count);
                }
            }

            if !ctrl {
                if i.key_pressed(egui::Key::PageDown) {
                    self.viewport.next_page(page_count);
                    self.scroll_to_page = Some(self.viewport.current_page);
                }
                if i.key_pressed(egui::Key::PageUp) {
                    self.viewport.prev_page();
                    self.scroll_to_page = Some(self.viewport.current_page);
                }
                if i.key_pressed(egui::Key::Home) {
                    self.viewport.go_to_page(0, page_count);
                    self.scroll_to_page = Some(0);
                }
                if i.key_pressed(egui::Key::End) {
                    self.viewport.go_to_page(page_count.saturating_sub(1), page_count);
                    self.scroll_to_page = Some(self.viewport.current_page);
                }
            }

            // Space / Shift+Space: scroll like SumatraPDF
            if !ctrl && i.key_pressed(egui::Key::Space) {
                if shift {
                    self.viewport.prev_page();
                    self.scroll_to_page = Some(self.viewport.current_page);
                } else {
                    self.viewport.next_page(page_count);
                    self.scroll_to_page = Some(self.viewport.current_page);
                }
            }

            // F3 search nav
            if i.key_pressed(egui::Key::F3) {
                if shift { self.navigate_search(false); }
                else { self.navigate_search(true); }
            }

            // Mouse back/forward buttons
            if i.pointer.button_clicked(egui::PointerButton::Extra1) {
                self.viewport.prev_page();
                self.scroll_to_page = Some(self.viewport.current_page);
            }
            if i.pointer.button_clicked(egui::PointerButton::Extra2) {
                self.viewport.next_page(page_count);
                self.scroll_to_page = Some(self.viewport.current_page);
            }
        });
    }

    fn open_file_dialog(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("PDF files", &["pdf"])
            .add_filter("All files", &["*"])
            .pick_file()
        {
            self.try_open_document(&path, ctx);
        }
    }

    /// Draw a centered page (for Continuous/Single modes)
    fn draw_page(
        &mut self, ui: &mut egui::Ui, page_no: usize,
        page_w: f32, page_h: f32, zoom: f32, available_w: f32, dark_mode: bool,
    ) {
        let offset_x = ((available_w - page_w - 8.0) / 2.0).max(0.0);
        ui.horizontal(|ui| {
            if offset_x > 0.0 { ui.add_space(offset_x); }
            self.draw_page_content(ui, page_no, page_w, page_h, zoom, dark_mode);
        });
    }

    /// Draw a page without centering (for DualPage mode)
    fn draw_page_no_center(
        &mut self, ui: &mut egui::Ui, page_no: usize,
        page_w: f32, page_h: f32, zoom: f32, dark_mode: bool,
    ) {
        self.draw_page_content(ui, page_no, page_w, page_h, zoom, dark_mode);
    }

    fn draw_page_content(
        &mut self, ui: &mut egui::Ui, page_no: usize,
        page_w: f32, page_h: f32, zoom: f32, dark_mode: bool,
    ) {
        let rotation = self.viewport.rotation;
        let has_exact = self.cache.contains(page_no, zoom, rotation);
        let texture = if has_exact {
            self.cache.get(page_no, zoom, rotation)
        } else {
            self.cache.get_any_scale(page_no)
        };

        let page_size = egui::Vec2::new(page_w, page_h);

        if let Some(tex) = texture {
            let display_size = if has_exact { tex.size_vec2() } else { page_size };

            // Reserve space
            let (rect, _) = ui.allocate_exact_size(
                display_size + egui::Vec2::new(4.0, 4.0), egui::Sense::hover(),
            );
            let page_rect = egui::Rect::from_min_size(rect.min, display_size);
            let painter = ui.painter();

            // Shadow
            painter.rect_filled(
                page_rect.translate(egui::Vec2::new(2.0, 2.0)), 0.0,
                egui::Color32::from_rgba_unmultiplied(0, 0, 0, 50),
            );
            // White background
            painter.rect_filled(page_rect, 0.0, egui::Color32::WHITE);
            // Image
            painter.image(
                tex.id(), page_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
            // Border
            painter.rect_stroke(page_rect, 0.0,
                egui::Stroke::new(1.0, egui::Color32::from_gray(180)), egui::StrokeKind::Outside);

            self.page_rects.insert(page_no, page_rect);

            // Search highlights (brighter orange for visibility)
            for hit in &self.search.results {
                if hit.page_no == page_no {
                    for quad in &hit.quads {
                        let min_x = quad[0].min(quad[4]) * zoom;
                        let min_y = quad[1].min(quad[3]) * zoom;
                        let max_x = quad[2].max(quad[6]) * zoom;
                        let max_y = quad[5].max(quad[7]) * zoom;
                        let r = egui::Rect::from_min_max(
                            page_rect.min + egui::Vec2::new(min_x, min_y),
                            page_rect.min + egui::Vec2::new(max_x, max_y),
                        );
                        painter.rect_filled(r, 2.0,
                            egui::Color32::from_rgba_unmultiplied(255, 165, 0, 160));
                        painter.rect_stroke(r, 2.0,
                            egui::Stroke::new(1.0, egui::Color32::from_rgb(200, 100, 0)),
                            egui::StrokeKind::Outside);
                    }
                }
            }

            // Selection highlight
            if self.selection.page_no == page_no {
                let sel_color = egui::Color32::from_rgba_unmultiplied(70, 130, 255, 100);
                if !self.selection.selected_chars.is_empty() {
                    for ch in &self.selection.selected_chars {
                        let r = egui::Rect::from_min_max(
                            page_rect.min + egui::Vec2::new(ch.x0 * zoom, ch.y0 * zoom),
                            page_rect.min + egui::Vec2::new(ch.x1 * zoom, ch.y1 * zoom),
                        );
                        painter.rect_filled(r, 0.0, sel_color);
                    }
                } else if self.selection.active {
                    let (sx, sy) = self.selection.start_page_pos;
                    let (ex, ey) = self.selection.end_page_pos;
                    let r = egui::Rect::from_min_max(
                        page_rect.min + egui::Vec2::new(sx.min(ex) * zoom, sy.min(ey) * zoom),
                        page_rect.min + egui::Vec2::new(sx.max(ex) * zoom, sy.max(ey) * zoom),
                    );
                    painter.rect_filled(r, 0.0, sel_color);
                }
            }
        } else {
            let (rect, _) = ui.allocate_exact_size(
                page_size + egui::Vec2::new(4.0, 4.0), egui::Sense::hover(),
            );
            let page_rect = egui::Rect::from_min_size(rect.min, page_size);
            let painter = ui.painter();
            painter.rect_filled(page_rect.translate(egui::Vec2::new(2.0, 2.0)), 0.0,
                egui::Color32::from_rgba_unmultiplied(0, 0, 0, 50));
            let bg = if dark_mode { egui::Color32::from_gray(50) } else { egui::Color32::from_gray(220) };
            painter.rect_filled(page_rect, 0.0, bg);
            painter.rect_stroke(page_rect, 0.0,
                egui::Stroke::new(1.0, egui::Color32::from_gray(180)), egui::StrokeKind::Outside);
            self.page_rects.insert(page_no, page_rect);
            painter.text(page_rect.center(), egui::Align2::CENTER_CENTER,
                format!("Loading page {}...", page_no + 1), egui::FontId::default(),
                if dark_mode { egui::Color32::GRAY } else { egui::Color32::DARK_GRAY });
        }
    }

    /// Draw password dialog
    fn draw_password_dialog(&mut self, ctx: &egui::Context) {
        egui::Window::new("Password Required")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("This PDF is password protected.");
                if self.password.error {
                    ui.colored_label(egui::Color32::RED, "Incorrect password. Try again.");
                }
                ui.horizontal(|ui| {
                    ui.label("Password:");
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.password.input)
                            .password(true)
                            .desired_width(200.0),
                    );
                    resp.request_focus();

                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.button("OK").clicked() || enter {
                        if let Some(path) = self.password.path.clone() {
                            let pw = self.password.input.clone();
                            if !self.open_document_with_password(&path, &pw, ctx) {
                                self.password.error = true;
                                self.password.input.clear();
                            }
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        self.password = PasswordState::default();
                    }
                });
            });
    }

    /// Draw Go To Page dialog (Feature 3)
    fn draw_goto_dialog(&mut self, ctx: &egui::Context) {
        let page_count = self.document.as_ref().map(|d| d.page_count).unwrap_or(0);

        let mut open = self.viewport.goto_dialog_open;
        egui::Window::new("Go to Page")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("Page (1-{}):", page_count));
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.viewport.goto_input)
                            .desired_width(80.0)
                            .horizontal_align(egui::Align::Center),
                    );
                    resp.request_focus();

                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if (ui.button("Go").clicked() || enter) && !self.viewport.goto_input.is_empty() {
                        match self.viewport.goto_input.trim().parse::<isize>() {
                            Ok(p) if p >= 1 && (p as usize) <= page_count => {
                                let page = (p as usize) - 1;
                                self.viewport.go_to_page(page, page_count);
                                self.scroll_to_page = Some(page);
                                self.viewport.goto_dialog_open = false;
                                self.viewport.goto_error.clear();
                            }
                            Ok(_) => {
                                self.viewport.goto_error = format!("Enter 1-{}", page_count);
                            }
                            Err(_) => {
                                self.viewport.goto_error = "Invalid number".to_string();
                            }
                        }
                    }
                });
                if !self.viewport.goto_error.is_empty() {
                    ui.colored_label(egui::Color32::RED, &self.viewport.goto_error);
                }
            });
        self.viewport.goto_dialog_open = open;
    }

    /// Draw Document Properties dialog (Feature 7)
    fn draw_properties_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.viewport.properties_open;
        egui::Window::new("Document Properties")
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .default_width(400.0)
            .open(&mut open)
            .show(ctx, |ui| {
                if let Some(ref doc) = self.document {
                    egui::Grid::new("properties_grid")
                        .num_columns(2)
                        .spacing([20.0, 6.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label("File:");
                            ui.label(doc.path.file_name().and_then(|f| f.to_str()).unwrap_or("Unknown"));
                            ui.end_row();

                            ui.label("Path:");
                            ui.label(doc.path.to_string_lossy().as_ref());
                            ui.end_row();

                            ui.label("Title:");
                            ui.label(doc.title.as_deref().unwrap_or("(none)"));
                            ui.end_row();

                            ui.label("Author:");
                            ui.label(self.doc_author.as_deref().unwrap_or("(none)"));
                            ui.end_row();

                            ui.label("Pages:");
                            ui.label(format!("{}", doc.page_count));
                            ui.end_row();

                            ui.label("File Size:");
                            if let Some(size) = self.doc_file_size {
                                let display = if size > 1_048_576 {
                                    format!("{:.1} MB", size as f64 / 1_048_576.0)
                                } else if size > 1024 {
                                    format!("{:.1} KB", size as f64 / 1024.0)
                                } else {
                                    format!("{} bytes", size)
                                };
                                ui.label(display);
                            } else {
                                ui.label("Unknown");
                            }
                            ui.end_row();
                        });
                }
            });
        self.viewport.properties_open = open;
    }

    /// Draw welcome screen with recent files
    fn draw_welcome(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.centered_and_justified(|ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() / 4.0);
                ui.heading("Simple PDF Reader");
                ui.add_space(8.0);
                ui.label("MuPDF-based portable PDF viewer");
                ui.add_space(16.0);

                if ui.button("Open PDF...").clicked() {
                    self.open_file_dialog(ctx);
                }
                ui.add_space(8.0);
                ui.label("or drag & drop a PDF file");

                // Recent files
                let recent = self.settings.recent_files.clone();
                if !recent.is_empty() {
                    ui.add_space(24.0);
                    ui.separator();
                    ui.add_space(8.0);
                    ui.label("Recent files:");
                    ui.add_space(4.0);

                    for path in recent.iter().take(10) {
                        let name = path.file_name()
                            .and_then(|f| f.to_str())
                            .unwrap_or("Unknown");
                        if ui.button(name).on_hover_text(path.to_string_lossy().as_ref()).clicked() {
                            if path.exists() {
                                self.try_open_document(path, ctx);
                            }
                        }
                    }
                }

                // Keyboard shortcuts help
                ui.add_space(24.0);
                ui.separator();
                ui.add_space(8.0);
                ui.label("Shortcuts:");
                let shortcuts = [
                    ("Ctrl+O", "Open file"),
                    ("Ctrl+F", "Search"),
                    ("Ctrl+G", "Go to page"),
                    ("Ctrl+H", "Hand/Select mode"),
                    ("Ctrl+M", "Cycle view mode"),
                    ("Ctrl+R", "Rotate CW"),
                    ("Ctrl+Shift+R", "Rotate CCW"),
                    ("Ctrl+I", "Document properties"),
                    ("Ctrl+D", "Dark/Light mode"),
                    ("Ctrl+B", "Bookmark"),
                    ("Ctrl+L", "Toggle sidebar"),
                    ("Ctrl+C", "Copy selected text"),
                    ("Ctrl+A", "Select all text on page"),
                    ("Ctrl+P", "Print"),
                    ("F11", "Fullscreen"),
                    ("Ctrl+Scroll", "Zoom"),
                    ("PageUp/Down", "Navigate pages"),
                ];
                for (key, desc) in shortcuts {
                    ui.horizontal(|ui| {
                        ui.monospace(format!("{:15}", key));
                        ui.label(desc);
                    });
                }
            });
        });
    }

    /// Handle character-level text selection in Select mode
    fn handle_text_selection(&mut self, ctx: &egui::Context) {
        if self.viewport.cursor_mode != CursorMode::Select { return; }

        let zoom = self.viewport.zoom;
        let pointer_pos = ctx.input(|i| i.pointer.interact_pos());
        let primary_pressed = ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary));
        let primary_down = ctx.input(|i| i.pointer.button_down(egui::PointerButton::Primary));
        let primary_released = ctx.input(|i| i.pointer.button_released(egui::PointerButton::Primary));

        if let Some(pos) = pointer_pos {
            let mut hit_page = None;
            for (&page_no, &rect) in &self.page_rects {
                if rect.contains(pos) {
                    hit_page = Some((page_no, rect));
                    break;
                }
            }

            if let Some((page_no, page_rect)) = hit_page {
                let page_x = (pos.x - page_rect.min.x) / zoom;
                let page_y = (pos.y - page_rect.min.y) / zoom;

                if primary_pressed {
                    self.selection = TextSelection {
                        page_no,
                        start_page_pos: (page_x, page_y),
                        end_page_pos: (page_x, page_y),
                        selected_chars: Vec::new(),
                        text: String::new(),
                        active: true,
                    };
                } else if primary_down && self.selection.active && self.selection.page_no == page_no {
                    self.selection.end_page_pos = (page_x, page_y);
                }

                if primary_released && self.selection.active {
                    self.selection.end_page_pos = (page_x, page_y);
                    self.selection.active = false;

                    // Text flow selection (reading order, not rectangle)
                    let (chars, text) = self.select_chars_by_flow(
                        self.selection.page_no,
                        self.selection.start_page_pos.0,
                        self.selection.start_page_pos.1,
                        self.selection.end_page_pos.0,
                        self.selection.end_page_pos.1,
                    );
                    self.selection.selected_chars = chars;
                    self.selection.text = text.clone();

                    if !text.is_empty() {
                        self.copy_to_clipboard(&text);
                    }
                }
            }
        }
    }

    /// Handle link hover and click in Hand mode (Feature 5)
    fn handle_links(&mut self, ctx: &egui::Context) {
        if self.viewport.cursor_mode != CursorMode::Hand { return; }
        if self.document.is_none() { return; }

        let zoom = self.viewport.zoom;
        let pointer_pos = ctx.input(|i| i.pointer.interact_pos());
        let primary_clicked = ctx.input(|i| i.pointer.button_clicked(egui::PointerButton::Primary));

        if let Some(pos) = pointer_pos {
            // Collect page_no and rect pairs to avoid borrowing self.page_rects during mutation
            let hit_pages: Vec<(usize, egui::Rect)> = self.page_rects.iter()
                .filter(|(_, rect)| rect.contains(pos))
                .map(|(&page_no, &rect)| (page_no, rect))
                .collect();

            let page_count = self.document.as_ref().map(|d| d.page_count).unwrap_or(0);

            for (page_no, page_rect) in hit_pages {
                let page_x = (pos.x - page_rect.min.x) / zoom;
                let page_y = (pos.y - page_rect.min.y) / zoom;

                let links = self.get_page_links(page_no);
                let mut found_link = false;

                for link in &links {
                    if page_x >= link.x0 && page_x <= link.x1 && page_y >= link.y0 && page_y <= link.y1 {
                        found_link = true;
                        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);

                        if primary_clicked {
                            if let Some(target) = link.page_target {
                                // Internal link
                                self.viewport.go_to_page(target, page_count);
                                self.scroll_to_page = Some(target);
                            } else if let Some(ref uri) = link.uri {
                                // External link (WSL2 compatible)
                                self.open_url(uri);
                            }
                        }
                        break;
                    }
                }

                if found_link {
                    return; // Don't process further, cursor is on a link
                }
            }
        }
    }
}

impl eframe::App for PdfReaderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.settings.dark_mode {
            ctx.set_visuals(egui::Visuals::dark());
        } else {
            ctx.set_visuals(egui::Visuals::light());
        }

        // Password dialog
        if self.password.needed {
            self.draw_password_dialog(ctx);
            return;
        }

        // Drain completed renders
        let rotation = self.viewport.rotation;
        if let Some(ref mut pipeline) = self.pipeline {
            for result in pipeline.drain_results() {
                self.pending_pages.remove(&result.page.page_no);
                let name = format!("p{}s{}r{}", result.page.page_no, (result.page.scale * 100.0) as u32, rotation);
                let texture = pixels_to_texture(ctx, &result.page, &name);
                self.cache.insert(result.page.page_no, result.page.scale, rotation, texture);
            }
        }

        self.handle_keyboard(ctx);
        self.handle_text_selection(ctx);
        self.handle_links(ctx);

        let page_count = self.document.as_ref().map(|d| d.page_count).unwrap_or(0);

        let is_fullscreen = self.viewport.fullscreen;

        let sidebar_w = if self.sidebar_open && !is_fullscreen { 250.0 } else { 0.0 };
        let avail_w = ctx.available_rect().width() - sidebar_w - 40.0;
        let avail_h = ctx.available_rect().height() - if is_fullscreen { 20.0 } else { 100.0 };
        if self.viewport.needs_fit_recalc(avail_w) {
            if let Some(new_zoom) = self.calc_fit_zoom(avail_w, avail_h) {
                if (new_zoom - self.viewport.zoom).abs() > 0.01 {
                    self.viewport.zoom = new_zoom;
                    self.pending_pages.clear();
                }
            }
        }

        // === Go to page dialog ===
        if self.viewport.goto_dialog_open {
            self.draw_goto_dialog(ctx);
        }

        // === Properties dialog ===
        if self.viewport.properties_open {
            self.draw_properties_dialog(ctx);
        }

        // === Fullscreen: show exit hint bar at top ===
        if is_fullscreen {
            egui::TopBottomPanel::top("fullscreen_hint").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Fullscreen - press F11 or Esc to exit");
                    if ui.small_button("Exit Fullscreen").clicked() {
                        self.viewport.fullscreen = false;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                    }
                });
            });
        }

        // === Toolbar (hidden in fullscreen) ===
        if !is_fullscreen {
            egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
                let is_bm = self.settings.is_bookmarked(&self.file_key(), self.viewport.current_page);
                let action = toolbar::draw_toolbar(
                    ui, &mut self.viewport, page_count, is_bm, self.settings.dark_mode, &mut self.page_input,
                );

                if action.open_file { self.open_file_dialog(ctx); }
                if action.toggle_sidebar { self.sidebar_open = !self.sidebar_open; }
                if action.toggle_search { self.search.visible = !self.search.visible; }
                if action.toggle_bookmark {
                    let key = self.file_key();
                    self.settings.toggle_bookmark(&key, self.viewport.current_page);
                    self.settings.save();
                }
                if action.toggle_dark_mode {
                    self.settings.dark_mode = !self.settings.dark_mode;
                    self.settings.save();
                }
                if action.toggle_view_mode { self.viewport.toggle_view_mode(); }
                if action.toggle_cursor_mode { self.viewport.toggle_cursor_mode(); }
                if action.print { self.print_document(); }
                if action.close_document { self.close_document(ctx); }
                if action.register_default { crate::register_as_default(); }
                if action.unregister_default { crate::unregister_as_default(); }
                if action.rotate_cw {
                    self.viewport.rotate_cw();
                    self.cache.clear();
                    self.pending_pages.clear();
                }
                if action.rotate_ccw {
                    self.viewport.rotate_ccw();
                    self.cache.clear();
                    self.pending_pages.clear();
                }
                if action.toggle_fullscreen {
                    self.viewport.fullscreen = !self.viewport.fullscreen;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.viewport.fullscreen));
                }
                if action.goto_dialog {
                    self.viewport.goto_dialog_open = !self.viewport.goto_dialog_open;
                    self.viewport.goto_input.clear();
                self.viewport.goto_error.clear();
                }
                if action.toggle_properties {
                    self.viewport.properties_open = !self.viewport.properties_open;
                }
                if let Some(page) = action.go_to_page {
                    self.viewport.go_to_page(page, page_count);
                    self.scroll_to_page = Some(page);
                }
                if let Some(preset) = action.zoom_preset {
                    match preset {
                        ZoomPreset::Percent(pct) => {
                            self.viewport.zoom = pct as f32 / 100.0;
                            self.viewport.fit_mode = FitMode::Custom;
                            self.pending_pages.clear();
                        }
                        ZoomPreset::FitWidth => {
                            self.viewport.fit_mode = FitMode::FitWidth;
                            self.viewport.fit_recalc_needed = true;
                            self.pending_pages.clear();
                        }
                        ZoomPreset::FitPage => {
                            self.viewport.fit_mode = FitMode::FitPage;
                            self.viewport.fit_recalc_needed = true;
                            self.pending_pages.clear();
                        }
                    }
                }
                if action.zoom_changed { self.pending_pages.clear(); }
                if action.fit_changed { self.pending_pages.clear(); }

                self.page_input = format!("{}", self.viewport.current_page + 1);
            });
        }

        // === Search bar (hidden in fullscreen) ===
        if self.search.visible && !is_fullscreen {
            egui::TopBottomPanel::top("search_bar").show(ctx, |ui| {
                let action = search_bar::draw_search_bar(ui, &mut self.search);
                match action {
                    SearchAction::Search(query) => self.do_search(&query),
                    SearchAction::NextResult => self.navigate_search(true),
                    SearchAction::PrevResult => self.navigate_search(false),
                    SearchAction::Close => { self.search.visible = false; self.search.results.clear(); }
                    SearchAction::None => {}
                }
            });
        }

        // === Status bar (hidden in fullscreen) ===
        if !is_fullscreen {
            egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if let Some(ref doc) = self.document {
                        let title = doc.title.as_deref().unwrap_or_else(|| {
                            doc.path.file_name().and_then(|f| f.to_str()).unwrap_or("Unknown")
                        });
                        ui.label(title);
                        ui.separator();
                        let mode_str = match self.viewport.view_mode {
                            ViewMode::SinglePage => "Single",
                            ViewMode::Continuous => "Scroll",
                            ViewMode::DualPage => "Dual",
                        };
                        let cursor_str = match self.viewport.cursor_mode {
                            CursorMode::Hand => "Hand", CursorMode::Select => "Select",
                        };
                        let rot_str = if self.viewport.rotation != 0 {
                            format!(" | Rot {}deg", self.viewport.rotation)
                        } else {
                            String::new()
                        };
                        ui.label(format!("Page {} / {} | {:.0}% | {} | {}{}",
                            self.viewport.current_page + 1, doc.page_count,
                            self.viewport.zoom * 100.0, mode_str, cursor_str, rot_str));

                        if !self.search.results.is_empty() {
                            ui.separator();
                            let total: usize = self.search.results.iter().map(|h| h.quads.len()).sum();
                            ui.label(format!("Search: {} hits", total));
                        }
                        if !self.selection.text.is_empty() {
                            ui.separator();
                            ui.label("Text copied!");
                        }
                    } else {
                        ui.label("No document open. Press Ctrl+O or click Open.");
                    }
                });
            });
        }

        // === Sidebar (hidden in fullscreen) ===
        if self.sidebar_open && !is_fullscreen {
            egui::SidePanel::left("sidebar").default_width(250.0).show(ctx, |ui| {
                let outline = self.document.as_ref().map(|d| d.outline.as_slice()).unwrap_or(&[]);
                let file_key = self.file_key();
                let bookmarks = self.settings.get_bookmarks(&file_key);
                if let Some(page) = sidebar::draw_sidebar(ui, &mut self.sidebar_tab, outline, &bookmarks) {
                    self.viewport.go_to_page(page, page_count);
                    self.scroll_to_page = Some(page);
                }
            });
        }

        // === Central panel ===
        egui::CentralPanel::default().show(ctx, |ui| {
            // Store panel origin for cursor-centered zoom coordinate conversion
            self.panel_origin = ui.max_rect().min;

            if self.document.is_none() {
                self.draw_welcome(ui, ctx);
                return;
            }

            let zoom = self.viewport.zoom;
            let dark_mode = self.settings.dark_mode;
            let view_mode = self.viewport.view_mode;
            let available_w = ui.available_width();
            let page_gap = 12.0;
            let cursor_mode = self.viewport.cursor_mode;

            // Use effective (rotated) page sizes
            let page_sizes: Vec<(f32, f32)> = (0..page_count)
                .map(|i| self.effective_page_size(i))
                .collect();

            // Cursor & drag
            let panel_rect = ui.max_rect();
            let pointer_in_panel = ui.rect_contains_pointer(panel_rect);
            let primary_down = ui.input(|i| i.pointer.button_down(egui::PointerButton::Primary));
            let drag_delta = ui.input(|i| i.pointer.delta());
            let is_hand_dragging = cursor_mode == CursorMode::Hand
                && pointer_in_panel && primary_down && drag_delta.length_sq() > 4.0;

            match cursor_mode {
                CursorMode::Hand => {
                    if is_hand_dragging {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                        self.scroll_offset.x = (self.scroll_offset.x - drag_delta.x).max(0.0);
                        self.scroll_offset.y = (self.scroll_offset.y - drag_delta.y).max(0.0);
                        self.inertial.stop(); // stop inertia when manually dragging
                    } else if pointer_in_panel {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                    }
                }
                CursorMode::Select => {
                    if pointer_in_panel {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
                    }
                }
            }

            // Check if page fits on screen (used for scroll→page change logic)
            let page_fits_screen = self.viewport.fit_mode == FitMode::FitPage
                || self.viewport.fit_mode == FitMode::FitWidth
                || self.viewport.zoom <= 1.0;

            // Inertial scroll tick
            let inertial_delta = self.inertial.tick();
            if inertial_delta.length() > 0.0 {
                self.scroll_offset.y = (self.scroll_offset.y - inertial_delta.y).max(0.0);
                self.scroll_offset.x = (self.scroll_offset.x - inertial_delta.x).max(0.0);
            }

            if let Some(target) = self.scroll_to_page.take() {
                let offset_y: f32 = page_sizes[..target].iter()
                    .map(|(_, h)| h * zoom + page_gap).sum();
                self.scroll_offset.y = offset_y;
                self.scroll_offset.x = 0.0;
            }

            // Clear page rects for this frame
            self.page_rects.clear();

            match view_mode {
                ViewMode::Continuous => {
                    // Pre-compute cumulative heights for binary search
                    let mut cum_heights = Vec::with_capacity(page_count + 1);
                    cum_heights.push(0.0f32);
                    for i in 0..page_count {
                        let h = page_sizes[i].1 * zoom + page_gap;
                        cum_heights.push(cum_heights[i] + h);
                    }
                    let total_height = *cum_heights.last().unwrap_or(&0.0);
                    let max_page_w: f32 = page_sizes.iter().map(|(w, _)| w * zoom).fold(0.0f32, f32::max);
                    let content_w = max_page_w.max(available_w);

                    let scroll_area = egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .scroll_source(egui::scroll_area::ScrollSource {
                            scroll_bar: true,
                            drag: cursor_mode == CursorMode::Hand,
                            mouse_wheel: true,
                        })
                        .vertical_scroll_offset(self.scroll_offset.y)
                        .horizontal_scroll_offset(self.scroll_offset.x);

                    let output = scroll_area.show(ui, |ui| {
                        let clip = ui.clip_rect();
                        let content_top = ui.min_rect().top();
                        let scroll_y = clip.top() - content_top;
                        let viewport_h = clip.height();

                        // Binary search for visible page range
                        let view_top = (scroll_y - 300.0).max(0.0);
                        let view_bottom = scroll_y + viewport_h + 300.0;

                        let first_visible = cum_heights.partition_point(|&h| h < view_top).saturating_sub(1);
                        let last_visible = cum_heights.partition_point(|&h| h < view_bottom).min(page_count);

                        let mut visible_pages = Vec::new();

                        // Space before visible pages (single allocation)
                        let pre_height = cum_heights[first_visible];
                        if pre_height > 0.0 {
                            ui.allocate_space(egui::Vec2::new(content_w, pre_height));
                        }

                        // Draw only visible pages
                        for page_no in first_visible..last_visible {
                            visible_pages.push(page_no);
                            let (pw, ph) = page_sizes[page_no];
                            let w = pw * zoom;
                            let h = ph * zoom;
                            self.draw_page(ui, page_no, w, h, zoom, content_w, dark_mode);
                            ui.add_space(page_gap);
                        }

                        // Space after visible pages
                        let post_height = total_height - cum_heights[last_visible];
                        if post_height > 0.0 {
                            ui.allocate_space(egui::Vec2::new(content_w, post_height));
                        }

                        // Update current page
                        if let Some(&first) = visible_pages.first() {
                            if self.viewport.current_page != first {
                                self.viewport.current_page = first;
                            }
                        }

                        self.prefetch_pages(&visible_pages);
                    });

                    // Don't update scroll offset during active text selection
                    if !(cursor_mode == CursorMode::Select && self.selection.active) {
                        self.scroll_offset = output.state.offset;
                    }
                }

                ViewMode::SinglePage => {
                    let page_no = self.viewport.current_page;
                    if page_no < page_sizes.len() {
                        let (pw, ph) = page_sizes[page_no];
                        let page_w = pw * zoom;
                        let page_h = ph * zoom;

                        self.request_render(page_no, zoom);
                        let mut adjacent = vec![page_no];
                        if page_no > 0 { adjacent.push(page_no - 1); }
                        if page_no + 1 < page_count { adjacent.push(page_no + 1); }
                        if page_no + 2 < page_count { adjacent.push(page_no + 2); }
                        self.prefetch_pages(&adjacent);

                        let scroll_area = egui::ScrollArea::both()
                            .auto_shrink([false, false])
                            .vertical_scroll_offset(self.scroll_offset.y)
                            .horizontal_scroll_offset(self.scroll_offset.x);

                        let output = scroll_area.show(ui, |ui| {
                            ui.add_space(4.0);
                            self.draw_page(ui, page_no, page_w, page_h, zoom, available_w, dark_mode);
                            ui.add_space(4.0);
                        });

                        // Don't update scroll offset during active text selection
                        if !(cursor_mode == CursorMode::Select && self.selection.active) {
                            self.scroll_offset = output.state.offset;
                        }

                        // When zoomed in: detect scroll hitting bottom/top edge → change page
                        if !page_fits_screen {
                            let content_h = page_h + 8.0;
                            let view_h = output.inner_rect.height();
                            let at_bottom = self.scroll_offset.y >= (content_h - view_h - 2.0).max(0.0);
                            let at_top = self.scroll_offset.y <= 1.0;
                            let scroll_delta = ctx.input(|i| i.raw_scroll_delta.y);

                            if at_bottom && scroll_delta < -10.0 {
                                self.viewport.next_page(page_count);
                                self.scroll_to_page = Some(self.viewport.current_page);
                                self.scroll_offset.y = 0.0;
                            } else if at_top && scroll_delta > 10.0 {
                                self.viewport.prev_page();
                                // Start at bottom of previous page
                                self.scroll_offset.y = f32::MAX;
                            }
                        }
                    }
                }

                ViewMode::DualPage => {
                    let current = self.viewport.current_page;
                    let left_page = if current % 2 == 0 { current } else { current.saturating_sub(1) };
                    let right_page = left_page + 1;

                    // Request renders
                    if left_page < page_count { self.request_render(left_page, zoom); }
                    if right_page < page_count { self.request_render(right_page, zoom); }
                    let mut adj = Vec::new();
                    if left_page >= 2 { adj.push(left_page - 2); adj.push(left_page - 1); }
                    if right_page + 1 < page_count { adj.push(right_page + 1); }
                    if right_page + 2 < page_count { adj.push(right_page + 2); }
                    self.prefetch_pages(&adj);

                    let scroll_area = egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .scroll_source(egui::scroll_area::ScrollSource {
                            scroll_bar: true,
                            drag: cursor_mode == CursorMode::Hand,
                            mouse_wheel: true,
                        })
                        .vertical_scroll_offset(self.scroll_offset.y)
                        .horizontal_scroll_offset(self.scroll_offset.x);

                    let output = scroll_area.show(ui, |ui| {
                        // Calculate page sizes
                        let (lpw, lph) = if left_page < page_count { page_sizes[left_page] } else { (612.0, 792.0) };
                        let (rpw, rph) = if right_page < page_count { page_sizes[right_page] } else { (lpw, lph) };
                        let lw = lpw * zoom;
                        let lh = lph * zoom;
                        let rw = rpw * zoom;
                        let rh = rph * zoom;
                        let total_w = lw + rw + page_gap + 16.0;
                        let center_offset = ((available_w - total_w) / 2.0).max(0.0);

                        ui.add_space(4.0);
                        // Use columns approach: allocate a horizontal group
                        ui.horizontal(|ui| {
                            ui.add_space(center_offset);
                            // Left page - draw directly without centering
                            if left_page < page_count {
                                self.draw_page_no_center(ui, left_page, lw, lh, zoom, dark_mode);
                            } else {
                                ui.allocate_space(egui::Vec2::new(lw + 8.0, lh + 8.0));
                            }
                            ui.add_space(page_gap);
                            // Right page
                            if right_page < page_count {
                                self.draw_page_no_center(ui, right_page, rw, rh, zoom, dark_mode);
                            } else {
                                ui.allocate_space(egui::Vec2::new(rw + 8.0, rh + 8.0));
                            }
                        });
                        ui.add_space(4.0);
                    });

                    // Don't update scroll offset during active text selection
                    if !(cursor_mode == CursorMode::Select && self.selection.active) {
                        self.scroll_offset = output.state.offset;
                    }
                }
            }
        });

        // Right-click context menu
        if self.document.is_some() {
            let secondary_clicked = ctx.input(|i| i.pointer.button_clicked(egui::PointerButton::Secondary));
            if secondary_clicked {
                if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                    self.context_menu_pos = Some(pos);
                }
            }
            if let Some(_pos) = self.context_menu_pos {
                let mut close_menu = false;
                egui::Area::new(egui::Id::new("context_menu"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(_pos)
                    .show(ctx, |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            if !self.selection.text.is_empty() {
                                if ui.button("Copy              Ctrl+C").clicked() {
                                    let text = self.selection.text.clone();
                                    self.copy_to_clipboard(&text);
                                    close_menu = true;
                                }
                            }
                            if ui.button("Select All        Ctrl+A").clicked() {
                                let (chars, text) = self.select_all_chars(self.viewport.current_page);
                                self.selection.selected_chars = chars;
                                self.selection.text = text;
                                self.selection.page_no = self.viewport.current_page;
                                close_menu = true;
                            }
                            ui.separator();
                            if ui.button("Fit Width").clicked() {
                                self.viewport.fit_mode = FitMode::FitWidth;
                                self.viewport.fit_recalc_needed = true;
                                self.pending_pages.clear();
                                close_menu = true;
                            }
                            if ui.button("Fit Page").clicked() {
                                self.viewport.fit_mode = FitMode::FitPage;
                                self.viewport.fit_recalc_needed = true;
                                self.pending_pages.clear();
                                close_menu = true;
                            }
                            ui.separator();
                            if ui.button("Properties        Ctrl+I").clicked() {
                                self.viewport.properties_open = true;
                                close_menu = true;
                            }
                        });
                    });
                // Close on click outside or after action
                let any_click = ctx.input(|i|
                    i.pointer.button_clicked(egui::PointerButton::Primary)
                    || i.key_pressed(egui::Key::Escape));
                if close_menu || any_click {
                    self.context_menu_pos = None;
                }
            }
        }

        // File drop
        ctx.input(|i| {
            if !i.raw.dropped_files.is_empty() {
                if let Some(path) = i.raw.dropped_files[0].path.clone() {
                    if path.extension().map(|e| e == "pdf").unwrap_or(false) {
                        self.try_open_document(&path, ctx);
                    }
                }
            }
        });

        // Repaint if renders pending or inertial scroll active
        if self.pipeline.as_ref().map(|p| p.has_pending()).unwrap_or(false)
            || self.inertial.active
        {
            ctx.request_repaint();
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_reading_position();
        self.settings.save();
    }
}
