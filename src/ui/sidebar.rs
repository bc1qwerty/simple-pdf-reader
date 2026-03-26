use crate::backend::OutlineEntry;
use crate::state::SidebarTab;
use crate::state::settings::Bookmark;
use egui::Ui;

/// Draw the sidebar with TOC and bookmarks
pub fn draw_sidebar(
    ui: &mut Ui,
    tab: &mut SidebarTab,
    outline: &[OutlineEntry],
    bookmarks: &[&Bookmark],
) -> Option<usize> {
    let mut go_to_page = None;

    ui.horizontal(|ui| {
        ui.selectable_value(tab, SidebarTab::Outline, "Outline");
        ui.selectable_value(tab, SidebarTab::Bookmarks, "Bookmarks");
    });

    ui.separator();

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            match tab {
                SidebarTab::Outline => {
                    if outline.is_empty() {
                        ui.label("No outline available");
                    } else {
                        go_to_page = draw_outline_tree(ui, outline, 0);
                    }
                }
                SidebarTab::Bookmarks => {
                    if bookmarks.is_empty() {
                        ui.label("No bookmarks yet.\nPress Ctrl+B to bookmark.");
                    } else {
                        for bm in bookmarks {
                            if ui
                                .button(format!("* {} (p.{})", bm.label, bm.page_no + 1))
                                .clicked()
                            {
                                go_to_page = Some(bm.page_no);
                            }
                        }
                    }
                }
            }
        });

    go_to_page
}

const MAX_OUTLINE_DEPTH: usize = 20;

fn draw_outline_tree(ui: &mut Ui, entries: &[OutlineEntry], depth: usize) -> Option<usize> {
    if depth > MAX_OUTLINE_DEPTH {
        return None;
    }

    let mut go_to = None;

    for entry in entries {
        let indent = depth as f32 * 16.0;
        ui.horizontal(|ui| {
            ui.add_space(indent);
            let label = if entry.children.is_empty() {
                format!("  {}", entry.title)
            } else {
                format!("> {}", entry.title)
            };

            if ui.button(&label).clicked() {
                if let Some(page) = entry.page_no {
                    go_to = Some(page);
                }
            }
        });

        if !entry.children.is_empty() {
            if let Some(p) = draw_outline_tree(ui, &entry.children, depth + 1) {
                go_to = Some(p);
            }
        }
    }

    go_to
}
