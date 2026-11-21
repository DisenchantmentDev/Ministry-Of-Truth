use std::path::{Path, PathBuf};

use eframe::egui::Context;
use egui::{FontFamily, FontId, TextStyle};
use uuid::Uuid;

use crate::backend::{
    instantiate, parse_start_date, Frontmatter, MotError, Page, Project, Result, TemplateKind,
};
use crate::frontend::{
    pick_folder, show_launch, show_project, Dialog, FloatingPage, MainView, NationTab, OpenPage,
    Session, UiAction,
};

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct Application {
    state: WindowState,
    project_dir: PathBuf,
    open_page_id: Option<Uuid>,
    pub(crate) main_view: MainView,
    #[serde(skip)]
    pub(crate) project: Option<Project>,
    #[serde(skip)]
    pub(crate) session: Session,
}

#[derive(Debug, Default, PartialEq, Clone, Copy, Eq, serde::Deserialize, serde::Serialize)]
pub enum WindowState {
    #[default]
    Landing,
    Project,
}

impl Default for Application {
    fn default() -> Self {
        Self {
            state: WindowState::Landing,
            project_dir: PathBuf::new(),
            open_page_id: None,
            main_view: MainView::Page,
            project: None,
            session: Session::default(),
        }
    }
}

impl eframe::App for Application {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if ui.input(|input| input.modifiers.command && input.key_pressed(egui::Key::S)) {
            self.apply(UiAction::Save);
        }
        let mut actions = Vec::new();
        egui::Panel::top("top_panel").show(ui, |ui| {
            actions.extend(self.menu(ui));
        });
        actions.extend(self.body(ui));
        for action in actions {
            self.apply(action);
        }
    }
}

impl Application {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        set_style(&cc.egui_ctx);
        let mut app: Self = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, eframe::APP_KEY))
            .unwrap_or_default();
        app.restore();
        app
    }

    fn restore(&mut self) {
        if self.state != WindowState::Project || self.project_dir.as_os_str().is_empty() {
            self.state = WindowState::Landing;
            return;
        }
        let saved_id = self.open_page_id;
        let saved_view = self.main_view;
        match Project::open(&self.project_dir) {
            Ok(project) => {
                self.project = Some(project);
                self.state = WindowState::Project;
                if let Some(id) = saved_id.filter(|id| self.project_contains(*id)) {
                    if let Err(err) = self.open_page(id) {
                        self.session.status = err.to_string();
                    }
                    self.main_view = saved_view;
                }
            }
            Err(err) => {
                self.state = WindowState::Landing;
                self.session.status = err.to_string();
            }
        }
    }

    fn menu(&mut self, ui: &mut egui::Ui) -> Vec<UiAction> {
        let mut actions = Vec::new();
        let in_project = self.state == WindowState::Project && self.project.is_some();
        let name = self
            .project
            .as_ref()
            .map(|project| project.meta.name.clone());
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New").clicked() {
                    actions.push(UiAction::CloseProject);
                }
                if ui.button("Open").clicked() {
                    if let Some(folder) = pick_folder() {
                        actions.push(UiAction::OpenProject(folder));
                    }
                }
                if ui
                    .add_enabled(in_project, egui::Button::new("Save"))
                    .clicked()
                {
                    actions.push(UiAction::Save);
                }
                if ui
                    .add_enabled(in_project, egui::Button::new("Close project"))
                    .clicked()
                {
                    actions.push(UiAction::CloseProject);
                }
                if !cfg!(target_arch = "wasm32") && ui.button("Quit").clicked() {
                    ui.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            if let Some(name) = name {
                ui.separator();
                ui.label(name);
            }
            ui.add_space(16.0);
            egui::widgets::global_theme_preference_buttons(ui);
        });
        actions
    }

    fn body(&mut self, ui: &mut egui::Ui) -> Vec<UiAction> {
        match self.state {
            WindowState::Landing => {
                let last = (!self.project_dir.as_os_str().is_empty()).then(|| self.project_dir.clone());
                vec![show_launch(ui, &mut self.session, last.as_deref())]
            }
            WindowState::Project => show_project(ui, self),
        }
    }

    fn apply(&mut self, action: UiAction) {
        if matches!(action, UiAction::None) {
            return;
        }
        if let Err(err) = self.dispatch(action) {
            self.session.status = err.to_string();
        }
    }

    fn dispatch(&mut self, action: UiAction) -> Result<()> {
        match action {
            UiAction::None => Ok(()),
            UiAction::OpenDialog(dialog) => {
                self.session.dialog = dialog;
                Ok(())
            }
            UiAction::Save => self.save_with_status(),
            UiAction::ShowTimeline => self.show_timeline(),
            UiAction::OpenPage(id) => self.open_page(id),
            UiAction::OpenFloating(id) => self.open_floating(id),
            UiAction::CloseFloating(id) => self.close_floating(id),
            UiAction::OpenFolder(path) => self.open_folder(&path),
            UiAction::CloseProject => self.close_project(),
            UiAction::OpenProject(path) => self.open_project(path),
            UiAction::CreateProject { folder, name, date } => self.create_project(folder, name, date),
            UiAction::SubmitCreatePage {
                parent,
                template,
                title,
                linked_nation,
            } => self.create_from_template(parent, template, title, linked_nation),
            UiAction::SubmitCreateFolder { parent, name } => self.create_folder(parent, name),
            UiAction::SubmitRenamePage { id, title } => self.rename_page(id, title),
            UiAction::SubmitRenameFolder { dir, name } => self.rename_folder(dir, name),
            UiAction::SubmitDeletePage(id) => self.delete_page(id),
            UiAction::SubmitDeleteFolder(path) => self.delete_folder(path),
        }
    }

    fn save_with_status(&mut self) -> Result<()> {
        self.save_open()?;
        self.save_all_floating()?;
        self.session.status = "Saved.".to_owned();
        Ok(())
    }

    fn show_timeline(&mut self) -> Result<()> {
        self.save_open()?;
        self.main_view = MainView::Timeline;
        Ok(())
    }

    fn project_contains(&self, id: Uuid) -> bool {
        self.project.as_ref().is_some_and(|project| project.contains(id))
    }

    fn require_project(&mut self) -> Result<&mut Project> {
        self.project
            .as_mut()
            .ok_or_else(|| MotError::new("no project is open"))
    }
}

fn set_style(ctx: &Context) {
    let mut style = (*ctx.global_style()).clone();
    style.text_styles = [
        (TextStyle::Heading, FontId::new(30.0, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(18.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(14.0, FontFamily::Monospace)),
        (TextStyle::Small, FontId::new(14.0, FontFamily::Monospace)),
        (TextStyle::Monospace, FontId::new(14.0, FontFamily::Monospace)),
    ]
    .into();
    ctx.set_global_style(style);
}

impl Application {
    fn save_open(&mut self) -> Result<()> {
        let Some(open) = self.session.open.as_ref() else {
            return Ok(());
        };
        if !open.is_dirty() {
            return Ok(());
        }
        let id = open.id;
        let frontmatter = open.frontmatter.clone();
        let body = open.body.clone();
        let saved = self.require_project()?.save_page(id, frontmatter, &body)?;
        if let Some(open) = self.session.open.as_mut() {
            open.mark_saved(saved.frontmatter, saved.body, saved.path);
        }
        Ok(())
    }

    fn save_all_floating(&mut self) -> Result<()> {
        let dirty: Vec<(Uuid, Frontmatter, String)> = self
            .session
            .floating
            .iter()
            .filter(|page| page.open.is_dirty())
            .map(|page| {
                (
                    page.open.id,
                    page.open.frontmatter.clone(),
                    page.open.body.clone(),
                )
            })
            .collect();
        for (id, frontmatter, body) in dirty {
            let saved = self.require_project()?.save_page(id, frontmatter, &body)?;
            if let Some(page) = self
                .session
                .floating
                .iter_mut()
                .find(|page| page.open.id == id)
            {
                page.open
                    .mark_saved(saved.frontmatter, saved.body, saved.path);
            }
        }
        Ok(())
    }

    fn open_page(&mut self, id: Uuid) -> Result<()> {
        self.save_open()?;
        let page = self.load_existing(id)?;
        self.install_page(page);
        Ok(())
    }

    fn open_floating(&mut self, id: Uuid) -> Result<()> {
        if let Some(existing) = self
            .session
            .floating
            .iter_mut()
            .find(|page| page.open.id == id)
        {
            existing.visible = true;
            return Ok(());
        }
        let page = self.load_existing(id)?;
        self.session.floating.push(FloatingPage::from_page(page));
        Ok(())
    }

    fn close_floating(&mut self, id: Uuid) -> Result<()> {
        if let Some(index) = self
            .session
            .floating
            .iter()
            .position(|page| page.open.id == id)
        {
            let page = &self.session.floating[index];
            if page.open.is_dirty() {
                let frontmatter = page.open.frontmatter.clone();
                let body = page.open.body.clone();
                self.require_project()?.save_page(id, frontmatter, &body)?;
            }
            self.session.floating.remove(index);
        }
        Ok(())
    }

    fn load_existing(&self, id: Uuid) -> Result<Page> {
        self.project
            .as_ref()
            .ok_or_else(|| MotError::new("no project is open"))?
            .load_page(id)
    }

    fn install_page(&mut self, page: Page) {
        self.open_page_id = Some(page.frontmatter.id);
        self.main_view = MainView::Page;
        self.session.nation_tab = NationTab::Notes;
        self.session.open = Some(OpenPage::from_page(page));
    }

    fn open_folder(&mut self, relative: &Path) -> Result<()> {
        self.save_open()?;
        let index = self
            .project
            .as_ref()
            .ok_or_else(|| MotError::new("no project is open"))?
            .root
            .join(relative)
            .join("index.md");
        if let Some(id) = self.page_id_at(&index) {
            return self.open_page(id);
        }
        let title = relative
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Folder")
            .to_owned();
        let parent = index
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| relative.to_path_buf());
        let id = self
            .require_project()?
            .create_page(&parent, Frontmatter::note(title), "")?;
        self.open_page(id)
    }

    fn page_id_at(&self, path: &Path) -> Option<Uuid> {
        self.project.as_ref().and_then(|project| {
            project
                .pages()
                .iter()
                .find(|page| page.path == path)
                .map(|page| page.frontmatter.id)
        })
    }

    fn close_project(&mut self) -> Result<()> {
        self.save_open()?;
        self.save_all_floating()?;
        self.project = None;
        self.session.open = None;
        self.session.floating.clear();
        self.session.dialog = Dialog::None;
        self.state = WindowState::Landing;
        self.open_page_id = None;
        self.main_view = MainView::Page;
        Ok(())
    }

    fn open_project(&mut self, path: PathBuf) -> Result<()> {
        self.save_open()?;
        let project = Project::open(&path)?;
        self.session.status = format!("Opened {}.", project.meta.name);
        self.set_project(project)
    }

    fn create_project(&mut self, folder: PathBuf, name: String, date: String) -> Result<()> {
        self.save_open()?;
        let start = parse_start_date(&date)?;
        let project = Project::create(&folder, &name, start)?;
        self.session.status = "Project created.".to_owned();
        self.set_project(project)
    }

    fn set_project(&mut self, project: Project) -> Result<()> {
        self.project_dir = project.root.clone();
        let root_id = project
            .pages()
            .iter()
            .find(|page| page.path == project.root.join("index.md"))
            .map(|page| page.frontmatter.id);
        self.project = Some(project);
        self.state = WindowState::Project;
        self.session.open = None;
        self.session.floating.clear();
        self.session.dialog = Dialog::None;
        self.main_view = MainView::Page;
        self.open_page_id = None;
        if let Some(id) = root_id {
            self.open_page(id)?;
        }
        Ok(())
    }

    fn create_from_template(
        &mut self,
        parent: PathBuf,
        template: TemplateKind,
        title: String,
        linked_nation: Option<String>,
    ) -> Result<()> {
        self.save_open()?;
        let (mut frontmatter, body) = instantiate(template, title);
        if let Some(nation) = linked_nation {
            if let Some(meta) = frontmatter.event_mut() {
                meta.nations.push(nation);
            }
        }
        let directory = if let Some(section) = template.section() {
            self.require_project()?.ensure_section(section)?
        } else {
            self.project
                .as_ref()
                .ok_or_else(|| MotError::new("no project is open"))?
                .root
                .join(parent)
        };
        let id = self
            .require_project()?
            .create_page(&directory, frontmatter, &body)?;
        self.open_page(id)?;
        self.session.status = "Created page.".to_owned();
        Ok(())
    }

    fn create_folder(&mut self, parent: PathBuf, name: String) -> Result<()> {
        self.save_open()?;
        let id = self.require_project()?.create_folder(&parent, &name)?;
        self.open_page(id)?;
        self.session.status = "Created folder.".to_owned();
        Ok(())
    }

    fn rename_page(&mut self, id: Uuid, title: String) -> Result<()> {
        self.save_open()?;
        if self.session.open.as_ref().is_some_and(|page| page.id == id) {
            if let Some(open) = self.session.open.as_mut() {
                open.frontmatter.title = title;
            }
            self.save_open()?;
            self.session.status = "Renamed.".to_owned();
            return Ok(());
        }
        let page = self.load_existing(id)?;
        let mut frontmatter = page.frontmatter;
        frontmatter.title = title;
        let body = page.body.contents();
        self.require_project()?.save_page(id, frontmatter, &body)?;
        self.session.status = "Renamed.".to_owned();
        Ok(())
    }

    fn rename_folder(&mut self, dir: PathBuf, name: String) -> Result<()> {
        self.save_open()?;
        let open_id = self.session.open.as_ref().map(|page| page.id);
        self.require_project()?.rename_folder(&dir, &name)?;
        if let Some(id) = open_id.filter(|id| self.project_contains(*id)) {
            let page = self.load_existing(id)?;
            self.install_page(page);
        }
        self.session.status = "Renamed folder.".to_owned();
        Ok(())
    }

    fn delete_page(&mut self, id: Uuid) -> Result<()> {
        if self.session.open.as_ref().is_some_and(|page| page.id == id) {
            self.session.open = None;
            self.open_page_id = None;
        }
        self.session.floating.retain(|page| page.open.id != id);
        self.require_project()?.delete_page(id)?;
        self.session.status = "Deleted.".to_owned();
        Ok(())
    }

    fn delete_folder(&mut self, relative: PathBuf) -> Result<()> {
        let absolute = self
            .project
            .as_ref()
            .ok_or_else(|| MotError::new("no project is open"))?
            .root
            .join(&relative);
        if self
            .session
            .open
            .as_ref()
            .is_some_and(|page| page.path.starts_with(&absolute))
        {
            self.session.open = None;
            self.open_page_id = None;
        }
        self.session
            .floating
            .retain(|page| !page.open.path.starts_with(&absolute));
        self.require_project()?.delete_folder(&relative)?;
        self.session.status = "Deleted folder.".to_owned();
        Ok(())
    }
}
