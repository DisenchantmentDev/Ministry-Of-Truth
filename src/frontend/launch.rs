use std::path::Path;

use crate::backend::parse_start_date;
use crate::frontend::files::pick_folder;
use crate::frontend::session::Session;
use crate::frontend::UiAction;

pub(crate) fn show(ui: &mut egui::Ui, session: &mut Session, last_project: Option<&Path>) -> UiAction {
    let mut action = UiAction::None;
    egui::CentralPanel::default().show(ui, |ui| {
        ui.heading("Ministry of Truth");
        ui.label("Create or open a project folder of markdown notes.");
        #[cfg(target_arch = "wasm32")]
        ui.label("Choosing a folder requires the native application.");

        if let Some(path) = last_project {
            if path.join("project.json").is_file() && ui.button("Reopen last project").clicked() {
                action = UiAction::OpenProject(path.to_path_buf());
            }
        }

        ui.separator();
        ui.label("Event name");
        ui.text_edit_singleline(&mut session.new_name);
        ui.label("Day 0 date (YYYY-MM-DD, optional)");
        ui.text_edit_singleline(&mut session.new_date);

        if ui.button("Create project").clicked() {
            action = create_action(session);
        }
        if ui.button("Open project").clicked() {
            action = open_action(session);
        }
        if !session.status.is_empty() {
            ui.separator();
            ui.label(&session.status);
        }
    });
    action
}

fn create_action(session: &mut Session) -> UiAction {
    if session.new_name.trim().is_empty() {
        session.status = "Enter an event name.".to_owned();
        return UiAction::None;
    }
    if let Err(err) = parse_start_date(&session.new_date) {
        session.status = err.to_string();
        return UiAction::None;
    }
    let Some(folder) = pick_folder() else {
        session.status = "No folder selected.".to_owned();
        return UiAction::None;
    };
    UiAction::CreateProject {
        folder,
        name: session.new_name.clone(),
        date: session.new_date.clone(),
    }
}

fn open_action(session: &mut Session) -> UiAction {
    let Some(folder) = pick_folder() else {
        session.status = "No folder selected.".to_owned();
        return UiAction::None;
    };
    UiAction::OpenProject(folder)
}
