use std::path::PathBuf;

use uuid::Uuid;

use crate::backend::TemplateKind;

/// Modal the workspace is waiting on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dialog {
    None,
    CreatePage {
        parent: PathBuf,
        template: TemplateKind,
        title: String,
        linked_nation: Option<String>,
    },
    CreateFolder {
        parent: PathBuf,
        name: String,
    },
    RenamePage {
        id: Uuid,
        title: String,
    },
    RenameFolder {
        dir: PathBuf,
        name: String,
    },
    DeletePage {
        id: Uuid,
        title: String,
    },
    DeleteFolder {
        dir: PathBuf,
    },
}

impl Default for Dialog {
    fn default() -> Self {
        Self::None
    }
}

/// Something the frame wants the application to do after drawing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiAction {
    None,
    OpenPage(Uuid),
    OpenFolder(PathBuf),
    ShowTimeline,
    Save,
    CloseProject,
    OpenProject(PathBuf),
    CreateProject {
        folder: PathBuf,
        name: String,
        date: String,
    },
    SubmitCreatePage {
        parent: PathBuf,
        template: TemplateKind,
        title: String,
        linked_nation: Option<String>,
    },
    SubmitCreateFolder {
        parent: PathBuf,
        name: String,
    },
    SubmitRenamePage {
        id: Uuid,
        title: String,
    },
    SubmitRenameFolder {
        dir: PathBuf,
        name: String,
    },
    SubmitDeletePage(Uuid),
    SubmitDeleteFolder(PathBuf),
    OpenDialog(Dialog),
}
