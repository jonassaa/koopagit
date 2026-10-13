//! Command lines and parsers for the additional dialogs: File Diff, Export,
//! Format Patch, Apply Patches, worktrees, Submodule Add, Request pull and
//! the Repository Browser (see `docs/spec/spec-dialogs-2.md`).

use std::collections::HashMap;

use crate::status::Action;
use crate::{Repo, Result};

fn s(v: &str) -> String {
    v.to_owned()
}

/// The similarity threshold TortoiseGit passes to `-C`/`-M`
/// (`DiffSimilarityIndexThreshold`, default 50).
pub const SIMILARITY: u32 = 50;

/// Whether `rev` denotes the working tree (empty or all zeros).
pub fn is_working_tree(rev: &str) -> bool {
    rev.is_empty() || rev.bytes().all(|b| b == b'0')
}

// ------------------------------------------------------------ File Diff

/// "Diff Options" of the File Diff dialog.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DiffOptions {
    pub ignore_space_at_eol: bool,
    pub ignore_space_change: bool,
    pub ignore_all_space: bool,
    pub ignore_blank_lines: bool,
}

impl DiffOptions {
    pub fn any(&self) -> bool {
        self.ignore_space_at_eol
            || self.ignore_space_change
            || self.ignore_all_space
            || self.ignore_blank_lines
    }

    fn args(&self) -> Vec<String> {
        let mut v = Vec::new();
        if self.ignore_space_at_eol {
            v.push(s("--ignore-space-at-eol"));
        }
        if self.ignore_space_change {
            v.push(s("--ignore-space-change"));
        }
        if self.ignore_all_space {
            v.push(s("--ignore-all-space"));
        }
        if self.ignore_blank_lines {
            v.push(s("--ignore-blank-lines"));
        }
        v
    }
}

/// `CGit::GetCommitDiffList`: the changed files between `rev1` (base) and
/// `rev2`; either may be the working tree ([`is_working_tree`]).
pub fn commit_diff_args(rev1: &str, rev2: &str, o: &DiffOptions) -> Vec<String> {
    let similarity = [format!("-C{SIMILARITY}%"), format!("-M{SIMILARITY}%")];
    let mut a;
    if is_working_tree(rev1) || is_working_tree(rev2) {
        a = vec![s("diff"), s("-r")];
        if is_working_tree(rev1) {
            a.push(s("-R"));
        }
        a.push(s("--raw"));
        a.extend(similarity);
        a.extend([s("--numstat"), s("-z")]);
        a.extend(o.args());
        let rev = if is_working_tree(rev1) { rev2 } else { rev1 };
        a.extend([s("--end-of-options"), s(rev), s("--")]);
    } else {
        a = vec![s("diff-tree"), s("-r"), s("--raw")];
        a.extend(similarity);
        a.extend([s("--numstat"), s("-z")]);
        a.extend(o.args());
        a.extend([s("--end-of-options"), s(rev1), s(rev2), s("--")]);
    }
    a
}

/// One row of the File Diff list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedFile {
    pub path: String,
    pub old_path: Option<String>,
    pub action: Action,
    /// `None` for binary files (`-` in numstat).
    pub added: Option<u32>,
    pub removed: Option<u32>,
    /// A gitlink (submodule) entry.
    pub is_submodule: bool,
    /// Whether numstat reported the file at all; files with only ignored
    /// changes (with ignore options) are missing and hidden upstream.
    pub has_stat: bool,
}

fn raw_action(code: u8) -> Action {
    match code {
        b'A' => Action::Added,
        b'D' => Action::Deleted,
        b'R' => Action::Renamed,
        b'C' => Action::Copied,
        b'U' => Action::Unmerged,
        _ => Action::Modified,
    }
}

/// Parses `--raw --numstat -z` output (raw records first, then numstat).
pub fn parse_raw_numstat(out: &str) -> Vec<ChangedFile> {
    let mut files: Vec<ChangedFile> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut it = out.split('\0').peekable();
    while let Some(tok) = it.next() {
        if tok.is_empty() {
            continue;
        }
        let tok = tok.trim_start_matches('\n');
        if let Some(raw) = tok.strip_prefix(':') {
            // :<srcmode> <dstmode> <src> <dst> <status>
            let fields: Vec<&str> = raw.split(' ').collect();
            let status = fields.get(4).copied().unwrap_or("M");
            let code = status.bytes().next().unwrap_or(b'M');
            let dst_mode = fields.get(1).copied().unwrap_or("");
            let src_mode = fields.first().copied().unwrap_or("");
            let first = it.next().unwrap_or_default().to_owned();
            let (path, old) = if matches!(code, b'R' | b'C') {
                (it.next().unwrap_or_default().to_owned(), Some(first))
            } else {
                (first, None)
            };
            index.insert(path.clone(), files.len());
            files.push(ChangedFile {
                path,
                old_path: old,
                action: raw_action(code),
                added: None,
                removed: None,
                is_submodule: dst_mode == "160000" || src_mode == "160000",
                has_stat: false,
            });
        } else {
            let mut parts = tok.splitn(3, '\t');
            let (Some(a), Some(d), Some(p)) = (parts.next(), parts.next(), parts.next()) else {
                continue;
            };
            let path = if p.is_empty() {
                let _old = it.next();
                it.next().unwrap_or_default().to_owned()
            } else {
                p.to_owned()
            };
            if let Some(&i) = index.get(&path) {
                let f = &mut files[i];
                f.added = a.parse().ok();
                f.removed = d.parse().ok();
                f.has_stat = true;
            }
        }
    }
    files
}

impl Repo {
    /// The File Diff list between `rev1` (base) and `rev2`. With
    /// `common_ancestor`, `rev1` is replaced by `merge-base(rev1, rev2)`.
    pub fn changed_files(
        &self,
        rev1: &str,
        rev2: &str,
        o: &DiffOptions,
        common_ancestor: bool,
    ) -> Result<Vec<ChangedFile>> {
        let mut base = rev1.to_owned();
        if common_ancestor && !is_working_tree(rev1) && !is_working_tree(rev2) {
            if let Ok(mb) = self.git().run(["merge-base", rev1, rev2]) {
                let mb = mb.trim();
                if !mb.is_empty() {
                    base = mb.to_owned();
                }
            }
        }
        if is_working_tree(&base) || is_working_tree(rev2) {
            // Refresh stat info so unchanged files don't show up (RefreshGitIndex).
            let _ = self.git().output(["update-index", "-q", "--refresh"]);
        }
        let out = self.git().run(commit_diff_args(&base, rev2, o))?;
        let mut files = parse_raw_numstat(&out);
        if o.any() {
            files.retain(|f| f.is_submodule || f.has_stat);
        }
        Ok(files)
    }

    /// `<short hash>: <subject>` of a revision, as shown under each version box.
    pub fn rev_summary(&self, rev: &str) -> Option<(String, String, i64)> {
        if is_working_tree(rev) {
            return None;
        }
        let out = self
            .git()
            .run(["log", "-1", "--format=%h%x00%s%x00%ct", rev, "--"])
            .ok()?;
        let mut p = out.trim_end().split('\0');
        Some((
            p.next()?.to_owned(),
            p.next().unwrap_or_default().to_owned(),
            p.next().and_then(|t| t.parse().ok()).unwrap_or(0),
        ))
    }

    /// Resolves `rev` to a commit id, or `None` if it isn't a valid revision.
    pub fn resolve_commit(&self, rev: &str) -> Option<String> {
        self.git()
            .run([
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{rev}^{{commit}}"),
            ])
            .ok()
            .map(|o| o.trim().to_owned())
            .filter(|o| !o.is_empty())
    }
}

// ------------------------------------------------------------ Export

/// Adds `.zip` when the file name has no extension (`CExportDlg::OnOK`).
pub fn export_file_name(file: &str) -> String {
    let name = file.rsplit(['/', '\\']).next().unwrap_or(file);
    if name.contains('.') {
        file.to_owned()
    } else {
        format!("{file}.zip")
    }
}

/// `git archive --output=<file> --verbose --end-of-options <version>`.
pub fn export_args(file: &str, version: &str) -> Vec<String> {
    vec![
        s("archive"),
        format!("--output={file}"),
        s("--verbose"),
        s("--end-of-options"),
        s(version),
    ]
}

// ------------------------------------------------------------ Format Patch

/// The Version group of the Format Patch dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchRange {
    Since(String),
    Number(u32),
    Range(String, String),
}

/// `git format-patch[ --no-prefix] -o <dir> <range> --`.
pub fn format_patch_args(no_prefix: bool, dir: &str, range: &PatchRange) -> Vec<String> {
    let mut a = vec![s("format-patch")];
    if no_prefix {
        a.push(s("--no-prefix"));
    }
    a.extend([s("-o"), s(dir)]);
    match range {
        PatchRange::Since(r) => a.extend([s("--end-of-options"), r.clone()]),
        PatchRange::Number(n) => a.push(format!("-{n}")),
        PatchRange::Range(f, t) => a.extend([s("--end-of-options"), format!("{f}..{t}")]),
    }
    a.push(s("--"));
    a
}

// ------------------------------------------------------------ Apply Patches

/// Options of the Apply Patches dialog (defaults as upstream).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AmOptions {
    pub three_way: bool,
    pub ignore_space_change: bool,
    pub sign_off: bool,
    pub keep_cr: bool,
}

impl Default for AmOptions {
    fn default() -> Self {
        AmOptions {
            three_way: true,
            ignore_space_change: true,
            sign_off: false,
            keep_cr: true,
        }
    }
}

/// `git am [--signoff ][--3way ][--ignore-space-change ][--keep-cr ]-- <patch>`.
pub fn am_args(o: &AmOptions, patch: &str) -> Vec<String> {
    let mut a = vec![s("am")];
    if o.sign_off {
        a.push(s("--signoff"));
    }
    if o.three_way {
        a.push(s("--3way"));
    }
    if o.ignore_space_change {
        a.push(s("--ignore-space-change"));
    }
    if o.keep_cr {
        a.push(s("--keep-cr"));
    }
    a.extend([s("--"), s(patch)]);
    a
}

/// Answers to "previous rebase directory rebase-apply still exists".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmControl {
    Abort,
    Skip,
    Resolved,
}

pub fn am_control_args(c: AmControl) -> Vec<String> {
    let flag = match c {
        AmControl::Abort => "--abort",
        AmControl::Skip => "--skip",
        AmControl::Resolved => "--resolved",
    };
    vec![s("am"), s(flag)]
}

impl Repo {
    /// `CTGitPath::HasRebaseApply`: a `git am` / rebase-apply session is active.
    pub fn am_in_progress(&self) -> bool {
        self.git_dir().join("rebase-apply").exists()
    }
}

// ------------------------------------------------------------ Worktrees

/// Options of the New Worktree dialog.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorktreeAdd {
    pub path: String,
    /// Empty or `HEAD` means "current HEAD" (no commit-ish passed).
    pub version: String,
    pub checkout: bool,
    pub force: bool,
    pub detach: bool,
    pub new_branch: Option<String>,
}

/// `git worktree add[ --no-checkout][ --force][ --detach | -b <name>] -- <dir> [<version>]`.
pub fn worktree_add_args(o: &WorktreeAdd) -> Vec<String> {
    let mut a = vec![s("worktree"), s("add")];
    if !o.checkout {
        a.push(s("--no-checkout"));
    }
    if o.force {
        a.push(s("--force"));
    }
    if o.detach {
        a.push(s("--detach"));
    } else if let Some(b) = &o.new_branch {
        a.extend([s("-b"), b.clone()]);
    }
    a.extend([s("--"), o.path.clone()]);
    if !o.version.is_empty() && o.version != "HEAD" {
        a.push(o.version.clone());
    }
    a
}

/// `git worktree remove[ --force] -- <path>`.
pub fn worktree_remove_args(path: &str, force: bool) -> Vec<String> {
    let mut a = vec![s("worktree"), s("remove")];
    if force {
        a.push(s("--force"));
    }
    a.extend([s("--"), s(path)]);
    a
}

/// Default directory of a new worktree: `<repo>-worktree`, or the bare
/// repository path without `.git`.
pub fn default_worktree_dir(repo_dir: &str) -> String {
    let trimmed = repo_dir.trim_end_matches(['/', '\\']);
    match trimmed.strip_suffix(".git") {
        Some(base) if !base.is_empty() && !base.ends_with(['/', '\\']) => base.to_owned(),
        _ => format!("{trimmed}-worktree"),
    }
}

/// Target of `dropnewworktree`: `<target>/<repo name without .git>`, unless
/// that is the repository itself.
pub fn drop_worktree_dir(target: &str, repo_dir: &str) -> String {
    let name = repo_dir
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default();
    let name = name.strip_suffix(".git").unwrap_or(name);
    let sep = if target.contains('\\') && !target.contains('/') {
        '\\'
    } else {
        '/'
    };
    let mut t = target.trim_end_matches(['/', '\\']).to_owned();
    t.push(sep);
    let candidate = format!("{t}{name}");
    if same_path(&candidate, repo_dir) {
        t
    } else {
        candidate
    }
}

fn same_path(a: &str, b: &str) -> bool {
    let n = |p: &str| p.trim_end_matches(['/', '\\']).replace('\\', "/");
    n(a) == n(b)
}

/// New-branch proposal when the version changes (`OnVersionChanged`):
/// returns `(name, create_branch)`. `mode`: 0 HEAD, 1 Branch, 2 Tag, 3 Commit.
pub fn worktree_branch_proposal(mode: i32, version: &str, dir: &str) -> (String, bool) {
    let after = |prefix: &str| -> Option<String> {
        let rest = version.strip_prefix(prefix)?;
        let i = rest.find('/')?;
        Some(rest[i + 1..].to_owned())
    };
    let (name, mut branch) = if mode == 0 {
        let leaf = dir
            .trim_end_matches(['/', '\\'])
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or_default()
            .to_owned();
        (leaf, false)
    } else if let Some(rest) = version.strip_prefix("origin/") {
        (rest.to_owned(), true)
    } else if let Some(n) = after("remotes/") {
        (n, true)
    } else {
        let v = if let Some(h) = version.strip_prefix("refs/heads/") {
            h.to_owned()
        } else if mode == 3 {
            version.chars().take(8).collect()
        } else {
            version.to_owned()
        };
        (format!("Branch_{v}"), false)
    };
    if mode == 2 || mode == 3 {
        branch = true;
    }
    (name, branch)
}

/// One entry of `git worktree list --porcelain`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Worktree {
    pub path: String,
    pub head: String,
    /// Short branch name, or `detached HEAD` like upstream.
    pub branch: String,
    pub is_main: bool,
    pub bare: bool,
    pub locked: bool,
    pub lock_reason: String,
    pub prunable: bool,
}

pub fn parse_worktree_list(out: &str) -> Vec<Worktree> {
    let mut list = Vec::new();
    for (i, block) in out
        .split("\n\n")
        .filter(|b| !b.trim().is_empty())
        .enumerate()
    {
        let mut w = Worktree {
            is_main: i == 0,
            ..Default::default()
        };
        for line in block.lines() {
            let (k, v) = line.split_once(' ').unwrap_or((line, ""));
            match k {
                "worktree" => w.path = v.to_owned(),
                "HEAD" => w.head = v.to_owned(),
                "branch" => w.branch = v.strip_prefix("refs/heads/").unwrap_or(v).to_owned(),
                "detached" => w.branch = s("detached HEAD"),
                "bare" => w.bare = true,
                "locked" => {
                    w.locked = true;
                    w.lock_reason = v.to_owned();
                }
                "prunable" => w.prunable = true,
                _ => {}
            }
        }
        list.push(w);
    }
    list
}

impl Repo {
    pub fn worktrees(&self) -> Result<Vec<Worktree>> {
        Ok(parse_worktree_list(&self.git().run([
            "worktree",
            "list",
            "--porcelain",
        ])?))
    }
}

// ------------------------------------------------------------ Submodule Add / Request pull

/// `git submodule add[ -b <branch>| --force] -- <repo> <path>`. Upstream
/// quirk kept: `--force` replaces `-b <branch>` (`SubmoduleCommand.cpp:47-51`).
pub fn submodule_add_args(
    repo: &str,
    path: &str,
    branch: Option<&str>,
    force: bool,
) -> Vec<String> {
    let mut a = vec![s("submodule"), s("add")];
    if force {
        a.push(s("--force"));
    } else if let Some(b) = branch {
        a.extend([s("-b"), s(b)]);
    }
    a.extend([s("--"), s(repo), s(path)]);
    a
}

/// Path proposal after editing the URL: the repo name (without `.git`)
/// appended to the path the dialog started with (`OnRepoEndEdit`).
pub fn submodule_path_for_url(base_path: &str, url: &str) -> String {
    let name = url
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\', ':'])
        .next()
        .unwrap_or_default();
    let name = name.strip_suffix(".git").unwrap_or(name);
    if base_path.is_empty() {
        name.to_owned()
    } else {
        format!("{}/{name}", base_path.trim_end_matches(['/', '\\']))
    }
}

/// `git request-pull -- <start> <url> <end>`.
pub fn request_pull_args(start: &str, url: &str, end: &str) -> Vec<String> {
    vec![s("request-pull"), s("--"), s(start), s(url), s(end)]
}

// ------------------------------------------------------------ Repository Browser

/// Kind of a tree entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Folder,
    Submodule,
}

/// One entry of a tree at a revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    /// Full repository-relative path.
    pub path: String,
    pub name: String,
    pub kind: EntryKind,
    pub oid: String,
    pub size: Option<u64>,
    pub executable: bool,
    pub symlink: bool,
}

/// Parses `git ls-tree -r -t -l -z` output.
pub fn parse_ls_tree(out: &str) -> Vec<TreeEntry> {
    out.split('\0')
        .filter(|r| !r.is_empty())
        .filter_map(|rec| {
            let (meta, path) = rec.split_once('\t')?;
            let mut f = meta.split_whitespace();
            let mode = f.next()?;
            let kind = match f.next()? {
                "tree" => EntryKind::Folder,
                "commit" => EntryKind::Submodule,
                _ => EntryKind::File,
            };
            let oid = f.next()?.to_owned();
            let size = f.next().and_then(|v| v.parse().ok());
            Some(TreeEntry {
                name: path.rsplit('/').next().unwrap_or(path).to_owned(),
                path: path.to_owned(),
                kind,
                oid,
                size,
                executable: mode == "100755",
                symlink: mode == "120000",
            })
        })
        .collect()
}

impl Repo {
    /// The whole tree of `rev` (all levels), for the Repository Browser.
    pub fn tree_entries(&self, rev: &str) -> Result<Vec<TreeEntry>> {
        Ok(parse_ls_tree(&self.git().run([
            "ls-tree",
            "-r",
            "-t",
            "-l",
            "-z",
            "--full-tree",
            rev,
        ])?))
    }
}

/// Children of `dir` (`""` = root), folders first then by name, like the
/// browser's default sort.
pub fn children<'a>(entries: &'a [TreeEntry], dir: &str) -> Vec<&'a TreeEntry> {
    let mut v: Vec<&TreeEntry> = entries
        .iter()
        .filter(|e| match e.path.rsplit_once('/') {
            Some((parent, _)) => parent == dir,
            None => dir.is_empty(),
        })
        .collect();
    v.sort_by(|a, b| {
        (a.kind != EntryKind::Folder, a.name.to_lowercase())
            .cmp(&(b.kind != EntryKind::Folder, b.name.to_lowercase()))
    });
    v
}

/// `StrFormatByteSize64`-like size text (`12 bytes`, `1.5 KB`, ...).
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["bytes", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} bytes");
    }
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if v < 10.0 {
        format!("{v:.2} {}", UNITS[u])
    } else if v < 100.0 {
        format!("{v:.1} {}", UNITS[u])
    } else {
        format!("{v:.0} {}", UNITS[u])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::tests::fixture;

    fn line(v: &[String]) -> String {
        v.join(" ")
    }

    #[test]
    fn builders() {
        assert_eq!(export_file_name("/tmp/out"), "/tmp/out.zip");
        assert_eq!(export_file_name("/tmp/a.b/out.tar"), "/tmp/a.b/out.tar");
        assert_eq!(
            line(&export_args("/x.zip", "v1")),
            "archive --output=/x.zip --verbose --end-of-options v1"
        );
        assert_eq!(
            line(&format_patch_args(true, "/p", &PatchRange::Number(3))),
            "format-patch --no-prefix -o /p -3 --"
        );
        assert_eq!(
            line(&format_patch_args(
                false,
                "/p",
                &PatchRange::Range("a".into(), "b".into())
            )),
            "format-patch -o /p --end-of-options a..b --"
        );
        assert_eq!(
            line(&am_args(&AmOptions::default(), "x.patch")),
            "am --3way --ignore-space-change --keep-cr -- x.patch"
        );
        assert_eq!(line(&am_control_args(AmControl::Resolved)), "am --resolved");
        assert_eq!(
            line(&submodule_add_args("u", "p", Some("dev"), true)),
            "submodule add --force -- u p"
        );
        assert_eq!(
            line(&submodule_add_args("u", "p", Some("dev"), false)),
            "submodule add -b dev -- u p"
        );
        assert_eq!(
            submodule_path_for_url("libs", "https://h/x/foo.git"),
            "libs/foo"
        );
        assert_eq!(submodule_path_for_url("", "git@h:foo.git"), "foo");
        assert_eq!(
            line(&request_pull_args("a", "u", "HEAD")),
            "request-pull -- a u HEAD"
        );
        assert_eq!(format_size(12), "12 bytes");
        assert_eq!(format_size(1536), "1.50 KB");
    }

    #[test]
    fn worktree_helpers() {
        assert_eq!(default_worktree_dir("/r/proj"), "/r/proj-worktree");
        assert_eq!(default_worktree_dir("/r/proj.git"), "/r/proj");
        assert_eq!(drop_worktree_dir("/t/", "/r/proj.git"), "/t/proj");
        assert_eq!(drop_worktree_dir("/r", "/r/proj"), "/r/");
        assert_eq!(
            worktree_branch_proposal(1, "remotes/origin/dev", "/w"),
            ("dev".to_owned(), true)
        );
        assert_eq!(
            worktree_branch_proposal(0, "HEAD", "/a/b-wt"),
            ("b-wt".to_owned(), false)
        );
        assert_eq!(
            worktree_branch_proposal(3, "0123456789abcdef", "/w"),
            ("Branch_01234567".to_owned(), true)
        );
        let o = WorktreeAdd {
            path: "/w".into(),
            version: "HEAD".into(),
            checkout: false,
            force: true,
            detach: false,
            new_branch: Some("b".into()),
        };
        assert_eq!(
            line(&worktree_add_args(&o)),
            "worktree add --no-checkout --force -b b -- /w"
        );
        assert_eq!(
            line(&worktree_remove_args("/w", true)),
            "worktree remove --force -- /w"
        );
        let list = parse_worktree_list(
            "worktree /r\nHEAD abc\nbranch refs/heads/main\n\nworktree /w\nHEAD def\ndetached\nlocked moving\n\n",
        );
        assert_eq!(list.len(), 2);
        assert!(list[0].is_main && list[0].branch == "main");
        assert_eq!(list[1].branch, "detached HEAD");
        assert!(list[1].locked && list[1].lock_reason == "moving");
    }

    #[test]
    fn changed_files_between_revisions_and_worktree() {
        let (dir, repo) = fixture();
        let git = repo.git();
        std::fs::write(dir.path().join("b.txt"), "b\nb2\n").unwrap();
        std::fs::write(dir.path().join("a.txt"), "a\nmore\n").unwrap();
        git.run(["add", "."]).unwrap();
        git.run(["commit", "-qm", "two"]).unwrap();
        git.run(["mv", "b.txt", "c.txt"]).unwrap();
        git.run(["commit", "-qm", "rename"]).unwrap();

        let files = repo
            .changed_files("HEAD~2", "HEAD~1", &DiffOptions::default(), false)
            .unwrap();
        let a = files.iter().find(|f| f.path == "a.txt").unwrap();
        assert_eq!(
            (a.action, a.added, a.removed),
            (Action::Modified, Some(1), Some(0))
        );
        assert_eq!(
            files.iter().find(|f| f.path == "b.txt").unwrap().action,
            Action::Added
        );

        let renamed = repo
            .changed_files("HEAD~1", "HEAD", &DiffOptions::default(), false)
            .unwrap();
        assert_eq!(renamed.len(), 1);
        assert_eq!(renamed[0].action, Action::Renamed);
        assert_eq!(renamed[0].old_path.as_deref(), Some("b.txt"));
        assert_eq!(renamed[0].path, "c.txt");

        std::fs::write(dir.path().join("a.txt"), "a\nmore\n \n").unwrap();
        let wt = repo
            .changed_files("HEAD", "", &DiffOptions::default(), false)
            .unwrap();
        assert_eq!(wt.len(), 1);
        let ignoring = DiffOptions {
            ignore_all_space: true,
            ignore_blank_lines: true,
            ..Default::default()
        };
        assert!(
            repo.changed_files("HEAD", "", &ignoring, false)
                .unwrap()
                .is_empty()
        );
        let (short, subject, _) = repo.rev_summary("HEAD").unwrap();
        assert_eq!(subject, "rename");
        assert!(!short.is_empty());
        assert!(repo.resolve_commit("nope").is_none());
    }

    #[test]
    fn tree_listing() {
        let (dir, repo) = fixture();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        repo.git().run(["add", "."]).unwrap();
        repo.git().run(["commit", "-qm", "src"]).unwrap();
        let entries = repo.tree_entries("HEAD").unwrap();
        let root: Vec<&str> = children(&entries, "")
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(root, ["src", "a.txt"]);
        let src = children(&entries, "src");
        assert_eq!(src[0].path, "src/main.rs");
        assert_eq!(src[0].size, Some(13));
    }

    #[test]
    fn export_format_patch_am_and_worktree_run() {
        let (dir, repo) = fixture();
        let git = repo.git();
        let out = tempfile::tempdir().unwrap();
        let zip = out.path().join("x.zip");
        git.run(export_args(&zip.to_string_lossy(), "HEAD"))
            .unwrap();
        assert!(zip.exists());

        std::fs::write(dir.path().join("a.txt"), "patched\n").unwrap();
        git.run(["commit", "-qam", "patch me"]).unwrap();
        let pdir = out.path().join("patches");
        git.run(format_patch_args(
            false,
            &pdir.to_string_lossy(),
            &PatchRange::Number(1),
        ))
        .unwrap();
        let patch = std::fs::read_dir(&pdir)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        git.run(["reset", "-q", "--hard", "HEAD~1"]).unwrap();
        git.run(am_args(&AmOptions::default(), &patch.to_string_lossy()))
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "patched\n"
        );
        assert!(!repo.am_in_progress());

        let wt = out.path().join("wt");
        git.run(worktree_add_args(&WorktreeAdd {
            path: wt.to_string_lossy().into_owned(),
            version: String::new(),
            checkout: true,
            new_branch: Some("wtb".into()),
            ..Default::default()
        }))
        .unwrap();
        let list = repo.worktrees().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].branch, "wtb");
        git.run(worktree_remove_args(&wt.to_string_lossy(), false))
            .unwrap();
        assert_eq!(repo.worktrees().unwrap().len(), 1);
    }
}
