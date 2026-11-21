use std::path::Path;

use egui_commonmark::CommonMarkCache;

use crate::backend::{PageData, PageKind, Project, TemplateKind};
use crate::frontend::fields;
use crate::frontend::live_markdown::{self, LiveMarkdownState};
use crate::frontend::preview;
use crate::frontend::session::{EditMode, NationTab, OpenPage};
use crate::frontend::{Dialog, UiAction};

pub(crate) fn show(
    ui: &mut egui::Ui,
    project: &Project,
    open: &mut OpenPage,
    mode: &mut EditMode,
    nation_tab: &mut NationTab,
    cache: &mut CommonMarkCache,
    live_markdown: &mut LiveMarkdownState,
) -> UiAction {
    let mut action = UiAction::None;
    egui::ScrollArea::vertical()
        .id_salt("page_scroll")
        .show(ui, |ui| {
            action = page_contents(ui, project, open, mode, nation_tab, cache, live_markdown);
        });
    action
}

fn page_contents(
    ui: &mut egui::Ui,
    project: &Project,
    open: &mut OpenPage,
    mode: &mut EditMode,
    nation_tab: &mut NationTab,
    cache: &mut CommonMarkCache,
    live_markdown: &mut LiveMarkdownState,
) -> UiAction {
    ui.label(open.frontmatter.kind().label());
    fields::plain_text(ui, "Title", &mut open.frontmatter.title);
    let mut action = header(ui, project, open);
    ui.separator();
    if open.frontmatter.kind() == PageKind::Nation {
        action = merge(
            action,
            nation_body(ui, project, open, mode, nation_tab, cache, live_markdown),
        );
    } else {
        action = merge(action, notes(ui, project, open, mode, cache, live_markdown));
    }
    action
}

fn header(ui: &mut egui::Ui, project: &Project, open: &mut OpenPage) -> UiAction {
    match &mut open.frontmatter.data {
        PageData::Note => UiAction::None,
        PageData::Nation(meta) => {
            let leaders = fields::string_list(ui, "leaders", "Leaders", &mut meta.leaders, project);
            fields::plain_text(ui, "Location", &mut meta.location);
            fields::optional_day(ui, "Founded day", &mut meta.founded);
            leaders
        }
        PageData::Player(meta) => {
            fields::optional_text(ui, "Nation", &mut meta.nation);
            if let Some(nation) = meta.nation.clone() {
                if let Some(page) = project.resolve_title(&nation) {
                    if ui.button("Open nation").clicked() {
                        return UiAction::OpenPage(page);
                    }
                }
            }
            fields::optional_text(ui, "Discord", &mut meta.discord);
            fields::optional_day(ui, "Death day", &mut meta.death);
            UiAction::None
        }
        PageData::Event(meta) => {
            fields::optional_day(ui, "Day", &mut meta.day);
            fields::event_type(ui, &mut meta.event_type);
            fields::plain_text(ui, "Location", &mut meta.location);
            let nations = fields::string_list(ui, "nations", "Nations", &mut meta.nations, project);
            let participants =
                fields::string_list(ui, "participants", "Participants", &mut meta.participants, project);
            merge(nations, participants)
        }
    }
}

fn nation_body(
    ui: &mut egui::Ui,
    project: &Project,
    open: &mut OpenPage,
    mode: &mut EditMode,
    nation_tab: &mut NationTab,
    cache: &mut CommonMarkCache,
    live_markdown: &mut LiveMarkdownState,
) -> UiAction {
    ui.horizontal(|ui| {
        if ui
            .selectable_label(*nation_tab == NationTab::Notes, "Notes")
            .clicked()
        {
            *nation_tab = NationTab::Notes;
        }
        if ui
            .selectable_label(*nation_tab == NationTab::Events, "Events")
            .clicked()
        {
            *nation_tab = NationTab::Events;
        }
        if ui
            .selectable_label(*nation_tab == NationTab::Relations, "Relations")
            .clicked()
        {
            *nation_tab = NationTab::Relations;
        }
    });
    match *nation_tab {
        NationTab::Notes => notes(ui, project, open, mode, cache, live_markdown),
        NationTab::Events => nation_events(ui, project, &open.frontmatter.title),
        NationTab::Relations => nation_relations(ui, project, open),
    }
}

fn nation_events(ui: &mut egui::Ui, project: &Project, nation: &str) -> UiAction {
    let mut action = UiAction::None;
    if ui.button("New linked event").clicked() {
        action = UiAction::OpenDialog(Dialog::CreatePage {
            parent: Path::new("events").to_path_buf(),
            template: TemplateKind::Event,
            title: String::new(),
            linked_nation: Some(nation.to_owned()),
        });
    }
    for event in project.events_for_nation(nation) {
        ui.horizontal(|ui| {
            if ui.button(&event.frontmatter.title).clicked() {
                action = UiAction::OpenPage(event.frontmatter.id);
            }
            if let PageData::Event(meta) = &event.frontmatter.data {
                ui.label(meta.event_type.label());
            }
        });
    }
    action
}

fn nation_relations(ui: &mut egui::Ui, project: &Project, open: &mut OpenPage) -> UiAction {
    let Some(meta) = open.frontmatter.nation_mut() else {
        return UiAction::None;
    };
    fields::relations(ui, &mut meta.relations, project)
}

fn notes(
    ui: &mut egui::Ui,
    project: &Project,
    open: &mut OpenPage,
    mode: &mut EditMode,
    cache: &mut CommonMarkCache,
    live_markdown: &mut LiveMarkdownState,
) -> UiAction {
    mode_bar(ui, mode);
    let parent = parent_dir(project, &open.path);
    match *mode {
        EditMode::Edit => {
            text_edit(ui, &mut open.body);
            UiAction::None
        }
        EditMode::Preview => preview::show(ui, project, cache, &parent, &open.body),
        EditMode::Split => split_view(ui, project, cache, &parent, &mut open.body),
        EditMode::Live => {
            live_markdown::show(ui, project, &parent, &mut open.body, live_markdown)
        }
    }
}

fn mode_bar(ui: &mut egui::Ui, mode: &mut EditMode) {
    ui.horizontal(|ui| {
        if ui.selectable_label(*mode == EditMode::Edit, "Edit").clicked() {
            *mode = EditMode::Edit;
        }
        if ui
            .selectable_label(*mode == EditMode::Preview, "Preview")
            .clicked()
        {
            *mode = EditMode::Preview;
        }
        if ui.selectable_label(*mode == EditMode::Split, "Split").clicked() {
            *mode = EditMode::Split;
        }
        if ui
            .selectable_label(*mode == EditMode::Live, "Live")
            .on_hover_text("Live markdown rendering")
            .clicked()
        {
            *mode = EditMode::Live;
        }
    });
}

fn split_view(
    ui: &mut egui::Ui,
    project: &Project,
    cache: &mut CommonMarkCache,
    parent: &Path,
    body: &mut String,
) -> UiAction {
    let mut action = UiAction::None;
    ui.columns(2, |columns| {
        let (left, right) = columns.split_at_mut(1);
        text_edit(&mut left[0], body);
        action = preview::show(&mut right[0], project, cache, parent, body);
    });
    action
}

fn text_edit(ui: &mut egui::Ui, body: &mut String) {
    let fill = ui.visuals().panel_fill;
    ui.add(
        egui::TextEdit::multiline(body)
            .desired_width(f32::INFINITY)
            .desired_rows(18)
            .code_editor()
            .frame(
                egui::Frame::new()
                    .fill(fill)
                    .stroke(egui::Stroke::NONE)
                    .inner_margin(egui::Margin::symmetric(4, 2)),
            ),
    );
}

fn parent_dir(project: &Project, file: &Path) -> std::path::PathBuf {
    file.parent()
        .and_then(|parent| parent.strip_prefix(&project.root).ok())
        .unwrap_or(Path::new(""))
        .to_path_buf()
}

fn merge(current: UiAction, next: UiAction) -> UiAction {
    if matches!(current, UiAction::None) {
        next
    } else {
        current
    }
}
