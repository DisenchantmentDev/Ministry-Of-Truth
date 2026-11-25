use std::path::Path;

use crate::backend::parse_start_date;
use crate::frontend::files::pick_folder;
use crate::frontend::session::Session;
use crate::frontend::UiAction;

const CARD_WIDTH: f32 = 380.0;
const FIELD_HEIGHT: f32 = 32.0;

pub(crate) fn show(ui: &mut egui::Ui, session: &mut Session, last_project: Option<&Path>) -> UiAction {
    let mut action = UiAction::None;
    egui::CentralPanel::default().show(ui, |ui| {
        let center = ui.max_rect().center();
        let ctx = ui.ctx().clone();
        egui::Area::new(egui::Id::new("landing_card"))
            .fixed_pos(center)
            .pivot(egui::Align2::CENTER_CENTER)
            .show(&ctx, |ui| {
                egui::Frame::group(ui.style())
                    .fill(ui.visuals().window_fill)
                    .corner_radius(10_u8)
                    .inner_margin(egui::Margin::same(28))
                    .show(ui, |ui| {
                        ui.set_width(CARD_WIDTH);
                        action = card(ui, session, last_project);
                    });
            });
    });
    action
}

fn card(ui: &mut egui::Ui, session: &mut Session, last_project: Option<&Path>) -> UiAction {
    let mut action = UiAction::None;

    ui.vertical_centered(|ui| {
        ui.label(egui::RichText::new("Ministry of Truth").size(34.0).strong());
        ui.label(egui::RichText::new("Lore notes for roleplay events").weak());
    });
    #[cfg(target_arch = "wasm32")]
    ui.label("Choosing a folder requires the native application.");
    ui.add_space(20.0);

    ui.label(egui::RichText::new("New project").strong());
    ui.add(
        egui::TextEdit::singleline(&mut session.new_name)
            .hint_text("Event name")
            .desired_width(f32::INFINITY)
            .margin(egui::Margin::symmetric(8, 6)),
    );
    ui.add(
        egui::TextEdit::singleline(&mut session.new_date)
            .hint_text("Day 0 date, YYYY-MM-DD (optional)")
            .desired_width(f32::INFINITY)
            .margin(egui::Margin::symmetric(8, 6)),
    );
    ui.add_space(4.0);
    if wide_button(ui, "Create project").clicked() {
        action = create_action(session);
    }

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(12.0);

    if let Some(path) = last_project.filter(|path| path.join("project.json").is_file()) {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("last project");
        if wide_button(ui, &format!("Reopen {name}")).clicked() {
            action = UiAction::OpenProject(path.to_path_buf());
        }
    }
    if wide_button(ui, "Open project folder").clicked() {
        action = open_action(session);
    }

    if !session.status.is_empty() {
        ui.add_space(12.0);
        ui.vertical_centered(|ui| {
            ui.colored_label(ui.visuals().warn_fg_color, &session.status);
        });
    }
    action
}

fn wide_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add_sized(
        [ui.available_width(), FIELD_HEIGHT],
        egui::Button::new(text),
    )
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
