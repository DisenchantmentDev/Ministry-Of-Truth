use uuid::Uuid;

use crate::backend::{EventType, NationMeta, Project, Relation, Stance};
use crate::frontend::UiAction;

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
) -> UiAction {
    ui.label(label);
    let mut action = UiAction::None;
    let mut remove = None;
    for (index, value) in values.iter_mut().enumerate() {
        ui.push_id((id, index), |ui| {
            ui.horizontal(|ui| {
                ui.text_edit_singleline(value);
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
                ui.text_edit_singleline(&mut relation.target);
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

pub(crate) fn plain_text(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.text_edit_singleline(value);
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
