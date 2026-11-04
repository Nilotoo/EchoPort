//! Native dialog wrappers. Desktop uses rfd; Android has its own SAF picker
//! for the local-music import flow, so the desktop-only dialogs are no-ops.

use std::path::PathBuf;

#[cfg(not(target_os = "android"))]
pub fn pick_file(title: &str, filters: &[(&str, &[&str])]) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new().set_title(title);
    for (name, extensions) in filters {
        dialog = dialog.add_filter(*name, extensions);
    }
    dialog.pick_file()
}

#[cfg(target_os = "android")]
pub fn pick_file(_title: &str, _filters: &[(&str, &[&str])]) -> Option<PathBuf> {
    None
}

#[cfg(not(target_os = "android"))]
pub fn pick_folder(title: &str) -> Option<PathBuf> {
    rfd::FileDialog::new().set_title(title).pick_folder()
}

#[cfg(target_os = "android")]
pub fn pick_folder(_title: &str) -> Option<PathBuf> {
    None
}

#[cfg(not(target_os = "android"))]
pub fn save_file(title: &str, filename: &str, filters: &[(&str, &[&str])]) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new()
        .set_title(title)
        .set_file_name(filename);
    for (name, extensions) in filters {
        dialog = dialog.add_filter(*name, extensions);
    }
    dialog.save_file()
}

#[cfg(target_os = "android")]
pub fn save_file(_title: &str, _filename: &str, _filters: &[(&str, &[&str])]) -> Option<PathBuf> {
    None
}
