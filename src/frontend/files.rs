use std::path::PathBuf;

pub(crate) fn pick_folder() -> Option<PathBuf> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        rfd::FileDialog::new()
            .set_title("Choose a project folder")
            .pick_folder()
    }
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
}
