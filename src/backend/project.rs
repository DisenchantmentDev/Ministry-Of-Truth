use crate::backend::file::File;
use std::path::PathBuf;

pub struct Project {
    pub project_root: PathBuf,
    loaded_files: Vec<File>,
}
