//use egui_commonmark::*;
use crate::backend::buffers::TextBuf;
use std::path::PathBuf;
//possible solution for text editing with Rope
use ropey::Rope;

pub struct File {
    name: String,
    path: PathBuf,
    main_buf: TextBuf,
}

impl File {
    pub fn new(name: String, path: PathBuf) -> Self {
        Self {
            name,
            path,
            main_buf: TextBuf::new(),
        }
    }
}
