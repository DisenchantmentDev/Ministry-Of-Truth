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

impl Application {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        } else {
            Default::default()
        }
    }
}
