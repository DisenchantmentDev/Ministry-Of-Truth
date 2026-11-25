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
    let source = hard_line_breaks(body);
    let (markdown, links) = render_wiki_links(&source, |title| project.resolve_title(title));
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

/// Make every single Enter a visible line break.
///
/// CommonMark renders a lone newline as a space. A line that is followed by
/// another non-blank line gets a trailing hard break so it shows as written.
/// Blank lines still separate paragraphs, and fenced code is left untouched.
pub(crate) fn hard_line_breaks(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out = String::with_capacity(text.len() + lines.len() * 2);
    let mut in_fence = false;
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let is_fence = trimmed.starts_with("```") || trimmed.starts_with("~~~");
        if is_fence {
            in_fence = !in_fence;
        }
        out.push_str(line);
        let Some(next) = lines.get(index + 1) else {
            break;
        };
        let wants_break = !in_fence
            && !is_fence
            && !line.trim().is_empty()
            && !next.trim().is_empty()
            && !line.ends_with("  ")
            && !line.ends_with('\\');
        if wants_break {
            out.push_str("  ");
        }
        out.push('\n');
    }
    out
}

fn create_note(parent: &Path, title: &str) -> UiAction {
    UiAction::SubmitCreatePage {
        parent: parent.to_path_buf(),
        template: TemplateKind::Note,
        title: title.to_owned(),
        linked_nation: None,
    }
}

#[cfg(test)]
mod tests {
    use super::hard_line_breaks;

    #[test]
    fn single_enter_becomes_a_hard_break() {
        assert_eq!(hard_line_breaks("one\ntwo"), "one  \ntwo");
    }

    #[test]
    fn blank_lines_and_code_fences_are_left_alone() {
        assert_eq!(hard_line_breaks("one\n\ntwo\n"), "one\n\ntwo\n");
        let fenced = "```\na\nb\n```\nafter\nmore";
        assert_eq!(hard_line_breaks(fenced), "```\na\nb\n```\nafter  \nmore");
    }

    #[test]
    fn existing_hard_breaks_are_not_doubled() {
        assert_eq!(hard_line_breaks("one  \ntwo"), "one  \ntwo");
        assert_eq!(hard_line_breaks("one\\\ntwo"), "one\\\ntwo");
    }
}
