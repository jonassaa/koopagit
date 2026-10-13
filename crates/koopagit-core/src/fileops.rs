//! Command lines and helpers for TortoiseGit's file operations: Add, Revert,
//! Remove, Resolve, Clean up, Ignore, Rename, Create repository and Rebase.

use std::path::Path;

use crate::status::{Action, StatusEntry};
use crate::{Repo, Result};

fn s(v: &str) -> String {
    v.to_owned()
}

fn with_paths(prefix: &[&str], paths: &[String]) -> Vec<String> {
    let mut v: Vec<String> = prefix.iter().map(|a| s(a)).collect();
    v.push(s("--"));
    v.extend(paths.iter().cloned());
    v
}

/// Add (`AddProgressCommand`, CLI path): `git add -f -- <paths>`.
pub fn add(paths: &[String]) -> Vec<String> {
    with_paths(&["add", "-f"], paths)
}

/// Remove: one call per path, `git rm -r -f [--cached] -- <p>`.
pub fn remove(path: &str, keep_local: bool) -> Vec<String> {
    let mut a = vec![s("rm"), s("-r"), s("-f")];
    if keep_local {
        a.push(s("--cached"));
    }
    a.extend([s("--"), s(path)]);
    a
}

/// Resolve (mark as resolved): `git add -f -- <paths>`.
pub fn resolve(paths: &[String]) -> Vec<String> {
    with_paths(&["add", "-f"], paths)
}

/// Resolve a conflict using one side: `git checkout-index -f --stage=2|3`.
pub fn resolve_using(path: &str, theirs: bool) -> Vec<Vec<String>> {
    let stage = if theirs { "--stage=3" } else { "--stage=2" };
    vec![
        vec![s("checkout-index"), s("-f"), s(stage), s("--"), s(path)],
        add(&[s(path)]),
    ]
}

/// Rename: `git mv -- <old> <new>`.
pub fn rename(old: &str, new: &str) -> Vec<String> {
    vec![s("mv"), s("--"), s(old), s(new)]
}

/// Create repository: `git init [--bare] -- <folder>`.
pub fn init(folder: &str, bare: bool) -> Vec<String> {
    let mut a = vec![s("init")];
    if bare {
        a.push(s("--bare"));
    }
    a.extend([s("--"), s(folder)]);
    a
}

/// The command lines of `RevertProgressCommand`, in its order: unstage added
/// files, drop renamed paths, check out from HEAD, re-add deleted files.
pub fn revert(entries: &[StatusEntry]) -> Vec<Vec<String>> {
    let mut unstage = Vec::new();
    let mut delete = Vec::new();
    let mut checkout = Vec::new();
    let mut readd = Vec::new();
    for e in entries {
        match e.action() {
            Action::Added | Action::Copied => unstage.push(e.path.clone()),
            Action::Renamed => {
                delete.push(e.path.clone());
                if let Some(old) = &e.old_path {
                    checkout.push(old.clone());
                }
            }
            Action::Unversioned | Action::Ignored => {}
            Action::Deleted | Action::Missing => {
                checkout.push(e.path.clone());
                readd.push(e.path.clone());
            }
            _ => checkout.push(e.path.clone()),
        }
    }
    let mut cmds = Vec::new();
    if !unstage.is_empty() {
        cmds.push(with_paths(&["rm", "-f", "--cached"], &unstage));
    }
    if !delete.is_empty() {
        cmds.push(with_paths(&["rm", "-f", "--ignore-unmatch"], &delete));
    }
    if !checkout.is_empty() {
        cmds.push(with_paths(
            &["checkout", "-f", "--end-of-options", "HEAD"],
            &checkout,
        ));
    }
    if !readd.is_empty() {
        cmds.push(with_paths(&["add", "-f"], &readd));
    }
    cmds
}

/// Clean dialog "Clean Type" radios.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CleanType {
    /// `-fx`: all untracked files.
    #[default]
    All,
    /// `-f`: non-ignored untracked files.
    NonIgnored,
    /// `-fX`: ignored files only.
    Ignored,
}

impl CleanType {
    pub fn from_index(i: u32) -> Self {
        match i {
            1 => CleanType::NonIgnored,
            2 => CleanType::Ignored,
            _ => CleanType::All,
        }
    }

    pub fn index(self) -> u32 {
        match self {
            CleanType::All => 0,
            CleanType::NonIgnored => 1,
            CleanType::Ignored => 2,
        }
    }
}

/// Clean dialog options.
#[derive(Debug, Clone, Default)]
pub struct CleanOptions {
    pub kind: CleanType,
    /// `-d`: remove untracked directories.
    pub directories: bool,
    /// Extra `-f`: also directories containing a `.git`.
    pub nested_repos: bool,
    /// `-n`.
    pub dry_run: bool,
    /// Pathspec (repository-relative directory); `None` for the root.
    pub path: Option<String>,
}

/// `git clean [-n] [-d] (-fx|-f|-fX) [-f] [-- <dir>]`.
pub fn clean(o: &CleanOptions) -> Vec<String> {
    let mut a = vec![s("clean")];
    if o.dry_run {
        a.push(s("-n"));
    }
    if o.directories {
        a.push(s("-d"));
    }
    a.push(s(match o.kind {
        CleanType::All => "-fx",
        CleanType::NonIgnored => "-f",
        CleanType::Ignored => "-fX",
    }));
    if o.directories && o.nested_repos {
        a.push(s("-f"));
    }
    if let Some(p) = o.path.as_deref().filter(|p| !p.is_empty()) {
        a.extend([s("--"), s(p)]);
    }
    a
}

/// Ignore dialog "Ignore Type".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IgnoreType {
    /// Only in the containing folder (anchored pattern).
    #[default]
    InFolder,
    Recursive,
}

/// Ignore dialog "Ignore File".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IgnoreFile {
    /// `.gitignore` in the repository root (the effective upstream default).
    #[default]
    Root,
    /// `.gitignore` in the containing directory of each item.
    Containing,
    /// `.git/info/exclude`.
    InfoExclude,
}

/// One pattern to append to one ignore file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoreEntry {
    /// The ignore file, relative to the work tree root (`.git/info/exclude`
    /// is relative to the git dir and returned as such).
    pub file: String,
    pub pattern: String,
}

/// Builds the pattern for `rel` (repository-relative, `/`-separated) like
/// `CAppUtils::IgnoreFile`. Returns `None` for `only_mask` on a name without
/// an extension.
pub fn ignore_entry(
    rel: &str,
    is_dir: bool,
    kind: IgnoreType,
    file: IgnoreFile,
    only_mask: bool,
) -> Option<IgnoreEntry> {
    let (dir, name) = match rel.rsplit_once('/') {
        Some((d, n)) => (d, n),
        None => ("", rel),
    };
    let body = if only_mask {
        if is_dir {
            return None;
        }
        match name.rfind('.') {
            Some(i) if i > 0 => format!("*{}", &name[i..]),
            _ => return None,
        }
    } else {
        name.to_owned()
    };
    let body = body.replace('[', "\\[").replace(']', "\\]");
    let prefix = match (kind, file) {
        (IgnoreType::Recursive, _) => String::new(),
        (IgnoreType::InFolder, IgnoreFile::Containing) => "/".to_owned(),
        (IgnoreType::InFolder, _) if dir.is_empty() => "/".to_owned(),
        (IgnoreType::InFolder, _) => format!("/{dir}/"),
    };
    let target = match file {
        IgnoreFile::Root => ".gitignore".to_owned(),
        IgnoreFile::Containing if dir.is_empty() => ".gitignore".to_owned(),
        IgnoreFile::Containing => format!("{dir}/.gitignore"),
        IgnoreFile::InfoExclude => "info/exclude".to_owned(),
    };
    Some(IgnoreEntry {
        file: target,
        pattern: format!("{prefix}{body}"),
    })
}

/// Appends `pattern` to the ignore file unless already present, keeping the
/// file's line endings and adding a newline first if the file lacks one.
pub fn append_ignore(file: &Path, pattern: &str) -> std::io::Result<bool> {
    let existing = match std::fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    if existing.lines().any(|l| l.trim_end() == pattern) {
        return Ok(false);
    }
    let eol = if existing.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut out = existing.clone();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push_str(eol);
    }
    out.push_str(pattern);
    out.push_str(eol);
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(file, out)?;
    Ok(true)
}

impl Repo {
    /// Appends an [`IgnoreEntry`] to its file (resolving `info/exclude` in the git dir).
    pub fn apply_ignore(&self, entry: &IgnoreEntry) -> std::io::Result<bool> {
        let path = if entry.file == "info/exclude" {
            self.common_dir().join("info").join("exclude")
        } else {
            self.workdir().join(&entry.file)
        };
        append_ignore(&path, &entry.pattern)
    }

    /// Commits of `upstream..branch` oldest first, as `(id, subject)`
    /// (the Rebase dialog's commit list; merges excluded).
    pub fn rebase_commits(&self, upstream: &str, branch: &str) -> Result<Vec<(String, String)>> {
        let out = self.git().run([
            "rev-list",
            "--reverse",
            "--topo-order",
            "--no-merges",
            "--format=%H%x00%s",
            &format!("{upstream}..{branch}"),
            "--",
        ])?;
        Ok(out
            .lines()
            .filter(|l| !l.starts_with("commit "))
            .filter_map(|l| l.split_once('\0'))
            .map(|(id, subj)| (id.to_owned(), subj.to_owned()))
            .collect())
    }

    /// Whether `ancestor` is an ancestor of `descendant`.
    pub fn is_ancestor(&self, ancestor: &str, descendant: &str) -> bool {
        self.git()
            .output(["merge-base", "--is-ancestor", ancestor, descendant])
            .is_ok_and(|o| o.success())
    }

    /// Resolves a revision to a full commit id.
    pub fn rev_parse(&self, rev: &str) -> Result<String> {
        Ok(self
            .git()
            .run([
                "rev-parse",
                "--verify",
                "--end-of-options",
                &format!("{rev}^{{commit}}"),
            ])?
            .trim()
            .to_owned())
    }
}

/// One row of the Rebase dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebaseAction {
    Pick,
    Squash,
    Edit,
    Skip,
}

/// Plans the cherry-pick sequence of TortoiseGit's rebase (`DoRebase`): picks
/// become `cherry-pick --ff <id>`, squash/edit use `--no-commit`; a squash is
/// folded into the previous commit with `commit --amend`.
pub fn rebase_steps(rows: &[(String, RebaseAction)]) -> Vec<Vec<String>> {
    let mut cmds = Vec::new();
    for (id, action) in rows {
        match action {
            RebaseAction::Skip => {}
            RebaseAction::Pick | RebaseAction::Edit => {
                cmds.push(vec![
                    s("cherry-pick"),
                    s("--ff"),
                    s("--allow-empty"),
                    id.clone(),
                ]);
            }
            RebaseAction::Squash => {
                cmds.push(vec![s("cherry-pick"), s("--no-commit"), id.clone()]);
                cmds.push(vec![
                    s("commit"),
                    s("--amend"),
                    s("--no-edit"),
                    s("--allow-empty"),
                ]);
            }
        }
    }
    cmds
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::tests::fixture;

    fn entry(path: &str, staged: Option<Action>, unstaged: Option<Action>) -> StatusEntry {
        StatusEntry {
            path: path.into(),
            old_path: None,
            staged,
            unstaged,
        }
    }

    #[test]
    fn simple_builders() {
        assert_eq!(remove("a b", true).join(" "), "rm -r -f --cached -- a b");
        assert_eq!(add(&[s("x")]).join(" "), "add -f -- x");
        assert_eq!(rename("a", "b").join(" "), "mv -- a b");
        assert_eq!(init("/x.git", true).join(" "), "init --bare -- /x.git");
        let c = CleanOptions {
            kind: CleanType::All,
            directories: true,
            nested_repos: true,
            dry_run: true,
            path: Some(s("sub")),
        };
        assert_eq!(clean(&c).join(" "), "clean -n -d -fx -f -- sub");
        let c = CleanOptions {
            kind: CleanType::Ignored,
            ..Default::default()
        };
        assert_eq!(clean(&c).join(" "), "clean -fX");
    }

    #[test]
    fn revert_order() {
        let entries = [
            entry("new.txt", Some(Action::Added), None),
            entry("mod.txt", None, Some(Action::Modified)),
            entry("gone.txt", Some(Action::Deleted), None),
        ];
        let lines: Vec<String> = revert(&entries).iter().map(|c| c.join(" ")).collect();
        assert_eq!(
            lines,
            [
                "rm -f --cached -- new.txt",
                "checkout -f --end-of-options HEAD -- mod.txt gone.txt",
                "add -f -- gone.txt",
            ]
        );
    }

    #[test]
    fn ignore_patterns() {
        let e = |rel, kind, file, mask| ignore_entry(rel, false, kind, file, mask);
        assert_eq!(
            e("src/a.log", IgnoreType::InFolder, IgnoreFile::Root, false),
            Some(IgnoreEntry {
                file: s(".gitignore"),
                pattern: s("/src/a.log")
            })
        );
        assert_eq!(
            e(
                "src/a.log",
                IgnoreType::InFolder,
                IgnoreFile::Containing,
                true
            ),
            Some(IgnoreEntry {
                file: s("src/.gitignore"),
                pattern: s("/*.log")
            })
        );
        assert_eq!(
            e(
                "x[1].txt",
                IgnoreType::Recursive,
                IgnoreFile::InfoExclude,
                false
            )
            .unwrap()
            .pattern,
            "x\\[1\\].txt"
        );
        assert_eq!(
            e("Makefile", IgnoreType::Recursive, IgnoreFile::Root, true),
            None
        );
    }

    #[test]
    fn append_keeps_eol_and_dedups() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join(".gitignore");
        std::fs::write(&f, "a\r\nb").unwrap();
        assert!(append_ignore(&f, "c").unwrap());
        assert!(!append_ignore(&f, "a").unwrap());
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "a\r\nb\r\nc\r\n");
    }

    #[test]
    fn rebase_engine_replays_commits() {
        let (dir, repo) = fixture();
        let git = repo.git();
        git.run(["checkout", "-qb", "topic"]).unwrap();
        for (f, m) in [("t1", "one"), ("t2", "two"), ("t3", "three")] {
            std::fs::write(dir.path().join(f), m).unwrap();
            git.run(["add", f]).unwrap();
            git.run(["commit", "-qm", m]).unwrap();
        }
        git.run(["checkout", "-q", "main"]).unwrap();
        std::fs::write(dir.path().join("m"), "m").unwrap();
        git.run(["add", "m"]).unwrap();
        git.run(["commit", "-qm", "main work"]).unwrap();

        let commits = repo.rebase_commits("main", "topic").unwrap();
        assert_eq!(
            commits.iter().map(|c| c.1.as_str()).collect::<Vec<_>>(),
            ["one", "two", "three"]
        );
        assert!(!repo.is_ancestor("topic", "main"));
        let rows = vec![
            (commits[0].0.clone(), RebaseAction::Pick),
            (commits[1].0.clone(), RebaseAction::Squash),
            (commits[2].0.clone(), RebaseAction::Skip),
        ];
        git.run(["checkout", "-qf", "main"]).unwrap();
        git.run(["checkout", "-q", "--detach"]).unwrap();
        for c in rebase_steps(&rows) {
            git.run(c).unwrap();
        }
        let log = git.run(["log", "--format=%s", "-3"]).unwrap();
        assert_eq!(
            log.lines().collect::<Vec<_>>(),
            ["one", "main work", "initial"]
        );
        assert!(dir.path().join("t2").exists());
        assert!(!dir.path().join("t3").exists());
    }
}
