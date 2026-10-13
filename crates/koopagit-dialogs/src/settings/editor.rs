//! Opening config files in the user's editor (TortoiseGit's "alternative editor",
//! which defaults to Notepad on Windows).

use std::path::Path;
use std::process::Command;

/// Opens `path` with the platform's default text editor, detached.
pub fn open(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, "")?;
    }
    platform::command(path).spawn().map(drop)
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;

    pub fn command(path: &Path) -> Command {
        let mut c = Command::new("notepad.exe");
        c.arg(path);
        c
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;

    pub fn command(path: &Path) -> Command {
        // -t: the default text editor, regardless of the file's extension.
        let mut c = Command::new("open");
        c.arg("-t").arg(path);
        c
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod platform {
    use super::*;

    pub fn command(path: &Path) -> Command {
        let mut c = Command::new("xdg-open");
        c.arg(path);
        c
    }
}
