use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::backend::{PageSummary, Project, TemplateKind};
use crate::frontend::session::MainView;
use crate::frontend::{Dialog, UiAction};

#[derive(Debug, Clone)]
struct TreeNode {
    name: String,
    is_dir: bool,
    page_id: Option<Uuid>,
    create_in: PathBuf,
    folder: PathBuf,
    children: Vec<TreeNode>,
}

pub(crate) fn show(
    ui: &mut egui::Ui,
    project: &Project,
    open_id: Option<Uuid>,
    main_view: MainView,
) -> UiAction {
    let mut action = UiAction::None;
    if ui
        .selectable_label(main_view == MainView::Timeline, "Timeline")
        .clicked()
    {
        action = UiAction::ShowTimeline;
    }
    ui.separator();
    for warning in &project.warnings {
        ui.colored_label(ui.visuals().warn_fg_color, warning);
    }
    let tree = build_tree(project);
    merge(
        action,
        show_node(ui, &tree, open_id, main_view == MainView::Page, 0),
    )
}

/// Root event name is largest; top-level folders are mid-size; deeper rows share one floor size.
fn label_size(depth: u8) -> f32 {
    match depth {
        0 => 22.0,
        1 => 16.0,
        _ => 14.0,
    }
}

fn show_node(
    ui: &mut egui::Ui,
    node: &TreeNode,
    open_id: Option<Uuid>,
    page_view: bool,
    depth: u8,
) -> UiAction {
    if node.is_dir {
        show_dir(ui, node, open_id, page_view, depth)
    } else {
        show_file(ui, node, open_id, page_view, depth)
    }
}

fn show_dir(
    ui: &mut egui::Ui,
    node: &TreeNode,
    open_id: Option<Uuid>,
    page_view: bool,
    depth: u8,
) -> UiAction {
    let mut action = UiAction::None;
    let id = ui.id().with(&node.folder);
    let selected = page_view && node.page_id.is_some() && node.page_id == open_id;
    let mut header_action = UiAction::None;
    let label = egui::RichText::new(&node.name).size(label_size(depth));
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
        .show_header(ui, |ui| {
            let response = ui.selectable_label(selected, label);
            header_action = menu_action(&response, node);
            if header_action.is_none() && response.clicked() {
                header_action = open_node(node);
            }
        })
        .body(|ui| {
            let child_depth = depth.saturating_add(1);
            for child in &node.children {
                let next = show_node(ui, child, open_id, page_view, child_depth);
                action = merge(std::mem::replace(&mut action, UiAction::None), next);
            }
        });
    merge(header_action, action)
}

fn show_file(
    ui: &mut egui::Ui,
    node: &TreeNode,
    open_id: Option<Uuid>,
    page_view: bool,
    depth: u8,
) -> UiAction {
    let selected = page_view && node.page_id == open_id;
    let label = egui::RichText::new(&node.name).size(label_size(depth));
    let response = ui.selectable_label(selected, label);
    let menu = menu_action(&response, node);
    if !menu.is_none() {
        menu
    } else if response.clicked() {
        open_node(node)
    } else {
        UiAction::None
    }
}

fn open_node(node: &TreeNode) -> UiAction {
    if let Some(id) = node.page_id {
        UiAction::OpenPage(id)
    } else if node.is_dir {
        UiAction::OpenFolder(node.folder.clone())
    } else {
        UiAction::None
    }
}

fn menu_action(response: &egui::Response, node: &TreeNode) -> UiAction {
    let mut action = UiAction::None;
    response.context_menu(|ui| {
        action = node_menu(ui, node);
    });
    action
}

fn node_menu(ui: &mut egui::Ui, node: &TreeNode) -> UiAction {
    let mut action = UiAction::None;
    if let Some(id) = node.page_id {
        if ui.button("Open in window").clicked() {
            action = UiAction::OpenFloating(id);
        }
    }
    if ui.button("New note").clicked() {
        action = create_page_dialog(node, TemplateKind::Note);
    }
    if ui.button("New folder").clicked() {
        action = UiAction::OpenDialog(Dialog::CreateFolder {
            parent: node.create_in.clone(),
            name: String::new(),
        });
    }
    ui.menu_button("New from template", |ui| {
        for template in TemplateKind::MENU {
            if ui.button(template.label()).clicked() {
                action = create_page_dialog(node, template);
            }
        }
    });
    if node.is_dir && !node.folder.as_os_str().is_empty() {
        if ui.button("Rename").clicked() {
            action = UiAction::OpenDialog(Dialog::RenameFolder {
                dir: node.folder.clone(),
                name: node.name.clone(),
            });
        }
        if ui.button("Delete").clicked() {
            action = UiAction::OpenDialog(Dialog::DeleteFolder {
                dir: node.folder.clone(),
            });
        }
    }
    if !node.is_dir {
        if let Some(id) = node.page_id {
            if ui.button("Rename").clicked() {
                action = UiAction::OpenDialog(Dialog::RenamePage {
                    id,
                    title: node.name.clone(),
                });
            }
            if ui.button("Delete").clicked() {
                action = UiAction::OpenDialog(Dialog::DeletePage {
                    id,
                    title: node.name.clone(),
                });
            }
        }
    }
    action
}

fn create_page_dialog(node: &TreeNode, template: TemplateKind) -> UiAction {
    UiAction::OpenDialog(Dialog::CreatePage {
        parent: node.create_in.clone(),
        template,
        title: String::new(),
        linked_nation: None,
    })
}

fn merge(current: UiAction, next: UiAction) -> UiAction {
    if current.is_none() { next } else { current }
}

trait ActionNone {
    fn is_none(&self) -> bool;
}

impl ActionNone for UiAction {
    fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

fn build_tree(project: &Project) -> TreeNode {
    let mut root = TreeNode {
        name: project.meta.name.clone(),
        is_dir: true,
        page_id: None,
        create_in: PathBuf::new(),
        folder: PathBuf::new(),
        children: Vec::new(),
    };
    for page in project.pages() {
        insert(&mut root, &project.relative(&page.path), page);
    }
    sort_tree(&mut root);
    root
}

fn insert(node: &mut TreeNode, relative: &Path, page: &PageSummary) {
    let comps: Vec<String> = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    insert_comps(node, &comps, page);
}

fn insert_comps(node: &mut TreeNode, comps: &[String], page: &PageSummary) {
    let Some((head, tail)) = comps.split_first() else {
        return;
    };
    if tail.is_empty() {
        attach_file(node, head, page);
        return;
    }
    let child_folder = node.folder.join(head);
    let index = dir_child(node, head, &child_folder);
    insert_comps(&mut node.children[index], tail, page);
}

fn attach_file(node: &mut TreeNode, name: &str, page: &PageSummary) {
    if name == "index.md" {
        node.page_id = Some(page.frontmatter.id);
        return;
    }
    node.children.push(TreeNode {
        name: page.frontmatter.title.clone(),
        is_dir: false,
        page_id: Some(page.frontmatter.id),
        create_in: node.folder.clone(),
        folder: PathBuf::new(),
        children: Vec::new(),
    });
}

fn dir_child(node: &mut TreeNode, name: &str, folder: &Path) -> usize {
    if let Some(index) = node
        .children
        .iter()
        .position(|child| child.is_dir && child.name == name)
    {
        return index;
    }
    node.children.push(TreeNode {
        name: name.to_owned(),
        is_dir: true,
        page_id: None,
        create_in: folder.to_path_buf(),
        folder: folder.to_path_buf(),
        children: Vec::new(),
    });
    node.children.len() - 1
}

fn sort_tree(node: &mut TreeNode) {
    node.children.sort_by(|left, right| {
        right
            .is_dir
            .cmp(&left.is_dir)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    for child in &mut node.children {
        sort_tree(child);
    }
}
