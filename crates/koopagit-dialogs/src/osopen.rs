//! Handing paths to the desktop: `ShellOpen` and `ExploreTo`.

use std::path::Path;
use std::process::Command;

/// Opens `path` with its default application (`CAppUtils::ShellOpen`).
pub fn open(path: &Path) -> std::io::Result<()> {
    let mut c = if cfg!(target_os = "macos") {
        Command::new("open")
    } else if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    } else {
        Command::new("xdg-open")
    };
    c.arg(path).spawn().map(|_| ())
}

/// Shows `path` in the file manager (`CAppUtils::ExploreTo`): a folder is
/// opened, a file is revealed in its folder where the platform supports it.
pub fn explore_to(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        return open(path);
    }
    if cfg!(target_os = "macos") {
        Command::new("open").arg("-R").arg(path).spawn().map(|_| ())
    } else if cfg!(windows) {
        Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn()
            .map(|_| ())
    } else {
        open(path.parent().unwrap_or(path))
    }
}
