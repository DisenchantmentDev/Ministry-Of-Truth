use crate::app::Application;
use crate::frontend::dialogs;
use crate::frontend::editor;
use crate::frontend::map_view;
use crate::frontend::floating;
use crate::frontend::session::{MainView, Session};
use crate::frontend::timeline_view;
use crate::frontend::tree;
use crate::frontend::UiAction;
use crate::backend::Project;

pub(crate) fn show(ui: &mut egui::Ui, app: &mut Application) -> Vec<UiAction> {
    let mut actions = Vec::new();
    let project = app.project.take();
    if let Some(ref project) = project {
        let open_id = app.session.open.as_ref().map(|page| page.id);
        egui::Panel::left("project_tree")
            .resizable(true)
            .default_size(240.0)
            .show(ui, |ui| {
                let active_map = app.session.map.as_ref().map(|map| map.slug.as_str());
                actions.push(tree::show(ui, project, open_id, app.main_view, active_map));
            });
        egui::Panel::bottom("status").show(ui, |ui| {
            let status = if app.session.status.is_empty() {
                " "
            } else {
                app.session.status.as_str()
            };
            ui.label(egui::RichText::new(status).small().weak());
        });
        egui::CentralPanel::default().show(ui, |ui| {
            actions.push(central(ui, project, &mut app.session, app.main_view));
        });
        actions.extend(floating::show(ui, project, &mut app.session.floating));
        actions.push(dialogs::show(ui, &mut app.session.dialog));
    }
    app.project = project;
    actions
}

fn central(
    ui: &mut egui::Ui,
    project: &Project,
    session: &mut Session,
    main_view: MainView,
) -> UiAction {
    match main_view {
        MainView::Timeline => timeline_view::show(ui, project),
        MainView::Map => map_view::show(ui, project, session),
        MainView::Page => show_page(ui, project, session),
    }
}

fn show_page(ui: &mut egui::Ui, project: &Project, session: &mut Session) -> UiAction {
    let Session {
        open,
        mode,
        nation_tab,
        cache,
        live_markdown,
        ..
    } = session;
    let Some(open) = open.as_mut() else {
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new("Select a page from the tree").weak());
        });
        return UiAction::None;
    };
    editor::show(ui, project, open, mode, nation_tab, cache, live_markdown)
}
