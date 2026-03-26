use crate::state::SearchState;
use egui::Ui;

pub enum SearchAction {
    None,
    Search(String),
    NextResult,
    PrevResult,
    Close,
}

pub fn draw_search_bar(ui: &mut Ui, state: &mut SearchState) -> SearchAction {
    let mut action = SearchAction::None;

    ui.horizontal(|ui| {
        ui.label("Find:");

        let _response = ui.add(
            egui::TextEdit::singleline(&mut state.query)
                .desired_width(250.0)
                .hint_text("Type and press Enter..."),
        );

        // Trigger search on Enter key while text field has focus
        let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));
        if enter_pressed && !state.query.is_empty() {
            action = SearchAction::Search(state.query.clone());
        }

        // Also add a Go button for click-based search
        if ui.button("Go").clicked() && !state.query.is_empty() {
            action = SearchAction::Search(state.query.clone());
        }

        // Result count (hit groups, not individual quads)
        if !state.results.is_empty() {
            ui.label(format!("{} / {}", state.current_hit_idx + 1, state.results.len()));
        }

        if ui.button("Prev").on_hover_text("Shift+F3").clicked() {
            action = SearchAction::PrevResult;
        }
        if ui.button("Next").on_hover_text("F3").clicked() {
            action = SearchAction::NextResult;
        }
        if ui.button("X").on_hover_text("Esc").clicked() {
            action = SearchAction::Close;
        }
    });

    action
}
