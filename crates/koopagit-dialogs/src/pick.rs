//! Native file and folder pickers (`rfd`), for "Save revision to...",
//! "Export selection to...", Clone/Export "Browse..." buttons.

use std::path::{Path, PathBuf};

/// Asks for a folder. `start` is the initially shown directory.
pub fn folder(title: &str, start: Option<&Path>) -> Option<PathBuf> {
    let mut d = rfd::FileDialog::new().set_title(title);
    if let Some(s) = start {
        d = d.set_directory(s);
    }
    d.pick_folder()
}

/// Asks for a file to open; `filters` are `(name, extensions)` pairs.
pub fn open_file(
    title: &str,
    start: Option<&Path>,
    filters: &[(&str, &[&str])],
) -> Option<PathBuf> {
    let mut d = rfd::FileDialog::new().set_title(title);
    if let Some(s) = start {
        d = d.set_directory(s);
    }
    for (name, exts) in filters {
        d = d.add_filter(*name, exts);
    }
    d.pick_file()
}

/// Asks for several files to open.
pub fn open_files(title: &str, start: Option<&Path>, filters: &[(&str, &[&str])]) -> Vec<PathBuf> {
    let mut d = rfd::FileDialog::new().set_title(title);
    if let Some(s) = start {
        d = d.set_directory(s);
    }
    for (name, exts) in filters {
        d = d.add_filter(*name, exts);
    }
    d.pick_files().unwrap_or_default()
}

/// Asks where to save a file, proposing `file_name` in `start`.
pub fn save_file(title: &str, start: Option<&Path>, file_name: &str) -> Option<PathBuf> {
    let mut d = rfd::FileDialog::new()
        .set_title(title)
        .set_file_name(file_name);
    if let Some(s) = start {
        d = d.set_directory(s);
    }
    d.save_file()
}
