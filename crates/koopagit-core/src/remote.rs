//! Remote-related queries and command lines used by the Push, Pull/Fetch,
//! Sync and Clone dialogs (`CPushDlg`, `CPullFetchDlg`, `CSyncDlg`, `CCloneDlg`).

use crate::{Repo, Result};

/// One row of the Sync dialog's "In Commits" / "Out Commits" lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitLine {
    pub id: String,
    pub subject: String,
    pub author: String,
    /// Author time, seconds since the epoch.
    pub time: i64,
}

impl Repo {
    /// `git config --get <key>`, `None` when unset or empty.
    pub fn config_get(&self, key: &str) -> Option<String> {
        self.git()
            .run(["config", "--get", key])
            .ok()
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    }

    /// Writes `key` to the local `.git/config`; an empty value unsets it.
    pub fn config_set_local(&self, key: &str, value: &str) -> Result<()> {
        if value.is_empty() {
            // Unsetting a missing key exits 5; that is not an error here.
            self.git().output(["config", "--local", "--unset", key])?;
        } else {
            self.git().run(["config", "--local", key, value])?;
        }
        Ok(())
    }

    /// The tracked `(remote, branch)` of a local branch
    /// (`branch.<b>.remote` / `branch.<b>.merge`, `refs/heads/` stripped).
    pub fn tracking_branch(&self, local: &str) -> Option<(String, String)> {
        let remote = self.config_get(&format!("branch.{local}.remote"))?;
        let merge = self.config_get(&format!("branch.{local}.merge"))?;
        let merge = merge
            .strip_prefix("refs/heads/")
            .unwrap_or(&merge)
            .to_owned();
        Some((remote, merge))
    }

    /// `GetRemotePushBranch`: `branch.<b>.pushremote`/`pushbranch`, falling back
    /// to the tracked remote/branch. Either part may be empty.
    pub fn push_target(&self, local: &str) -> (String, String) {
        let tracked = self.tracking_branch(local);
        let remote = self
            .config_get(&format!("branch.{local}.pushremote"))
            .or_else(|| self.config_get("remote.pushdefault"))
            .or_else(|| tracked.as_ref().map(|t| t.0.clone()))
            .unwrap_or_default();
        let branch = self
            .config_get(&format!("branch.{local}.pushbranch"))
            .or_else(|| tracked.map(|t| t.1))
            .unwrap_or_default();
        (remote, branch)
    }

    /// Whether the repository is shallow (the Pull/Fetch "Depth" option is shown).
    pub fn is_shallow(&self) -> bool {
        self.common_dir().join("shallow").exists()
    }

    /// Whether `refs/remotes/<remote>/<branch>` exists.
    pub fn has_remote_tracking_ref(&self, remote: &str, branch: &str) -> bool {
        self.gix()
            .try_find_reference(format!("refs/remotes/{remote}/{branch}").as_str())
            .ok()
            .flatten()
            .is_some()
    }

    /// Whether two revisions share a merge base (pull offers "Merge unrelated
    /// histories" when they do not).
    pub fn has_merge_base(&self, a: &str, b: &str) -> bool {
        self.git()
            .output(["merge-base", "--end-of-options", a, b])
            .is_ok_and(|o| o.success())
    }

    /// Commits reachable from `include` but not from `exclude`, newest first
    /// (`git log exclude..include`). Missing refs yield an empty list.
    pub fn commits_between(&self, exclude: &str, include: &str) -> Vec<CommitLine> {
        let range = format!("{exclude}..{include}");
        let Ok(out) =
            self.git()
                .run(["log", "-z", "--format=%H%x1f%s%x1f%an%x1f%at", &range, "--"])
        else {
            return Vec::new();
        };
        out.split('\0')
            .filter(|r| !r.trim().is_empty())
            .filter_map(|r| {
                let mut f = r.trim_start_matches('\n').split('\x1f');
                Some(CommitLine {
                    id: f.next()?.to_owned(),
                    subject: f.next()?.to_owned(),
                    author: f.next()?.to_owned(),
                    time: f.next()?.trim().parse().unwrap_or(0),
                })
            })
            .collect()
    }
}

/// The remote combo's "- all -" entry (`IDS_PROC_PUSHFETCH_ALLREMOTES`).
pub const ALL_REMOTES: &str = "- all -";

/// Entries of the Push/Fetch remote combo: "- all -" first when there is more
/// than one remote.
pub fn remote_choices(remotes: &[String]) -> Vec<String> {
    let mut v = Vec::new();
    if remotes.len() > 1 {
        v.push(ALL_REMOTES.to_owned());
    }
    v.extend(remotes.iter().cloned());
    v
}

/// Whether push output says the remote rejected the update (offers Pull/Fetch).
pub fn push_rejected(output: &str) -> bool {
    output.contains("! [rejected]")
}

/// The module name the Clone dialog appends to the target directory: the last
/// URL path segment with a trailing `.git` removed (`CCloneDlg::OnCbnEditchangeUrlcombo`).
pub fn clone_module_name(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches(['/', '\\']);
    let last = trimmed.rsplit(['/', '\\', ':']).next().unwrap_or(trimmed);
    let last = last.rsplit('@').next().unwrap_or(last);
    let lower = last.to_lowercase();
    if lower.ends_with(".git") && last.len() > 4 {
        last[..last.len() - 4].to_owned()
    } else {
        last.to_owned()
    }
}

/// Replaces the previous module suffix of `dir` with the module of the new URL.
pub fn clone_target_dir(dir: &str, previous_module: &str, url: &str) -> String {
    let module = clone_module_name(url);
    let sep = if dir.contains('\\') && !dir.contains('/') {
        '\\'
    } else {
        std::path::MAIN_SEPARATOR
    };
    let base = if !previous_module.is_empty() {
        dir.strip_suffix(previous_module)
            .map(|b| b.trim_end_matches(['/', '\\']))
            .unwrap_or(dir)
    } else {
        dir
    };
    let base = base.trim_end_matches(['/', '\\']);
    if module.is_empty() {
        base.to_owned()
    } else if base.is_empty() {
        module
    } else {
        format!("{base}{sep}{module}")
    }
}

fn s(v: &str) -> String {
    v.to_owned()
}

/// The Sync dialog's Pull split-button entries (`CSyncDlg::OnBnClickedButtonPull`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncPull {
    Pull,
    Fetch,
    FetchRebase,
    FetchAllRefs,
    RemoteUpdate,
    CleanupStale,
}

impl SyncPull {
    pub const ALL: [SyncPull; 6] = [
        SyncPull::Pull,
        SyncPull::Fetch,
        SyncPull::FetchRebase,
        SyncPull::FetchAllRefs,
        SyncPull::RemoteUpdate,
        SyncPull::CleanupStale,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SyncPull::Pull => "Pull",
            SyncPull::Fetch => "Fetch",
            SyncPull::FetchRebase => "Fetch && Rebase",
            SyncPull::FetchAllRefs => "Fetch all refs",
            SyncPull::RemoteUpdate => "Remote Update",
            SyncPull::CleanupStale => "Clean up stale remote branches",
        }
    }
}

/// Inputs of a Sync dialog pull/push.
#[derive(Debug, Clone, Default)]
pub struct SyncOptions {
    /// Remote name or URL.
    pub url: String,
    pub local_branch: String,
    pub remote_branch: String,
    pub force: bool,
    /// `FetchVerbose` (default on).
    pub verbose: bool,
    /// Whether `url` is a configured remote name.
    pub is_named_remote: bool,
    /// Whether `refs/remotes/<url>/<remote_branch>` exists.
    pub tracking_ref_exists: bool,
}

/// Sync dialog pull-side command line.
pub fn sync_pull(kind: SyncPull, o: &SyncOptions) -> Vec<String> {
    match kind {
        SyncPull::Pull => {
            let mut a = vec![s("pull"), s("-v"), s("--progress")];
            if o.force {
                a.push(s("--force"));
            }
            a.extend([s("--"), o.url.clone()]);
            if !o.remote_branch.is_empty() {
                a.push(o.remote_branch.clone());
            }
            a
        }
        SyncPull::Fetch | SyncPull::FetchRebase | SyncPull::FetchAllRefs => {
            let refspec = if kind == SyncPull::FetchAllRefs || o.remote_branch.is_empty() {
                String::new()
            } else if !o.is_named_remote || !o.tracking_ref_exists {
                o.remote_branch.clone()
            } else {
                format!("{rb}:remotes/{url}/{rb}", rb = o.remote_branch, url = o.url)
            };
            let mut a = vec![s("fetch"), s("--progress")];
            if o.verbose {
                a.push(s("-v"));
            }
            if o.force {
                a.push(s("--force"));
            }
            a.extend([s("--"), o.url.clone()]);
            if !refspec.is_empty() {
                a.push(refspec);
            }
            a
        }
        SyncPull::RemoteUpdate => vec![s("remote"), s("update")],
        SyncPull::CleanupStale => vec![s("remote"), s("prune"), s("--"), o.url.clone()],
    }
}

/// Sync dialog push (`tags`: the "Push tags" entry).
pub fn sync_push(o: &SyncOptions, tags: bool) -> Vec<String> {
    let mut a = vec![s("push"), s("-v"), s("--progress")];
    if tags {
        a.push(s("--tags"));
    }
    if o.force {
        a.push(s("--force"));
    }
    a.extend([s("--"), o.url.clone()]);
    let mut refspec = o.local_branch.clone();
    if !o.remote_branch.is_empty() {
        refspec.push(':');
        refspec.push_str(&o.remote_branch);
    }
    a.push(refspec);
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{self, CloneOptions, PullFetchOptions, PushOptions, TriState};
    use crate::git_cli::Git;

    #[test]
    fn module_names_and_dirs() {
        assert_eq!(
            clone_module_name("https://github.com/x/koopagit.git"),
            "koopagit"
        );
        assert_eq!(clone_module_name("git@host:team/Repo.GIT"), "Repo");
        assert_eq!(clone_module_name("ssh://h/path/repo/"), "repo");
        assert_eq!(clone_module_name(r"\\server\share\proj"), "proj");
        assert_eq!(clone_module_name("host:proj"), "proj");
        let d = clone_target_dir("/home/u/src", "", "https://h/a.git");
        assert_eq!(d, format!("/home/u/src{}a", std::path::MAIN_SEPARATOR));
        assert_eq!(
            clone_target_dir(&d, "a", "https://h/b.git"),
            d.replace("/a", "/b").replace("\\a", "\\b")
        );
        assert_eq!(clone_target_dir(r"C:\src\a", "a", "x/b"), r"C:\src\b");
        assert_eq!(remote_choices(&[s("a")]), ["a"]);
        assert_eq!(remote_choices(&[s("a"), s("b")]), [ALL_REMOTES, "a", "b"]);
        assert!(push_rejected(
            " ! [rejected]        main -> main (fetch first)"
        ));
    }

    #[test]
    fn sync_lines() {
        let o = SyncOptions {
            url: s("origin"),
            local_branch: s("main"),
            remote_branch: s("main"),
            force: true,
            verbose: true,
            is_named_remote: true,
            tracking_ref_exists: true,
        };
        assert_eq!(
            sync_pull(SyncPull::Pull, &o).join(" "),
            "pull -v --progress --force -- origin main"
        );
        assert_eq!(
            sync_pull(SyncPull::Fetch, &o).join(" "),
            "fetch --progress -v --force -- origin main:remotes/origin/main"
        );
        assert_eq!(
            sync_pull(SyncPull::FetchAllRefs, &o).join(" "),
            "fetch --progress -v --force -- origin"
        );
        assert_eq!(
            sync_pull(SyncPull::CleanupStale, &o).join(" "),
            "remote prune -- origin"
        );
        assert_eq!(
            sync_push(&o, true).join(" "),
            "push -v --progress --tags --force -- origin main:main"
        );
    }

    /// A bare "remote" with one commit, and a clone of it.
    fn remote_and_clone() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let bare = dir.path().join("remote.git");
        let seed = dir.path().join("seed");
        Git::new(dir.path())
            .run(["init", "-q", "--bare", "-b", "main", bare.to_str().unwrap()])
            .unwrap();
        std::fs::create_dir(&seed).unwrap();
        let g = Git::new(&seed);
        g.run(["init", "-q", "-b", "main"]).unwrap();
        for (k, v) in [
            ("user.name", "T"),
            ("user.email", "t@e"),
            ("commit.gpgsign", "false"),
        ] {
            g.run(["config", k, v]).unwrap();
        }
        std::fs::write(seed.join("a.txt"), "a\n").unwrap();
        g.run(["add", "."]).unwrap();
        g.run(["commit", "-qm", "init"]).unwrap();
        g.run(["push", "-q", bare.to_str().unwrap(), "main"])
            .unwrap();

        let work = dir.path().join("work");
        let clone = commands::clone(&CloneOptions {
            url: bare.to_string_lossy().into_owned(),
            directory: work.to_string_lossy().into_owned(),
            ..Default::default()
        });
        Git::new(dir.path()).run(&clone).unwrap();
        let w = Git::new(&work);
        for (k, v) in [
            ("user.name", "T"),
            ("user.email", "t@e"),
            ("commit.gpgsign", "false"),
        ] {
            w.run(["config", k, v]).unwrap();
        }
        (dir, seed, work)
    }

    #[test]
    fn push_fetch_pull_against_local_bare_repo() {
        let (_dir, seed, work) = remote_and_clone();
        let repo = Repo::discover(&work).unwrap();
        assert_eq!(repo.tracking_branch("main"), Some((s("origin"), s("main"))));
        assert_eq!(repo.push_target("main"), (s("origin"), s("main")));
        assert!(!repo.is_shallow());

        // Push a local commit.
        std::fs::write(work.join("b.txt"), "b\n").unwrap();
        let w = repo.git();
        w.run(["add", "."]).unwrap();
        w.run(["commit", "-qm", "b"]).unwrap();
        assert_eq!(repo.commits_between("origin/main", "main").len(), 1);
        let push = commands::push(&PushOptions {
            remote: s("origin"),
            local: s("main"),
            remote_branch: s("main"),
            ..Default::default()
        });
        w.run(&push[0]).unwrap();

        // A commit made elsewhere is rejected on push, then fetched and pulled.
        let sg = Git::new(&seed);
        sg.run(["pull", "-q", "--no-rebase", "../remote.git", "main"])
            .unwrap();
        std::fs::write(seed.join("c.txt"), "c\n").unwrap();
        sg.run(["add", "."]).unwrap();
        sg.run(["commit", "-qm", "c"]).unwrap();
        sg.run(["push", "-q", "../remote.git", "main"]).unwrap();

        std::fs::write(work.join("d.txt"), "d\n").unwrap();
        w.run(["add", "."]).unwrap();
        w.run(["commit", "-qm", "d"]).unwrap();
        let out = w.output(&push[0]).unwrap();
        assert!(!out.success());
        assert!(push_rejected(&out.stderr), "{}", out.stderr);

        let fetch = commands::fetch(&PullFetchOptions {
            remote: s("origin"),
            prune: TriState::On,
            verbose: true,
            ..Default::default()
        });
        w.run(&fetch).unwrap();
        assert_eq!(repo.commits_between("main", "origin/main").len(), 1);
        assert!(repo.has_merge_base("main", "origin/main"));

        let pull = commands::pull(&PullFetchOptions {
            remote: s("origin"),
            remote_branch: s("main"),
            ..Default::default()
        });
        w.run(&pull).unwrap();
        assert!(work.join("c.txt").exists());
        assert!(repo.commits_between("main", "origin/main").is_empty());

        // Sync dialog fetch with an explicit tracking refspec.
        let sync = sync_pull(
            SyncPull::Fetch,
            &SyncOptions {
                url: s("origin"),
                remote_branch: s("main"),
                verbose: true,
                is_named_remote: true,
                tracking_ref_exists: repo.has_remote_tracking_ref("origin", "main"),
                ..Default::default()
            },
        );
        w.run(&sync).unwrap();

        repo.config_set_local("branch.main.pushbranch", "other")
            .unwrap();
        assert_eq!(repo.push_target("main").1, "other");
        repo.config_set_local("branch.main.pushbranch", "").unwrap();
        assert_eq!(repo.push_target("main").1, "main");
    }
}
