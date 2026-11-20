use std::path::PathBuf;

use uuid::Uuid;

use crate::backend::TemplateKind;
use crate::frontend::{Dialog, UiAction};

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

fn finished(close: bool, action: UiAction, dialog: Dialog) -> (Dialog, UiAction) {
    if close {
        (Dialog::None, action)
    } else {
        (dialog, action)
    }
}
