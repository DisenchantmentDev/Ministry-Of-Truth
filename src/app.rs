use eframe::egui::CentralPanel;
use eframe::egui::Context;
use egui::TextStyle;
use egui::{FontFamily, FontId};

use crate::backend::project;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
pub struct Application {
    state: WindowState,
    project_dir: PathBuf,
}

#[derive(Debug, Default, PartialEq, Clone, Copy, Eq, serde::Deserialize, serde::Serialize)]
pub enum WindowState {
    #[default]
    Landing,
    Project,
    //Other things maybe, idk
}

//TODO: loading functionality framework
//TODO: Define project information struct

impl Default for Application {
    fn default() -> Self {
        Self {
            state: WindowState::Landing,
            project_dir: PathBuf::new(),
        }
    }
}

impl eframe::App for Application {
    /// Called by the framework to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Put your widgets into a `SidePanel`, `TopBottomPanel`, `CentralPanel`, `Window` or `Area`.
        // For inspiration and more examples, go to https://emilk.github.io/egui

        egui::Panel::top("top_panel").show_inside(ui, |ui| {
            // The top panel is often a good place for a menu bar:

            egui::MenuBar::new().ui(ui, |ui| {
                // NOTE: no File->Quit on web pages!
                let is_web = cfg!(target_arch = "wasm32");
                if !is_web {
                    ui.menu_button("File", |ui| {
                        if ui.button("Quit").clicked() {
                            ui.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.add_space(16.0);
                }

                egui::widgets::global_theme_preference_buttons(ui);
            });
        });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            // The central panel the region left after adding TopPanel's and SidePanel's
            ui.heading("eframe template");

            ui.horizontal(|ui| {
                ui.label("Write something: ");
            });

            ui.separator();

            ui.add(egui::github_link_file!(
                "https://github.com/emilk/eframe_template/blob/main/",
                "Source code."
            ));

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                powered_by_egui_and_eframe(ui);
                egui::warn_if_debug_build(ui);
            });
        });
    }
}

impl Application {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        } else {
            Default::default()
        }
    }
}

fn set_style(ctx: &Context) {
    let mut style = (*ctx.global_style()).clone();
    style.text_styles = [
        (
            TextStyle::Heading,
            FontId::new(30.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(18.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(14.0, FontFamily::Monospace)),
        (TextStyle::Small, FontId::new(14.0, FontFamily::Monospace)),
    ]
    .into();
    ctx.set_global_style(style);
}

fn powered_by_egui_and_eframe(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label("Powered by ");
        ui.hyperlink_to("egui", "https://github.com/emilk/egui");
        ui.label(" and ");
        ui.hyperlink_to(
            "eframe",
            "https://github.com/emilk/egui/tree/master/crates/eframe",
        );
        ui.label(".");
    });
}
