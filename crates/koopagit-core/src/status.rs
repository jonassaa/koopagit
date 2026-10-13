//! Working-tree status, modelled on TortoiseGit's `git_wc_status_kind` and
//! `CTGitPath::LOGACTIONS_*`.

use std::collections::BTreeMap;

use gix::bstr::ByteSlice;
use gix::status::index_worktree;
use gix_diff_index::ChangeRef;

use crate::{Error, Repo, Result};

// `gix` re-exports `gix_diff`; alias the index-diff module for readability.
mod gix_diff_index {
    pub use gix::diff::index::ChangeRef;
}

/// Status of a single path, ordered by priority exactly like TortoiseGit's
/// `git_wc_status_kind`: when statuses are folded (e.g. a folder's overlay is the
/// "worst" status of its children), the greater value wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum WcStatus {
    #[default]
    None,
    Unversioned,
    Ignored,
    Normal,
    Deleted,
    Modified,
    Added,
    Conflicted,
    Unknown,
}

/// What happened to a path, mirroring `CTGitPath::LOGACTIONS_*` as shown in the
/// status column of TortoiseGit's file lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Unmerged,
    /// Tracked file deleted from the working tree but not from the index.
    Missing,
    Unversioned,
    Ignored,
}

impl Action {
    /// The text TortoiseGit shows in the "Status" column (`CTGitPath::GetActionName`).
    /// Upstream shows "Unknown" for unversioned files; we keep that.
    pub fn name(self) -> &'static str {
        match self {
            Action::Unmerged => "Conflict",
            Action::Added => "Added",
            Action::Missing => "Missing",
            Action::Deleted => "Deleted",
            Action::Modified => "Modified",
            Action::Renamed => "Rename",
            Action::Copied => "Copy",
            Action::Ignored => "Ignored",
            Action::Unversioned => "Unknown",
        }
    }

    pub fn wc_status(self) -> WcStatus {
        match self {
            Action::Added | Action::Copied => WcStatus::Added,
            Action::Modified | Action::Renamed => WcStatus::Modified,
            Action::Deleted | Action::Missing => WcStatus::Deleted,
            Action::Unmerged => WcStatus::Conflicted,
            Action::Unversioned => WcStatus::Unversioned,
            Action::Ignored => WcStatus::Ignored,
        }
    }
}

/// One changed path in the working tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusEntry {
    /// Repository-relative path with `/` separators.
    pub path: String,
    /// Source path of a rename/copy.
    pub old_path: Option<String>,
    /// Change between `HEAD` and the index ("staged").
    pub staged: Option<Action>,
    /// Change between the index and the working tree ("unstaged"), including
    /// untracked/ignored files and conflicts.
    pub unstaged: Option<Action>,
}

impl StatusEntry {
    /// The single action TortoiseGit displays for this path when it does not
    /// distinguish staged and unstaged changes (the classic commit dialog).
    pub fn action(&self) -> Action {
        match (self.staged, self.unstaged) {
            (_, Some(Action::Unmerged)) => Action::Unmerged,
            (Some(Action::Added), Some(Action::Missing | Action::Deleted)) => Action::Missing,
            (Some(s @ (Action::Added | Action::Renamed | Action::Copied)), _) => s,
            (_, Some(Action::Missing)) => Action::Deleted,
            (Some(s), None) => s,
            (Some(Action::Deleted), Some(u)) => u,
            (Some(_), Some(Action::Modified)) => Action::Modified,
            (Some(s), Some(_)) => s,
            (None, Some(u)) => u,
            (None, None) => Action::Modified,
        }
    }

    pub fn wc_status(&self) -> WcStatus {
        self.action().wc_status()
    }
}

/// Which untracked/ignored files to report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StatusOptions {
    /// Report each unversioned file (TortoiseGit's "Show Unversioned Files").
    pub unversioned: bool,
    /// Report ignored files (TortoiseGit's "Show Ignored Files").
    pub ignored: bool,
}

impl StatusOptions {
    pub fn with_unversioned() -> Self {
        StatusOptions {
            unversioned: true,
            ignored: false,
        }
    }
}

impl Repo {
    /// Computes the status of every changed path in the working tree, sorted by path.
    ///
    /// `pathspecs` limits the result to the given repository-relative paths
    /// (directories match recursively); empty means the whole tree.
    pub fn status(&self, options: StatusOptions, pathspecs: &[String]) -> Result<Vec<StatusEntry>> {
        let repo = self.gix();
        let mut platform = repo
            .status(gix::progress::Discard)
            .map_err(Error::gix)?
            .untracked_files(if options.unversioned {
                gix::status::UntrackedFiles::Files
            } else {
                gix::status::UntrackedFiles::None
            });
        if options.ignored {
            platform = platform
                .dirwalk_options(|o| o.emit_ignored(Some(gix::dir::walk::EmissionMode::Matching)));
        }
        let patterns: Vec<gix::bstr::BString> = pathspecs
            .iter()
            .filter(|p| !p.is_empty())
            .map(|p| p.as_str().into())
            .collect();
        let iter = platform.into_iter(patterns).map_err(Error::gix)?;

        let mut entries: BTreeMap<String, StatusEntry> = BTreeMap::new();
        fn entry<'a>(
            entries: &'a mut BTreeMap<String, StatusEntry>,
            path: &str,
        ) -> &'a mut StatusEntry {
            entries
                .entry(path.to_owned())
                .or_insert_with(|| StatusEntry {
                    path: path.to_owned(),
                    old_path: None,
                    staged: None,
                    unstaged: None,
                })
        }
        for item in iter {
            let item = item.map_err(Error::gix)?;
            match item {
                gix::status::Item::TreeIndex(change) => match &change {
                    ChangeRef::Addition { location, .. } => {
                        entry(&mut entries, &location.to_str_lossy()).staged = Some(Action::Added)
                    }
                    ChangeRef::Deletion { location, .. } => {
                        entry(&mut entries, &location.to_str_lossy()).staged = Some(Action::Deleted)
                    }
                    ChangeRef::Modification { location, .. } => {
                        entry(&mut entries, &location.to_str_lossy()).staged =
                            Some(Action::Modified)
                    }
                    ChangeRef::Rewrite {
                        source_location,
                        location,
                        copy,
                        ..
                    } => {
                        let e = entry(&mut entries, &location.to_str_lossy());
                        e.staged = Some(if *copy {
                            Action::Copied
                        } else {
                            Action::Renamed
                        });
                        e.old_path = Some(source_location.to_str_lossy().into_owned());
                    }
                },
                gix::status::Item::IndexWorktree(item) => {
                    let path = item.rela_path().to_str_lossy().into_owned();
                    let action = match &item {
                        index_worktree::Item::Modification { status, .. } => {
                            use gix::status::plumbing::index_as_worktree::{Change, EntryStatus};
                            match status {
                                EntryStatus::Conflict { .. } => Some(Action::Unmerged),
                                EntryStatus::Change(Change::Removed) => Some(Action::Missing),
                                EntryStatus::Change(_) => Some(Action::Modified),
                                EntryStatus::IntentToAdd => Some(Action::Added),
                                EntryStatus::NeedsUpdate(_) => None,
                            }
                        }
                        index_worktree::Item::DirectoryContents { entry, .. } => {
                            match entry.status {
                                gix::dir::entry::Status::Untracked => Some(Action::Unversioned),
                                gix::dir::entry::Status::Ignored(_) => Some(Action::Ignored),
                                _ => None,
                            }
                        }
                        index_worktree::Item::Rewrite { .. } => Some(Action::Unversioned),
                    };
                    if let Some(action) = action {
                        entry(&mut entries, &path).unstaged = Some(action);
                    }
                }
            }
        }
        Ok(entries.into_values().collect())
    }

    /// Status of a single path for overlay icons. Folders fold their children's
    /// statuses (highest [`WcStatus`] wins); a clean tracked path is
    /// [`WcStatus::Normal`].
    pub fn path_status(&self, path: impl AsRef<std::path::Path>) -> Result<WcStatus> {
        self.path_status_with(path, false)
    }

    /// Like [`Repo::path_status`]; with `unversioned_as_modified` (TortoiseGit's
    /// `UnversionedAsModified`), unversioned files make their parent folders modified.
    pub fn path_status_with(
        &self,
        path: impl AsRef<std::path::Path>,
        unversioned_as_modified: bool,
    ) -> Result<WcStatus> {
        let path = path.as_ref();
        let Some(rel) = self.relative_path(path) else {
            return Ok(WcStatus::None);
        };
        if rel == ".git" || rel.starts_with(".git/") {
            return Ok(WcStatus::None);
        }
        let entries = self.status(
            StatusOptions {
                unversioned: true,
                ignored: true,
            },
            &if rel.is_empty() {
                vec![]
            } else {
                vec![rel.clone()]
            },
        )?;
        let uam = unversioned_as_modified
            && path.is_dir()
            && (rel.is_empty() || self.has_tracked_under(&rel)?);
        let folded = entries
            .iter()
            .filter(|e| e.path == rel || rel.is_empty() || e.path.starts_with(&format!("{rel}/")))
            .map(|e| {
                let st = e.wc_status();
                if uam && st == WcStatus::Unversioned {
                    WcStatus::Modified
                } else {
                    st
                }
            })
            .max();
        if path.is_dir() {
            // An untracked/ignored folder takes that status; any other folder is
            // "normal" unless a child is worse.
            return Ok(match folded {
                Some(s) if s > WcStatus::Normal => s,
                Some(s @ (WcStatus::Ignored | WcStatus::Unversioned))
                    if !self.has_tracked_under(&rel)? =>
                {
                    s
                }
                _ => WcStatus::Normal,
            });
        }
        Ok(folded.unwrap_or(WcStatus::Normal))
    }

    fn has_tracked_under(&self, rel: &str) -> Result<bool> {
        if rel.is_empty() {
            // The working-tree root is always versioned.
            return Ok(true);
        }
        let index = self.gix().index_or_empty().map_err(Error::gix)?;
        let prefix = format!("{rel}/");
        Ok(index
            .entries()
            .iter()
            .any(|e| e.path(&index).to_str_lossy().starts_with(&prefix)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::tests::fixture;

    fn find<'a>(entries: &'a [StatusEntry], path: &str) -> &'a StatusEntry {
        entries
            .iter()
            .find(|e| e.path == path)
            .unwrap_or_else(|| panic!("{path} not in {entries:?}"))
    }

    #[test]
    fn reports_staged_unstaged_and_untracked() {
        let (dir, repo) = fixture();
        let git = repo.git();
        std::fs::write(dir.path().join("a.txt"), "changed\n").unwrap();
        std::fs::write(dir.path().join("new.txt"), "n\n").unwrap();
        std::fs::write(dir.path().join("staged.txt"), "s\n").unwrap();
        std::fs::create_dir(dir.path().join("dir")).unwrap();
        std::fs::write(dir.path().join("dir/u.txt"), "u\n").unwrap();
        git.run(["add", "staged.txt"]).unwrap();

        let clean = repo.status(StatusOptions::default(), &[]).unwrap();
        assert_eq!(clean.len(), 2, "{clean:?}");
        assert_eq!(find(&clean, "a.txt").action(), Action::Modified);
        assert_eq!(find(&clean, "staged.txt").staged, Some(Action::Added));

        let all = repo.status(StatusOptions::with_unversioned(), &[]).unwrap();
        assert_eq!(find(&all, "new.txt").action(), Action::Unversioned);
        assert_eq!(find(&all, "dir/u.txt").action(), Action::Unversioned);
    }

    #[test]
    fn deleted_and_conflicted() {
        let (dir, repo) = fixture();
        let git = repo.git();
        git.run(["checkout", "-q", "-b", "other"]).unwrap();
        std::fs::write(dir.path().join("a.txt"), "other\n").unwrap();
        git.run(["commit", "-qam", "other"]).unwrap();
        git.run(["checkout", "-q", "main"]).unwrap();
        std::fs::write(dir.path().join("a.txt"), "main\n").unwrap();
        git.run(["commit", "-qam", "main"]).unwrap();
        assert!(!git.output(["merge", "other"]).unwrap().success());
        let entries = repo.status(StatusOptions::default(), &[]).unwrap();
        assert_eq!(find(&entries, "a.txt").action(), Action::Unmerged);
        assert_eq!(repo.path_status(dir.path()).unwrap(), WcStatus::Conflicted);

        git.run(["merge", "--abort"]).unwrap();
        std::fs::remove_file(dir.path().join("a.txt")).unwrap();
        let entries = repo.status(StatusOptions::default(), &[]).unwrap();
        assert_eq!(find(&entries, "a.txt").action(), Action::Deleted);
    }

    #[test]
    fn overlay_status_of_paths() {
        let (dir, repo) = fixture();
        std::fs::write(dir.path().join(".gitignore"), "*.log\n").unwrap();
        std::fs::write(dir.path().join("x.log"), "").unwrap();
        std::fs::write(dir.path().join("n.txt"), "").unwrap();
        std::fs::create_dir(dir.path().join("untracked")).unwrap();
        std::fs::write(dir.path().join("untracked/f"), "").unwrap();
        let p = |s: &str| dir.path().join(s);
        assert_eq!(repo.path_status(p("a.txt")).unwrap(), WcStatus::Normal);
        assert_eq!(repo.path_status(p("x.log")).unwrap(), WcStatus::Ignored);
        assert_eq!(repo.path_status(p("n.txt")).unwrap(), WcStatus::Unversioned);
        assert_eq!(
            repo.path_status(p("untracked")).unwrap(),
            WcStatus::Unversioned
        );
        assert_eq!(repo.path_status(dir.path()).unwrap(), WcStatus::Normal);
        assert_eq!(
            repo.path_status_with(dir.path(), true).unwrap(),
            WcStatus::Modified
        );
        assert_eq!(
            repo.path_status_with(p("untracked"), true).unwrap(),
            WcStatus::Unversioned
        );
        std::fs::write(p("a.txt"), "x").unwrap();
        assert_eq!(repo.path_status(p("a.txt")).unwrap(), WcStatus::Modified);
        assert_eq!(repo.path_status(dir.path()).unwrap(), WcStatus::Modified);
    }
}
