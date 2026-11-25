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

/// Ask for a PNG or JPEG to use as a map image.
pub(crate) fn pick_image() -> Option<PathBuf> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        rfd::FileDialog::new()
            .set_title("Choose a map image")
            .add_filter("Images", &["png", "jpg", "jpeg"])
            .pick_file()
    }
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
}
