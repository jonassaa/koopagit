//! Slint implementations of TortoiseProc's dialogs.

// The Slint components live in their own crate so editing this one doesn't
// recompile the generated code.
pub use koopagit_ui::*;

pub mod applypatch;
pub mod clone;
pub mod commit;
pub mod diffcmd;
pub mod editor;
pub mod export;
pub mod filediff;
pub mod filelist;
pub mod filelist_actions;
pub mod filelist_menu;
pub mod fileops;
pub mod format;
pub mod msgbox;
pub mod osopen;
pub mod pick;
pub mod progress;
pub mod pullfetch;
pub mod push;
pub mod rebase;
pub mod refs;
pub mod remote_util;
pub mod selection;
pub mod settings;
pub mod snapshot;
pub mod subadd;
pub mod sync;
pub mod trashcan;
pub mod viewers;
pub mod workingtree;
pub mod worktree;

pub mod clipboard;
pub mod log;
pub mod menus;
pub mod misc;
pub mod nonmodal;
pub mod refbrowse;
pub mod reflog;
pub mod repobrowser;
pub mod reset;
// Worker C: menus for reflog/refs/rebase/viewers.
pub mod commit_message_menu;
pub mod prompt;
pub mod rowsel;

// Worker E: remaining commands.
pub mod extras;
pub mod hookrun;
pub mod revgraph;
pub mod sendmail;
pub mod svn;

pub use nonmodal::keep_alive;

use std::path::{Path, PathBuf};

use koopagit_core::Repo;
use koopagit_core::launcher::Invocation;
use koopagit_core::settings::Settings;

pub type DialogResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Everything a dialog needs: the repository, settings and the launcher arguments.
pub struct Context {
    pub repo: Repo,
    pub settings: Settings,
    pub invocation: Invocation,
    /// Absolute target paths (`/path`, `/pathfile`); may be empty.
    pub paths: Vec<PathBuf>,
}

impl Context {
    /// Target paths relative to the work-tree root (git pathspecs); the root
    /// itself becomes an empty list.
    pub fn rel_paths(&self) -> Vec<String> {
        self.paths
            .iter()
            .filter_map(|p| self.repo.relative_path(p))
            .filter(|p| !p.is_empty())
            .collect()
    }

    /// The single path or common directory shown in window titles.
    pub fn title_path(&self) -> PathBuf {
        match self.paths.as_slice() {
            [] => self.repo.workdir().to_owned(),
            [one] => one.clone(),
            many => common_dir(many).unwrap_or_else(|| self.repo.workdir().to_owned()),
        }
    }

    /// `CAppUtils::SetWindowTitle`: `<path> - <dialog> - KoopaGit`.
    pub fn window_title(&self, dialog: &str) -> String {
        window_title(&self.title_path(), dialog)
    }

    /// `/command:... /path:<workdir>` for post-command buttons.
    pub fn launch(&self, command: &str) -> Invocation {
        Invocation::new(command).with_paths([self.repo.workdir()])
    }
}

pub fn window_title(path: &Path, dialog: &str) -> String {
    let max = 80usize.saturating_sub(dialog.len());
    let p = path.to_string_lossy();
    let p = if p.chars().count() > max {
        let tail: String = p
            .chars()
            .rev()
            .take(max.saturating_sub(3))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("...{tail}")
    } else {
        p.into_owned()
    };
    format!("{p} - {dialog} - KoopaGit")
}

fn common_dir(paths: &[PathBuf]) -> Option<PathBuf> {
    let mut iter = paths.iter();
    let mut common = iter.next()?.clone();
    for p in iter {
        while !p.starts_with(&common) {
            common = common.parent()?.to_owned();
        }
    }
    Some(common)
}

/// Starts another launcher process for `/command:` invocations (post-command
/// buttons, context-menu actions inside dialogs). Errors are reported on stderr;
/// a failed spawn must not take down the current dialog.
pub fn spawn_launcher(invocation: &Invocation) {
    let exe = std::env::current_exe().unwrap_or_else(|_| "koopagit-proc".into());
    if let Err(e) = std::process::Command::new(exe)
        .args(invocation.to_args())
        .spawn()
    {
        eprintln!("koopagit: failed to launch {:?}: {e}", invocation.command);
    }
}

/// Writes `text` to a temporary UTF-8 file (`SaveCommitUnicodeFile`) and returns its path.
pub fn temp_message_file(text: &str) -> std::io::Result<PathBuf> {
    let dir = std::env::temp_dir().join("koopagit");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("msg-{}-{}.txt", std::process::id(), format::now()));
    std::fs::write(&path, text)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_and_common_dir() {
        assert_eq!(
            window_title(Path::new("/r/a"), "Commit"),
            "/r/a - Commit - KoopaGit"
        );
        assert_eq!(
            common_dir(&[PathBuf::from("/r/a/x"), PathBuf::from("/r/b")]),
            Some(PathBuf::from("/r"))
        );
    }
}
#[path = "menus/log.rs"]
pub mod log_menu;
pub mod logactions;
pub mod logtools;
