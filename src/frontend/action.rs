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
    ImportMap(ImportMapForm),
    DeleteMap {
        slug: String,
        name: String,
    },
}

/// Fields of the "Import map" dialog. Numbers stay as text until submitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportMapForm {
    pub name: String,
    pub image: Option<PathBuf>,
    pub scale: String,
    pub origin_x: String,
    pub origin_z: String,
    pub dimension: String,
}

impl Default for ImportMapForm {
    fn default() -> Self {
        Self {
            name: String::new(),
            image: None,
            scale: "1".to_owned(),
            origin_x: "0".to_owned(),
            origin_z: "0".to_owned(),
            dimension: "Overworld".to_owned(),
        }
    }
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
    /// Open (or focus) a page in a floating window over the main editor.
    OpenFloating(Uuid),
    /// Save and drop a floating window the user closed.
    CloseFloating(Uuid),
    OpenFolder(PathBuf),
    ShowTimeline,
    /// Open a map (by folder name) in the Map view.
    ShowMap(String),
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
    SubmitImportMap(ImportMapForm),
    SubmitDeleteMap(String),
    OpenDialog(Dialog),
}
