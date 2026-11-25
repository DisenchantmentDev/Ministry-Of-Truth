use egui::text::{CCursor, CCursorRange};
use egui::{EventFilter, Key, Modifiers};
use uuid::Uuid;

use crate::backend::{
    suggest_link_titles, EventType, NationMeta, PageKind, Project, Relation, Stance,
};
use crate::frontend::UiAction;

/// Most rows a link field's suggestion popup shows.
const MAX_SUGGESTIONS: usize = 8;

/// Per-field popup state, kept in egui's temporary memory.
#[derive(Debug, Clone, Default)]
struct LinkEditState {
    /// Highlighted row in the popup.
    selected: usize,
    /// Text the highlight belongs to; the highlight resets when it changes.
    query: String,
    /// Text whose popup the user dismissed with Escape.
    dismissed: Option<String>,
}

/// Page titles of the given `kinds` worth offering for `text`.
/// Empty once the text already names such a page.
fn link_titles(project: &Project, kinds: &[PageKind], text: &str) -> Vec<String> {
    let titles: Vec<String> = suggest_link_titles(
        project
            .pages()
            .iter()
            .filter(|page| kinds.contains(&page.frontmatter.kind()))
            .map(|page| page.frontmatter.title.as_str())
            .filter(|title| !title.trim().is_empty()),
        text,
        MAX_SUGGESTIONS,
    )
    .into_iter()
    .map(str::to_owned)
    .collect();
    if titles.len() == 1 && titles[0].eq_ignore_ascii_case(text.trim()) {
        return Vec::new();
    }
    titles
}

/// Single-line field for a note title that suggests matching pages as you type.
///
/// Only pages of the given `kinds` are suggested. Arrow keys move the
/// highlight, Enter or Tab accepts it, and Escape hides the popup. Rows can
/// also be picked with the mouse.
pub(crate) fn link_edit(
    ui: &mut egui::Ui,
    project: &Project,
    kinds: &[PageKind],
    value: &mut String,
) -> egui::Response {
    let id = ui.next_auto_id();
    let mut state: LinkEditState = ui.data(|data| data.get_temp(id)).unwrap_or_default();
    if state.dismissed.as_deref() != Some(value.as_str()) {
        state.dismissed = None;
    }
    let mut changed = false;

    // Keys that drive the popup are consumed before the text edit sees them.
    let had_focus = ui.memory(|memory| memory.has_focus(id));
    if had_focus && state.dismissed.is_none() {
        let titles = link_titles(project, kinds, value);
        if !titles.is_empty() {
            let none = Modifiers::NONE;
            let (up, down, accept, escape) = ui.input_mut(|input| {
                let up = input.consume_key(none, Key::ArrowUp);
                let down = input.consume_key(none, Key::ArrowDown);
                let enter = input.consume_key(none, Key::Enter);
                let tab = input.consume_key(none, Key::Tab);
                (up, down, enter || tab, input.consume_key(none, Key::Escape))
            });
            let count = titles.len();
            let selected = state.selected.min(count - 1);
            if up {
                state.selected = (selected + count - 1) % count;
            } else if down {
                state.selected = (selected + 1) % count;
            }
            if accept {
                accept_title(ui, id, value, &titles[selected]);
                changed = true;
                state.query.clear();
                state.selected = 0;
            } else if escape {
                state.dismissed = Some(value.clone());
            }
        }
    }

    let mut response = ui.add(egui::TextEdit::singleline(value).id(id));

    // `had_focus` keeps the popup alive on the frame a press on a row steals focus.
    if (had_focus || response.has_focus()) && state.dismissed.is_none() {
        let titles = link_titles(project, kinds, value);
        if titles.is_empty() {
            state.query.clear();
        } else {
            if state.query != *value {
                state.query = value.clone();
                state.selected = 0;
            }
            state.selected = state.selected.min(titles.len() - 1);
            if response.has_focus() {
                ui.memory_mut(|memory| {
                    memory.set_focus_lock_filter(
                        id,
                        EventFilter {
                            horizontal_arrows: true,
                            vertical_arrows: true,
                            // Tab and Escape belong to the popup while it is open.
                            tab: true,
                            escape: true,
                        },
                    );
                });
            }
            let anchor = response.rect.left_bottom() + egui::vec2(0.0, 2.0);
            let width = response.rect.width();
            if let Some(index) = suggestion_popup(ui, id, &titles, state.selected, anchor, width) {
                accept_title(ui, id, value, &titles[index]);
                changed = true;
                response.request_focus();
                state.query.clear();
                state.selected = 0;
            }
        }
    }
    if changed {
        response.mark_changed();
    }

    ui.data_mut(|data| data.insert_temp(id, state));
    response
}

/// Replace the field text with `title` and put the caret at its end.
fn accept_title(ui: &egui::Ui, id: egui::Id, value: &mut String, title: &str) {
    *value = title.to_owned();
    let mut edit_state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
    edit_state
        .cursor
        .set_char_range(Some(CCursorRange::one(CCursor::new(title.chars().count()))));
    edit_state.store(ui.ctx(), id);
}

/// Floating list under the field. Returns the row pressed with the mouse.
///
/// Rows react on press, not release: the press takes focus from the field,
/// and by release the popup would already be gone.
fn suggestion_popup(
    ui: &egui::Ui,
    id: egui::Id,
    titles: &[String],
    selected: usize,
    anchor: egui::Pos2,
    min_width: f32,
) -> Option<usize> {
    let mut picked = None;
    egui::Area::new(id.with("link_suggestions"))
        .order(egui::Order::Foreground)
        .fixed_pos(anchor)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(min_width.max(180.0));
                for (index, title) in titles.iter().enumerate() {
                    let row = ui.selectable_label(index == selected, title);
                    if row.hovered() && ui.input(|input| input.pointer.primary_pressed()) {
                        picked = Some(index);
                    }
                }
            });
        });
    picked
}

pub(crate) fn optional_text(ui: &mut egui::Ui, label: &str, value: &mut Option<String>) {
    let mut text = value.clone().unwrap_or_default();
    let changed = ui
        .horizontal(|ui| {
            ui.label(label);
            ui.text_edit_singleline(&mut text).changed()
        })
        .inner;
    if changed {
        *value = if text.is_empty() { None } else { Some(text) };
    }
}

/// Like `optional_text`, but the value names a page and is autocompleted.
pub(crate) fn optional_link(
    ui: &mut egui::Ui,
    project: &Project,
    kinds: &[PageKind],
    label: &str,
    value: &mut Option<String>,
) {
    let mut text = value.clone().unwrap_or_default();
    let changed = ui
        .horizontal(|ui| {
            ui.label(label);
            link_edit(ui, project, kinds, &mut text).changed()
        })
        .inner;
    if changed {
        *value = if text.is_empty() { None } else { Some(text) };
    }
}

pub(crate) fn optional_day(ui: &mut egui::Ui, label: &str, value: &mut Option<i32>) {
    let mut enabled = value.is_some();
    ui.horizontal(|ui| {
        if ui.checkbox(&mut enabled, label).changed() {
            if enabled {
                if value.is_none() {
                    *value = Some(0);
                }
            } else {
                *value = None;
            }
        }
        if let Some(day) = value.as_mut() {
            ui.add(egui::DragValue::new(day));
        }
    });
}

pub(crate) fn event_type(ui: &mut egui::Ui, value: &mut EventType) {
    egui::ComboBox::from_id_salt("event_type")
        .selected_text(value.label())
        .show_ui(ui, |ui| {
            for kind in EventType::ALL {
                ui.selectable_value(value, kind, kind.label());
            }
        });
}

pub(crate) fn string_list(
    ui: &mut egui::Ui,
    id: &str,
    label: &str,
    values: &mut Vec<String>,
    project: &Project,
    kinds: &[PageKind],
) -> UiAction {
    ui.label(label);
    let mut action = UiAction::None;
    let mut remove = None;
    for (index, value) in values.iter_mut().enumerate() {
        ui.push_id((id, index), |ui| {
            ui.horizontal(|ui| {
                link_edit(ui, project, kinds, value);
                if let Some(page) = project.resolve_title(value) {
                    if ui.button("Open").clicked() {
                        action = UiAction::OpenPage(page);
                    }
                }
                if ui.button("Remove").clicked() {
                    remove = Some(index);
                }
            });
        });
    }
    if let Some(index) = remove {
        values.remove(index);
    }
    if ui.button(format!("Add {label}")).clicked() {
        values.push(String::new());
    }
    action
}

pub(crate) fn relations(ui: &mut egui::Ui, values: &mut Vec<Relation>, project: &Project) -> UiAction {
    ui.label("Relations");
    let mut action = UiAction::None;
    let mut remove = None;
    for (index, relation) in values.iter_mut().enumerate() {
        ui.push_id(index, |ui| {
            ui.horizontal(|ui| {
                link_edit(ui, project, &[PageKind::Nation], &mut relation.target);
                egui::ComboBox::from_id_salt("stance")
                    .selected_text(relation.stance.label())
                    .show_ui(ui, |ui| {
                        for stance in Stance::ALL {
                            ui.selectable_value(&mut relation.stance, stance, stance.label());
                        }
                    });
                if let Some(page) = project.resolve_title(&relation.target) {
                    if ui.button("Open").clicked() {
                        action = UiAction::OpenPage(page);
                    }
                }
                if ui.button("Remove").clicked() {
                    remove = Some(index);
                }
            });
        });
    }
    if let Some(index) = remove {
        values.remove(index);
    }
    if ui.button("Add relation").clicked() {
        values.push(Relation {
            target: String::new(),
            stance: Stance::Neutral,
        });
    }
    action
}

/// A labeled text field that suggests pages of the given `kinds`.
pub(crate) fn link_text(
    ui: &mut egui::Ui,
    project: &Project,
    kinds: &[PageKind],
    label: &str,
    value: &mut String,
) {
    ui.horizontal(|ui| {
        ui.label(label);
        link_edit(ui, project, kinds, value);
    });
}

/// Color picker for a nation's map tint. Unset colors show the derived default.
pub(crate) fn nation_color(ui: &mut egui::Ui, id: Uuid, meta: &mut NationMeta) {
    ui.horizontal(|ui| {
        ui.label("Map color");
        let mut rgb = meta.color_for(id);
        if ui.color_edit_button_srgb(&mut rgb).changed() {
            meta.color = Some(rgb);
        }
        if meta.color.is_some() {
            if ui.button("Reset").on_hover_text("Use the automatic color").clicked() {
                meta.color = None;
            }
        } else {
            ui.label(egui::RichText::new("automatic").small().weak());
        }
    });
}
