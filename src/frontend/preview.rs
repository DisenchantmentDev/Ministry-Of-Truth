use std::path::Path;

use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use crate::backend::{render_wiki_links, Project, TemplateKind};
use crate::frontend::UiAction;

pub(crate) fn show(
    ui: &mut egui::Ui,
    project: &Project,
    cache: &mut CommonMarkCache,
    parent: &Path,
    body: &str,
) -> UiAction {
    let (markdown, links) = render_wiki_links(body, |title| project.resolve_title(title));
    cache.link_hooks_clear();
    for link in &links {
        cache.add_link_hook(&link.url);
    }
    let _shown = CommonMarkViewer::new().show(ui, cache, &markdown);
    for link in &links {
        if cache.get_link_hook(&link.url) == Some(true) {
            return match link.page {
                Some(id) => UiAction::OpenPage(id),
                None => create_note(parent, &link.title),
            };
        }
    }
    let mut action = UiAction::None;
    for link in links.iter().filter(|link| link.page.is_none()) {
        if ui.button(format!("Create \"{}\"", link.title)).clicked() {
            action = create_note(parent, &link.title);
        }
    }
    action
}

fn create_note(parent: &Path, title: &str) -> UiAction {
    UiAction::SubmitCreatePage {
        parent: parent.to_path_buf(),
        template: TemplateKind::Note,
        title: title.to_owned(),
        linked_nation: None,
    }
}
