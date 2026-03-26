pub mod settings;

use crate::backend::OutlineEntry;
use crate::backend::PageChar;

pub struct DocumentState {
    pub path: std::path::PathBuf,
    pub page_count: usize,
    pub outline: Vec<OutlineEntry>,
    pub title: Option<String>,
    pub page_sizes: Vec<(f32, f32)>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewMode {
    SinglePage,
    Continuous,
    DualPage,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FitMode {
    FitWidth,
    FitPage,
    Custom,
}

pub struct ViewportState {
    pub current_page: usize,
    pub zoom: f32,
    pub fit_mode: FitMode,
    pub view_mode: ViewMode,
    pub cursor_mode: CursorMode,
    last_available_width: f32,
    pub fit_recalc_needed: bool,
    pub rotation: i32,
    pub fullscreen: bool,
    pub goto_dialog_open: bool,
    pub goto_input: String,
    pub properties_open: bool,
    pub goto_error: String,
    pub last_scroll_direction: f32,
    pub about_open: bool,
    pub extract_dialog_open: bool,
    pub extract_input: String,
    pub extract_status: String,
}

// No debounce - SumatraPDF/PDF.js approach:
// Show old texture scaled immediately, render correct resolution in background.
// The pipeline worker drains stale requests automatically.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CursorMode {
    Hand,   // drag to pan
    Select, // text selection
}


impl Default for ViewportState {
    fn default() -> Self {
        Self {
            current_page: 0,
            zoom: 1.0,
            fit_mode: FitMode::FitPage,
            view_mode: ViewMode::SinglePage,
            cursor_mode: CursorMode::Hand,
            last_available_width: 0.0,
            fit_recalc_needed: true,
            rotation: 0,
            fullscreen: false,
            goto_dialog_open: false,
            goto_input: String::new(),
            properties_open: false,
            goto_error: String::new(),
            about_open: false,
            extract_dialog_open: false,
            extract_input: String::new(),
            extract_status: String::new(),
            last_scroll_direction: 0.0,
        }
    }
}

impl ViewportState {
    pub fn zoom_in(&mut self) {
        self.zoom = (self.zoom * 1.15).min(5.0);
        self.fit_mode = FitMode::Custom;
    }

    pub fn zoom_out(&mut self) {
        self.zoom = (self.zoom / 1.15).max(0.25);
        self.fit_mode = FitMode::Custom;
    }

    pub fn go_to_page(&mut self, page: usize, max_pages: usize) {
        self.current_page = page.min(max_pages.saturating_sub(1));
    }

    pub fn next_page(&mut self, max_pages: usize) {
        if self.current_page + 1 < max_pages {
            self.current_page += 1;
        }
    }

    pub fn prev_page(&mut self) {
        if self.current_page > 0 {
            self.current_page -= 1;
        }
    }

    pub fn toggle_view_mode(&mut self) {
        self.view_mode = match self.view_mode {
            ViewMode::SinglePage => ViewMode::Continuous,
            ViewMode::Continuous => ViewMode::DualPage,
            ViewMode::DualPage => ViewMode::SinglePage,
        };
    }

    pub fn rotate_cw(&mut self) {
        self.rotation = (self.rotation + 90) % 360;
    }

    pub fn rotate_ccw(&mut self) {
        self.rotation = (self.rotation + 270) % 360;
    }

    /// Get the effective page size after rotation
    pub fn rotated_page_size(&self, pw: f32, ph: f32) -> (f32, f32) {
        if self.rotation == 90 || self.rotation == 270 {
            (ph, pw)
        } else {
            (pw, ph)
        }
    }

    /// Check if fit zoom needs recalculating
    pub fn needs_fit_recalc(&mut self, available_width: f32) -> bool {
        let width_changed = (available_width - self.last_available_width).abs() > 1.0;
        if width_changed {
            self.last_available_width = available_width;
        }
        if self.fit_recalc_needed || width_changed {
            self.fit_recalc_needed = false;
            true
        } else {
            false
        }
    }

    pub fn toggle_cursor_mode(&mut self) {
        self.cursor_mode = match self.cursor_mode {
            CursorMode::Hand => CursorMode::Select,
            CursorMode::Select => CursorMode::Hand,
        };
    }
}

pub struct SearchState {
    pub query: String,
    pub visible: bool,
    pub results: Vec<crate::backend::SearchHit>,
    pub current_hit_idx: usize,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            query: String::new(),
            visible: false,
            results: Vec::new(),
            current_hit_idx: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SidebarTab {
    Outline,
    Bookmarks,
}

impl Default for SidebarTab {
    fn default() -> Self { Self::Outline }
}

/// Character-level text selection
pub struct TextSelection {
    pub page_no: usize,
    pub start_page_pos: (f32, f32),
    pub end_page_pos: (f32, f32),
    pub selected_chars: Vec<PageChar>, // matched characters
    pub text: String,
    pub active: bool,
}

impl Default for TextSelection {
    fn default() -> Self {
        Self {
            page_no: 0,
            start_page_pos: (0.0, 0.0),
            end_page_pos: (0.0, 0.0),
            selected_chars: Vec::new(),
            text: String::new(),
            active: false,
        }
    }
}

/// Inertial scroll physics (Sioyek-style)
pub struct InertialScroll {
    pub velocity: egui::Vec2,
    pub active: bool,
}

const SCROLL_DRAG: f32 = 0.92; // deceleration per frame
const SCROLL_MIN_VELOCITY: f32 = 0.5;

impl Default for InertialScroll {
    fn default() -> Self {
        Self { velocity: egui::Vec2::ZERO, active: false }
    }
}

impl InertialScroll {
    /// Apply a scroll impulse
    pub fn impulse(&mut self, delta: egui::Vec2) {
        self.velocity = delta * 1.2; // slight amplification
        self.active = true;
    }

    /// Tick physics, returns offset delta to apply
    pub fn tick(&mut self) -> egui::Vec2 {
        if !self.active { return egui::Vec2::ZERO; }
        let delta = self.velocity;
        self.velocity *= SCROLL_DRAG;
        if self.velocity.length() < SCROLL_MIN_VELOCITY {
            self.velocity = egui::Vec2::ZERO;
            self.active = false;
        }
        delta
    }

    pub fn stop(&mut self) {
        self.velocity = egui::Vec2::ZERO;
        self.active = false;
    }
}

/// Password dialog state
pub struct PasswordState {
    pub needed: bool,
    pub input: String,
    pub path: Option<std::path::PathBuf>,
    pub error: bool,
}

impl Default for PasswordState {
    fn default() -> Self {
        Self { needed: false, input: String::new(), path: None, error: false }
    }
}
