use std::path::PathBuf;

use egui_commonmark::CommonMarkCache;
use uuid::Uuid;

use crate::backend::{Frontmatter, Page};
use crate::frontend::live_markdown::LiveMarkdownState;
use crate::frontend::Dialog;

/// Which markdown surface is in the main pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MainView {
    Page,
    Timeline,
}

impl Default for MainView {
    fn default() -> Self {
        Self::Page
    }
}

/// How the page body is shown in the main pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditMode {
    Edit,
    Preview,
    Split,
    /// Styled source editing (live markdown rendering).
    Live,
}

impl Default for EditMode {
    fn default() -> Self {
        Self::Split
    }
}

/// Extra nation-page tabs. Notes is the markdown body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NationTab {
    Notes,
    Events,
    Relations,
}

impl Default for NationTab {
    fn default() -> Self {
        Self::Notes
    }
}

/// The page currently loaded into the editor.
#[derive(Debug, Clone)]
pub struct OpenPage {
    pub id: Uuid,
    pub path: PathBuf,
    pub saved_frontmatter: Frontmatter,
    pub saved_body: String,
    pub frontmatter: Frontmatter,
    pub body: String,
}

impl OpenPage {
    pub(crate) fn from_page(page: Page) -> Self {
        let body = page.body.contents();
        Self {
            id: page.frontmatter.id,
            path: page.path,
            saved_frontmatter: page.frontmatter.clone(),
            saved_body: body.clone(),
            frontmatter: page.frontmatter,
            body,
        }
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.frontmatter != self.saved_frontmatter || self.body != self.saved_body
    }

    pub(crate) fn mark_saved(&mut self, frontmatter: Frontmatter, body: String, path: PathBuf) {
        self.id = frontmatter.id;
        self.path = path;
        self.saved_frontmatter = frontmatter.clone();
        self.saved_body = body.clone();
        self.frontmatter = frontmatter;
        self.body = body;
    }
}

/// A page open in a movable, resizable overlay window.
pub struct FloatingPage {
    pub open: OpenPage,
    pub visible: bool,
    pub mode: EditMode,
    pub nation_tab: NationTab,
    pub cache: CommonMarkCache,
    pub live_markdown: LiveMarkdownState,
}

impl FloatingPage {
    pub(crate) fn from_page(page: Page) -> Self {
        Self {
            open: OpenPage::from_page(page),
            visible: true,
            mode: EditMode::default(),
            nation_tab: NationTab::default(),
            cache: CommonMarkCache::default(),
            live_markdown: LiveMarkdownState::default(),
        }
    }
}

/// Frame-local UI state that is not written into `project.json`.
pub struct Session {
    pub open: Option<OpenPage>,
    pub mode: EditMode,
    pub nation_tab: NationTab,
    pub dialog: Dialog,
    pub status: String,
    pub cache: CommonMarkCache,
    pub live_markdown: LiveMarkdownState,
    pub floating: Vec<FloatingPage>,
    pub new_name: String,
    pub new_date: String,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            open: None,
            mode: EditMode::Split,
            nation_tab: NationTab::Notes,
            dialog: Dialog::None,
            status: String::new(),
            cache: CommonMarkCache::default(),
            live_markdown: LiveMarkdownState::default(),
            floating: Vec::new(),
            new_name: String::new(),
            new_date: String::new(),
        }
    }
}
