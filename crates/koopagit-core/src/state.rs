//! A snapshot of repository state that context menus and dialogs consult
//! (TortoiseGit checks these per menu entry: merge/rebase/bisect active, ...).

use crate::Repo;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepoState {
    /// Short name of the checked-out branch; `None` when detached.
    pub branch: Option<String>,
    /// Full id of HEAD; `None` for an unborn branch.
    pub head: Option<String>,
    pub merge_active: bool,
    /// A `git rebase` / `git am` or TortoiseGit rebase is in progress.
    pub rebase_active: bool,
    pub cherry_pick_active: bool,
    pub revert_active: bool,
    pub bisect_active: bool,
    pub has_stash: bool,
    pub has_submodules: bool,
    pub uses_lfs: bool,
    pub is_git_svn: bool,
    /// Configured remotes, sorted.
    pub remotes: Vec<String>,
}

impl RepoState {
    pub fn is_detached(&self) -> bool {
        self.branch.is_none() && self.head.is_some()
    }

    /// Any sequencer-style operation that blocks starting another one.
    pub fn busy(&self) -> bool {
        self.merge_active || self.rebase_active || self.cherry_pick_active || self.revert_active
    }
}

impl Repo {
    /// Reads the current [`RepoState`].
    pub fn state(&self) -> RepoState {
        let gd = self.git_dir();
        let common = self.common_dir();
        RepoState {
            branch: self.current_branch().ok().flatten(),
            head: self.head_id().ok().flatten(),
            merge_active: gd.join("MERGE_HEAD").exists(),
            rebase_active: gd.join("rebase-merge").exists()
                || gd.join("rebase-apply").exists()
                || gd.join("tgitrebase.active").exists(),
            cherry_pick_active: gd.join("CHERRY_PICK_HEAD").exists(),
            revert_active: gd.join("REVERT_HEAD").exists(),
            bisect_active: gd.join("BISECT_START").exists(),
            has_stash: self.has_stash(),
            has_submodules: self.workdir().join(".gitmodules").exists(),
            uses_lfs: common.join("lfs").exists()
                || std::fs::read_to_string(self.workdir().join(".gitattributes"))
                    .is_ok_and(|t| t.contains("filter=lfs")),
            is_git_svn: common.join("svn").exists(),
            remotes: self.remotes(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::repo::tests::fixture;

    #[test]
    fn detects_merge_and_stash() {
        let (dir, repo) = fixture();
        let s = repo.state();
        assert_eq!(s.branch.as_deref(), Some("main"));
        assert!(!s.busy() && !s.has_stash && !s.is_detached());
        let git = repo.git();
        git.run(["checkout", "-qb", "o"]).unwrap();
        std::fs::write(dir.path().join("a.txt"), "o\n").unwrap();
        git.run(["commit", "-qam", "o"]).unwrap();
        git.run(["checkout", "-q", "main"]).unwrap();
        std::fs::write(dir.path().join("a.txt"), "m\n").unwrap();
        git.run(["commit", "-qam", "m"]).unwrap();
        let _ = git.output(["merge", "o"]);
        assert!(repo.state().merge_active);
        git.run(["merge", "--abort"]).unwrap();
        std::fs::write(dir.path().join("a.txt"), "dirty\n").unwrap();
        git.run(["stash", "-q"]).unwrap();
        assert!(crate::Repo::discover(dir.path()).unwrap().state().has_stash);
    }
}
