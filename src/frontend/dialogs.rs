use std::path::PathBuf;

use uuid::Uuid;

use crate::backend::TemplateKind;
use crate::frontend::{pick_image, Dialog, ImportMapForm, UiAction};

pub(crate) fn show(ui: &mut egui::Ui, dialog: &mut Dialog) -> UiAction {
    let current = std::mem::take(dialog);
    let (next, action) = render(ui, current);
    *dialog = next;
    action
}

fn render(ui: &mut egui::Ui, dialog: Dialog) -> (Dialog, UiAction) {
    match dialog {
        Dialog::None => (Dialog::None, UiAction::None),
        Dialog::CreatePage {
            parent,
            template,
            title,
            linked_nation,
        } => create_page(ui, parent, template, title, linked_nation),
        Dialog::CreateFolder { parent, name } => create_folder(ui, parent, name),
        Dialog::RenamePage { id, title } => rename_page(ui, id, title),
        Dialog::RenameFolder { dir, name } => rename_folder(ui, dir, name),
        Dialog::DeletePage { id, title } => confirm_delete_page(ui, id, title),
        Dialog::DeleteFolder { dir } => confirm_delete_folder(ui, dir),
        Dialog::ImportMap(form) => import_map(ui, form),
        Dialog::DeleteMap { slug, name } => confirm_delete_map(ui, slug, name),
    }
}

fn create_page(
    ui: &mut egui::Ui,
    parent: PathBuf,
    template: TemplateKind,
    mut title: String,
    linked_nation: Option<String>,
) -> (Dialog, UiAction) {
    let mut close = false;
    let mut action = UiAction::None;
    let mut open = true;
    egui::Window::new("New page")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ui.ctx(), |ui| {
            ui.label(template.label());
            if let Some(nation) = &linked_nation {
                ui.label(format!("Linked to {nation}"));
            }
            ui.label("Title");
            let response = ui.text_edit_singleline(&mut title);
            let enter = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if (ui.button("Create").clicked() || enter) && !title.trim().is_empty() {
                action = UiAction::SubmitCreatePage {
                    parent: parent.clone(),
                    template,
                    title: title.clone(),
                    linked_nation: linked_nation.clone(),
                };
                close = true;
            }
            if ui.button("Cancel").clicked() {
                close = true;
            }
        });
    if close || !open {
        (Dialog::None, action)
    } else {
        (
            Dialog::CreatePage {
                parent,
                template,
                title,
                linked_nation,
            },
            action,
        )
    }
}

fn create_folder(ui: &mut egui::Ui, parent: PathBuf, mut name: String) -> (Dialog, UiAction) {
    let mut close = false;
    let mut action = UiAction::None;
    let mut open = true;
    egui::Window::new("New folder")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ui.ctx(), |ui| {
            ui.label("Folder name");
            let response = ui.text_edit_singleline(&mut name);
            let enter = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if (ui.button("Create").clicked() || enter) && !name.trim().is_empty() {
                action = UiAction::SubmitCreateFolder {
                    parent: parent.clone(),
                    name: name.clone(),
                };
                close = true;
            }
            if ui.button("Cancel").clicked() {
                close = true;
            }
        });
    finished(close || !open, action, Dialog::CreateFolder { parent, name })
}

fn rename_page(ui: &mut egui::Ui, id: Uuid, mut title: String) -> (Dialog, UiAction) {
    let mut close = false;
    let mut action = UiAction::None;
    let mut open = true;
    egui::Window::new("Rename page")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ui.ctx(), |ui| {
            ui.label("Title");
            let response = ui.text_edit_singleline(&mut title);
            let enter = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if (ui.button("Rename").clicked() || enter) && !title.trim().is_empty() {
                action = UiAction::SubmitRenamePage {
                    id,
                    title: title.clone(),
                };
                close = true;
            }
            if ui.button("Cancel").clicked() {
                close = true;
            }
        });
    finished(close || !open, action, Dialog::RenamePage { id, title })
}

fn rename_folder(ui: &mut egui::Ui, dir: PathBuf, mut name: String) -> (Dialog, UiAction) {
    let mut close = false;
    let mut action = UiAction::None;
    let mut open = true;
    egui::Window::new("Rename folder")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ui.ctx(), |ui| {
            ui.label("Folder name");
            let response = ui.text_edit_singleline(&mut name);
            let enter = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if (ui.button("Rename").clicked() || enter) && !name.trim().is_empty() {
                action = UiAction::SubmitRenameFolder {
                    dir: dir.clone(),
                    name: name.clone(),
                };
                close = true;
            }
            if ui.button("Cancel").clicked() {
                close = true;
            }
        });
    finished(close || !open, action, Dialog::RenameFolder { dir, name })
}

fn confirm_delete_page(ui: &mut egui::Ui, id: Uuid, title: String) -> (Dialog, UiAction) {
    let mut close = false;
    let mut action = UiAction::None;
    let mut open = true;
    egui::Window::new("Delete page")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ui.ctx(), |ui| {
            ui.label(format!("Delete \"{title}\"? This cannot be undone."));
            if ui.button("Delete").clicked() {
                action = UiAction::SubmitDeletePage(id);
                close = true;
            }
            if ui.button("Cancel").clicked() {
                close = true;
            }
        });
    finished(
        close || !open,
        action,
        Dialog::DeletePage { id, title },
    )
}

fn confirm_delete_folder(ui: &mut egui::Ui, dir: PathBuf) -> (Dialog, UiAction) {
    let mut close = false;
    let mut action = UiAction::None;
    let mut open = true;
    let name = dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("this folder");
    egui::Window::new("Delete folder")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ui.ctx(), |ui| {
            ui.label(format!(
                "Delete folder \"{name}\" and everything inside it?"
            ));
            if ui.button("Delete").clicked() {
                action = UiAction::SubmitDeleteFolder(dir.clone());
                close = true;
            }
            if ui.button("Cancel").clicked() {
                close = true;
            }
        });
    finished(close || !open, action, Dialog::DeleteFolder { dir })
}

const DIMENSIONS: [&str; 3] = ["Overworld", "Nether", "End"];

fn import_map(ui: &mut egui::Ui, mut form: ImportMapForm) -> (Dialog, UiAction) {
    let mut close = false;
    let mut action = UiAction::None;
    let mut open = true;
    egui::Window::new("Import map")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ui.ctx(), |ui| {
            ui.label("Use a PNG or JPEG exported from Xaero's World Map.");
            ui.label("Name");
            ui.text_edit_singleline(&mut form.name);
            ui.horizontal(|ui| {
                if ui.button("Choose image...").clicked() {
                    if let Some(path) = pick_image() {
                        apply_picked_image(&mut form, path);
                    }
                }
                let shown = form
                    .image
                    .as_ref()
                    .and_then(|path| path.file_name())
                    .map_or_else(|| "No image chosen".to_owned(), |name| name.to_string_lossy().into_owned());
                ui.label(shown);
            });
            egui::ComboBox::from_label("Dimension")
                .selected_text(form.dimension.clone())
                .show_ui(ui, |ui| {
                    for name in DIMENSIONS {
                        ui.selectable_value(&mut form.dimension, name.to_owned(), name);
                    }
                });
            ui.label("Pixels per block (1 if the image shows one pixel per block)");
            ui.text_edit_singleline(&mut form.scale);
            ui.label("Block coordinates of the image's top-left corner");
            ui.horizontal(|ui| {
                ui.label("X");
                ui.add(egui::TextEdit::singleline(&mut form.origin_x).desired_width(80.0));
                ui.label("Z");
                ui.add(egui::TextEdit::singleline(&mut form.origin_z).desired_width(80.0));
            });
            ui.label(egui::RichText::new("You can refine these later in the map's Calibration panel.").small().weak());
            let ready = form.image.is_some() && !form.name.trim().is_empty();
            ui.horizontal(|ui| {
                if ui.add_enabled(ready, egui::Button::new("Import")).clicked() {
                    action = UiAction::SubmitImportMap(form.clone());
                    close = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
    finished(close || !open, action, Dialog::ImportMap(form))
}

/// Remember the image and suggest a name and a starting origin that centers it on 0, 0.
fn apply_picked_image(form: &mut ImportMapForm, path: std::path::PathBuf) {
    if form.name.trim().is_empty() {
        if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
            stem.clone_into(&mut form.name);
        }
    }
    if let (Ok((width, height)), Ok(scale)) = (
        image::image_dimensions(&path),
        form.scale.trim().parse::<f64>(),
    ) {
        if scale > 0.0 {
            form.origin_x = format!("{:.0}", -f64::from(width) / scale / 2.0);
            form.origin_z = format!("{:.0}", -f64::from(height) / scale / 2.0);
        }
    }
    form.image = Some(path);
}

fn confirm_delete_map(ui: &mut egui::Ui, slug: String, name: String) -> (Dialog, UiAction) {
    let mut close = false;
    let mut action = UiAction::None;
    let mut open = true;
    egui::Window::new("Delete map")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ui.ctx(), |ui| {
            ui.label(format!(
                "Delete map \"{name}\" with its image and all claims? This cannot be undone."
            ));
            if ui.button("Delete").clicked() {
                action = UiAction::SubmitDeleteMap(slug.clone());
                close = true;
            }
            if ui.button("Cancel").clicked() {
                close = true;
            }
        });
    finished(close || !open, action, Dialog::DeleteMap { slug, name })
}

fn finished(close: bool, action: UiAction, dialog: Dialog) -> (Dialog, UiAction) {
    if close {
        (Dialog::None, action)
    } else {
        (dialog, action)
    }
}
