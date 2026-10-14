//! Native file and folder pickers (`rfd`), for "Save revision to...",
//! "Export selection to...", Clone/Export "Browse..." buttons, and the
//! Log / Browse References picker processes.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use koopagit_core::launcher::Invocation;

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

/// Runs a picker dialog (`inv` plus `/outfile:<tmp>`, e.g. the Log or Browse
/// References) as its own process without blocking the caller's dialog —
/// upstream opens them modally in-process. When the user picks something,
/// `done` gets the first line written to the file, on the UI thread.
pub fn launcher_picker(inv: Invocation, done: impl FnOnce(String) + Send + 'static) {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let out = std::env::temp_dir().join(format!(
        "koopagit-pick-{}-{}-{}.txt",
        std::process::id(),
        crate::format::now(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let inv = inv.with("outfile", out.to_string_lossy());
    let exe = std::env::current_exe().unwrap_or_else(|_| "koopagit-proc".into());
    std::thread::spawn(move || {
        let status = std::process::Command::new(exe).args(inv.to_args()).status();
        let picked = std::fs::read_to_string(&out).unwrap_or_default();
        let _ = std::fs::remove_file(&out);
        let line = picked.lines().next().unwrap_or_default().trim().to_owned();
        if status.is_ok() && !line.is_empty() {
            let _ = slint::invoke_from_event_loop(move || done(line));
        }
    });
}
