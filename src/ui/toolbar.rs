use crate::state::{CursorMode, FitMode, ViewMode, ViewportState};
use egui::Ui;

#[derive(Default)]
pub struct ToolbarAction {
    pub open_file: bool,
    pub toggle_sidebar: bool,
    pub toggle_search: bool,
    pub toggle_bookmark: bool,
    pub toggle_dark_mode: bool,
    pub toggle_view_mode: bool,
    pub toggle_cursor_mode: bool,
    pub print: bool,
    pub go_to_page: Option<usize>,
    pub zoom_changed: bool,
    pub fit_changed: bool,
    pub rotate_cw: bool,
    pub rotate_ccw: bool,
    pub toggle_fullscreen: bool,
    pub goto_dialog: bool,
    pub zoom_preset: Option<ZoomPreset>,
    pub toggle_properties: bool,
    pub close_document: bool,
    pub register_default: bool,
    pub unregister_default: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ZoomPreset {
    Percent(u32),
    FitWidth,
    FitPage,
}

/// Draw menu bar (top row) + toolbar (second row)
pub fn draw_toolbar(
    ui: &mut Ui,
    viewport: &mut ViewportState,
    page_count: usize,
    is_bookmarked: bool,
    dark_mode: bool,
    page_input: &mut String,
) -> ToolbarAction {
    let mut action = ToolbarAction::default();

    // ═══════════ Row 1: Menu Bar ═══════════
    egui::menu::bar(ui, |ui| {
        ui.menu_button("File", |ui| {
            if ui.button("  Open                     Ctrl+O").clicked() { action.open_file = true; ui.close(); }
            ui.separator();
            if ui.button("  Close                    Ctrl+W").clicked() { action.close_document = true; ui.close(); }
            ui.separator();
            if ui.button("  Print                    Ctrl+P").clicked() { action.print = true; ui.close(); }
            ui.separator();
            if ui.button("  Properties               Ctrl+I").clicked() { action.toggle_properties = true; ui.close(); }
            ui.separator();
            if ui.button("  Set as Default PDF Viewer").clicked() { action.register_default = true; ui.close(); }
            if ui.button("  Remove Default Association").clicked() { action.unregister_default = true; ui.close(); }
        });

        ui.menu_button("View", |ui| {
            // Zoom submenu
            ui.menu_button("  Zoom", |ui| {
                for pct in [50, 75, 100, 125, 150, 200, 300] {
                    let selected = viewport.fit_mode == FitMode::Custom
                        && ((viewport.zoom * 100.0).round() as u32 == pct);
                    if ui.selectable_label(selected, format!("{}%", pct)).clicked() {
                        action.zoom_preset = Some(ZoomPreset::Percent(pct)); ui.close();
                    }
                }
                ui.separator();
                if ui.selectable_label(viewport.fit_mode == FitMode::FitWidth, "Fit Width").clicked() {
                    action.zoom_preset = Some(ZoomPreset::FitWidth); ui.close();
                }
                if ui.selectable_label(viewport.fit_mode == FitMode::FitPage, "Fit Page").clicked() {
                    action.zoom_preset = Some(ZoomPreset::FitPage); ui.close();
                }
            });
            ui.separator();
            if ui.selectable_label(viewport.view_mode == ViewMode::SinglePage, "  Single Page        ").clicked() {
                viewport.view_mode = ViewMode::SinglePage; ui.close();
            }
            if ui.selectable_label(viewport.view_mode == ViewMode::Continuous, "  Continuous Scroll  ").clicked() {
                viewport.view_mode = ViewMode::Continuous; ui.close();
            }
            if ui.selectable_label(viewport.view_mode == ViewMode::DualPage, "  Dual Page          ").clicked() {
                viewport.view_mode = ViewMode::DualPage; ui.close();
            }
            ui.separator();
            if ui.button("  Rotate CW            Ctrl+R").clicked() { action.rotate_cw = true; ui.close(); }
            if ui.button("  Rotate CCW      Ctrl+Shift+R").clicked() { action.rotate_ccw = true; ui.close(); }
            ui.separator();
            if ui.button("  Fullscreen               F11").clicked() { action.toggle_fullscreen = true; ui.close(); }
            let dm = if dark_mode { "  Light Mode           Ctrl+D" } else { "  Dark Mode            Ctrl+D" };
            if ui.button(dm).clicked() { action.toggle_dark_mode = true; ui.close(); }
        });

        ui.menu_button("Edit", |ui| {
            if ui.button("  Find                     Ctrl+F").clicked() { action.toggle_search = true; ui.close(); }
            if ui.button("  Go to Page               Ctrl+G").clicked() { action.goto_dialog = true; ui.close(); }
            ui.separator();
            let tool = match viewport.cursor_mode {
                CursorMode::Hand => "  Switch to Text Select    Ctrl+H",
                CursorMode::Select => "  Switch to Hand Tool      Ctrl+H",
            };
            if ui.button(tool).clicked() { action.toggle_cursor_mode = true; ui.close(); }
            ui.separator();
            let bm = if is_bookmarked { "  Remove Bookmark          Ctrl+B" } else { "  Add Bookmark             Ctrl+B" };
            if ui.button(bm).clicked() { action.toggle_bookmark = true; ui.close(); }
            ui.separator();
            if ui.button("  Sidebar                  Ctrl+L").clicked() { action.toggle_sidebar = true; ui.close(); }
        });
    });

    // ═══════════ Row 2: Toolbar ═══════════
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::Vec2::new(2.0, 0.0);

        // Page navigation
        if ui.small_button(" < ").on_hover_text("Previous page").clicked() {
            viewport.prev_page();
        }
        let resp = ui.add(
            egui::TextEdit::singleline(page_input)
                .desired_width(36.0)
                .horizontal_align(egui::Align::Center)
                .font(egui::TextStyle::Small),
        );
        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            if let Ok(p) = page_input.trim().parse::<usize>() {
                if p >= 1 && p <= page_count { action.go_to_page = Some(p - 1); }
            }
        }
        ui.label(format!("/ {}", page_count));
        if ui.small_button(" > ").on_hover_text("Next page").clicked() {
            viewport.next_page(page_count);
        }

        ui.add(egui::Separator::default().spacing(6.0));

        // Zoom controls
        if ui.small_button(" \u{2212} ").on_hover_text("Zoom out (Ctrl+-)").clicked() {
            viewport.zoom_out(); action.zoom_changed = true;
        }
        let zoom_text = match viewport.fit_mode {
            FitMode::FitWidth => "Fit W".into(),
            FitMode::FitPage => "Fit P".into(),
            FitMode::Custom => format!("{:.0}%", viewport.zoom * 100.0),
        };
        egui::ComboBox::from_id_salt("zoom")
            .selected_text(&zoom_text)
            .width(58.0)
            .show_ui(ui, |ui| {
                for pct in [50, 75, 100, 125, 150, 200, 300] {
                    if ui.selectable_label(false, format!("{}%", pct)).clicked() {
                        action.zoom_preset = Some(ZoomPreset::Percent(pct));
                    }
                }
                ui.separator();
                if ui.selectable_label(viewport.fit_mode == FitMode::FitWidth, "Fit Width").clicked() {
                    action.zoom_preset = Some(ZoomPreset::FitWidth);
                }
                if ui.selectable_label(viewport.fit_mode == FitMode::FitPage, "Fit Page").clicked() {
                    action.zoom_preset = Some(ZoomPreset::FitPage);
                }
            });
        if ui.small_button(" + ").on_hover_text("Zoom in (Ctrl++)").clicked() {
            viewport.zoom_in(); action.zoom_changed = true;
        }
        // Fit buttons - one click access
        if ui.selectable_label(viewport.fit_mode == FitMode::FitWidth, "FitW")
            .on_hover_text("Fit Width").clicked()
        {
            action.zoom_preset = Some(ZoomPreset::FitWidth);
        }
        if ui.selectable_label(viewport.fit_mode == FitMode::FitPage, "FitP")
            .on_hover_text("Fit Page").clicked()
        {
            action.zoom_preset = Some(ZoomPreset::FitPage);
        }

        ui.add(egui::Separator::default().spacing(6.0));

        // View mode toggle (highlighted selection)
        for (mode, label, tip) in [
            (ViewMode::SinglePage, " 1 ", "Single page"),
            (ViewMode::Continuous, " S ", "Continuous scroll"),
            (ViewMode::DualPage, " 2 ", "Dual page"),
        ] {
            if ui.selectable_label(viewport.view_mode == mode, label)
                .on_hover_text(tip).clicked()
            {
                viewport.view_mode = mode;
            }
        }

        ui.add(egui::Separator::default().spacing(6.0));

        // Tool mode (highlighted selection)
        if ui.selectable_label(viewport.cursor_mode == CursorMode::Hand, " Hand ")
            .on_hover_text("Drag to pan (Ctrl+H)").clicked()
        {
            viewport.cursor_mode = CursorMode::Hand;
        }
        if ui.selectable_label(viewport.cursor_mode == CursorMode::Select, " Text ")
            .on_hover_text("Select text (Ctrl+H)").clicked()
        {
            viewport.cursor_mode = CursorMode::Select;
        }

        ui.add(egui::Separator::default().spacing(6.0));

        // Quick actions
        if ui.small_button("Find").on_hover_text("Ctrl+F").clicked() {
            action.toggle_search = true;
        }
        if ui.small_button("TOC").on_hover_text("Sidebar (Ctrl+L)").clicked() {
            action.toggle_sidebar = true;
        }
        let bm = if is_bookmarked { " * " } else { " . " };
        if ui.small_button(bm).on_hover_text("Bookmark (Ctrl+B)").clicked() {
            action.toggle_bookmark = true;
        }
    });

    action
}
