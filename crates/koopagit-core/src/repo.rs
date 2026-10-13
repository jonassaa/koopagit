//! Locating and describing a git working tree.

use std::path::{Path, PathBuf};

use crate::git_cli::Git;
use crate::{Error, Result};

/// An opened git working tree.
pub struct Repo {
    gix: gix::Repository,
    workdir: PathBuf,
}

impl std::fmt::Debug for Repo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Repo")
            .field("workdir", &self.workdir)
            .finish()
    }
}

impl Repo {
    /// Finds the working tree containing `path` (a file or directory), like
    /// TortoiseGit's `GitAdminDir::HasAdminDir`.
    pub fn discover(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let start = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(path)
        };
        let gix = gix::discover(start).map_err(|_| Error::NotAWorkingTree(path.to_owned()))?;
        let workdir = gix
            .workdir()
            .ok_or_else(|| Error::NotAWorkingTree(path.to_owned()))?
            .to_owned();
        let workdir = workdir.canonicalize().unwrap_or(workdir);
        Ok(Repo { gix, workdir })
    }

    /// Like [`Repo::discover`], but also accepts a bare repository (used by
    /// commands with TortoiseGit's `WorkingTreeOrBareRepoRequired`, e.g. `log`).
    /// For a bare repository [`Repo::workdir`] is the git directory itself, so
    /// git commands run there.
    pub fn discover_including_bare(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Ok(repo) = Repo::discover(path) {
            return Ok(repo);
        }
        let start = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(path)
        };
        let gix = gix::discover(start).map_err(|_| Error::NotAWorkingTree(path.to_owned()))?;
        if !gix.is_bare() {
            return Err(Error::NotAWorkingTree(path.to_owned()));
        }
        let dir = gix.git_dir().to_owned();
        let workdir = dir.canonicalize().unwrap_or(dir);
        Ok(Repo { gix, workdir })
    }

    /// Whether this is a bare repository (no working tree).
    pub fn is_bare(&self) -> bool {
        self.gix.is_bare()
    }

    /// Returns `true` if `path` lies inside a git working tree.
    pub fn is_in_working_tree(path: impl AsRef<Path>) -> bool {
        Repo::discover(path).is_ok()
    }

    pub fn workdir(&self) -> &Path {
        &self.workdir
    }

    pub fn git_dir(&self) -> &Path {
        self.gix.git_dir()
    }

    /// The directory shared by all worktrees (holds refs, objects, `refs/stash`).
    pub fn common_dir(&self) -> &Path {
        self.gix.common_dir()
    }

    /// Whether `path` itself is a bare repository (TortoiseGit's `IsBareRepo`).
    pub fn is_bare_repo(path: impl AsRef<Path>) -> bool {
        let path = path.as_ref();
        path.is_dir()
            && path.join("HEAD").is_file()
            && path.join("objects").is_dir()
            && gix::open(path).is_ok_and(|r| r.is_bare())
    }

    /// `(assume_valid, skip_worktree)` index flags of a tracked path.
    pub fn index_flags(&self, path: impl AsRef<Path>) -> (bool, bool) {
        let Some(rel) = self.relative_path(path) else {
            return (false, false);
        };
        let Ok(index) = self.gix.index_or_empty() else {
            return (false, false);
        };
        match index.entry_by_path(rel.as_str().into()) {
            Some(e) => (
                e.flags.contains(gix::index::entry::Flags::ASSUME_VALID),
                e.flags.contains(gix::index::entry::Flags::SKIP_WORKTREE),
            ),
            None => (false, false),
        }
    }

    pub fn gix(&self) -> &gix::Repository {
        &self.gix
    }

    /// A git CLI runner rooted at the working tree.
    pub fn git(&self) -> Git {
        Git::new(&self.workdir)
    }

    /// The path of `path` relative to the working tree root, with `/` separators,
    /// as git expects in pathspecs. Returns `None` if `path` is outside the tree.
    pub fn relative_path(&self, path: impl AsRef<Path>) -> Option<String> {
        let path = path.as_ref();
        let abs = path.canonicalize().unwrap_or_else(|_| path.to_owned());
        let rel = abs.strip_prefix(&self.workdir).ok()?;
        let parts: Vec<_> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        Some(parts.join("/"))
    }

    /// The short name of the checked-out branch, or `None` when HEAD is detached.
    /// An unborn branch (fresh repository) still reports its name.
    pub fn current_branch(&self) -> Result<Option<String>> {
        let name = self.gix.head_name().map_err(Error::gix)?;
        Ok(name.map(|n| n.shorten().to_string()))
    }

    /// The commit id HEAD points to, or `None` for an unborn branch.
    pub fn head_id(&self) -> Result<Option<String>> {
        let mut head = self.gix.head().map_err(Error::gix)?;
        if head.is_unborn() {
            return Ok(None);
        }
        let commit = head.peel_to_commit().map_err(Error::gix)?;
        Ok(Some(commit.id.to_string()))
    }

    /// Local branch names (`refs/heads/*`), sorted.
    pub fn local_branches(&self) -> Result<Vec<String>> {
        self.ref_names("refs/heads/")
    }

    /// Remote-tracking branch names (`refs/remotes/*`, e.g. `origin/main`), sorted.
    pub fn remote_branches(&self) -> Result<Vec<String>> {
        let mut names = self.ref_names("refs/remotes/")?;
        names.retain(|n| !n.ends_with("/HEAD"));
        Ok(names)
    }

    /// Tag names (`refs/tags/*`), sorted.
    pub fn tags(&self) -> Result<Vec<String>> {
        self.ref_names("refs/tags/")
    }

    /// Configured remote names, sorted.
    pub fn remotes(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .gix
            .remote_names()
            .into_iter()
            .map(|n| n.to_string())
            .collect();
        names.sort();
        names
    }

    /// Whether `refs/stash` exists (drives the stash entries of the context menu).
    pub fn has_stash(&self) -> bool {
        self.gix
            .try_find_reference("refs/stash")
            .ok()
            .flatten()
            .is_some()
    }

    fn ref_names(&self, prefix: &str) -> Result<Vec<String>> {
        let platform = self.gix.references().map_err(Error::gix)?;
        let iter = platform.prefixed(prefix).map_err(Error::gix)?;
        let mut names: Vec<String> = iter
            .filter_map(|r| r.ok())
            .filter_map(|r| {
                r.name()
                    .as_bstr()
                    .to_string()
                    .strip_prefix(prefix)
                    .map(str::to_owned)
            })
            .collect();
        names.sort();
        Ok(names)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Creates a repository with one commit containing `a.txt`.
    pub(crate) fn fixture() -> (tempfile::TempDir, Repo) {
        let dir = tempfile::tempdir().unwrap();
        let git = Git::new(dir.path());
        git.run(["init", "-q", "-b", "main"]).unwrap();
        git.run(["config", "user.name", "Test"]).unwrap();
        git.run(["config", "user.email", "test@example.com"])
            .unwrap();
        git.run(["config", "commit.gpgsign", "false"]).unwrap();
        std::fs::write(dir.path().join("a.txt"), "a\n").unwrap();
        git.run(["add", "a.txt"]).unwrap();
        git.run(["commit", "-q", "-m", "initial"]).unwrap();
        let repo = Repo::discover(dir.path()).unwrap();
        (dir, repo)
    }

    #[test]
    fn discovers_from_file_and_subdir() {
        let (dir, repo) = fixture();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        let from_sub = Repo::discover(dir.path().join("sub")).unwrap();
        assert_eq!(from_sub.workdir(), repo.workdir());
        let from_file = Repo::discover(dir.path().join("a.txt")).unwrap();
        assert_eq!(from_file.workdir(), repo.workdir());
        assert_eq!(
            repo.relative_path(dir.path().join("sub")).as_deref(),
            Some("sub")
        );
    }

    #[test]
    fn bare_repositories() {
        let (dir, repo) = fixture();
        assert!(!repo.is_bare());
        let bare = dir.path().join("bare.git");
        repo.git()
            .run(["clone", "-q", "--bare", ".", &bare.to_string_lossy()])
            .unwrap();
        assert!(Repo::discover(&bare).is_err());
        let b = Repo::discover_including_bare(&bare).unwrap();
        assert!(b.is_bare());
        assert_eq!(b.current_branch().unwrap().as_deref(), Some("main"));
    }

    #[test]
    fn not_a_repo() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            Repo::discover(dir.path()),
            Err(Error::NotAWorkingTree(_))
        ));
    }

    #[test]
    fn branches_tags_and_stash() {
        let (dir, repo) = fixture();
        assert_eq!(repo.current_branch().unwrap().as_deref(), Some("main"));
        assert!(repo.head_id().unwrap().is_some());
        let git = repo.git();
        git.run(["branch", "feature"]).unwrap();
        git.run(["tag", "v1"]).unwrap();
        assert_eq!(repo.local_branches().unwrap(), ["feature", "main"]);
        assert_eq!(repo.tags().unwrap(), ["v1"]);
        assert!(!repo.has_stash());
        std::fs::write(dir.path().join("a.txt"), "changed\n").unwrap();
        git.run(["stash", "-q"]).unwrap();
        let repo = Repo::discover(dir.path()).unwrap();
        assert!(repo.has_stash());
    }
}
