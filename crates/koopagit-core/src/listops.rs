//! Git command lines behind the file-list context menu
//! (`CGitStatusListCtrl` actions, spec-menus §1.6).

use crate::Result;
use crate::git_cli::Git;

fn s(v: &str) -> String {
    v.to_owned()
}

fn with_paths(prefix: &[&str], paths: &[String]) -> Vec<String> {
    let mut v: Vec<String> = prefix.iter().map(|a| s(a)).collect();
    v.push(s("--"));
    v.extend(paths.iter().cloned());
    v
}

/// Index flags toggled by "Skip worktree" / "Assume Unchanged" / "Unflag ...".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexFlag {
    SkipWorktree,
    AssumeUnchanged,
}

/// `git update-index --[no-]skip-worktree|--[no-]assume-unchanged -- <paths>`.
pub fn set_index_flag(flag: IndexFlag, on: bool, paths: &[String]) -> Vec<String> {
    let opt = match (flag, on) {
        (IndexFlag::SkipWorktree, true) => "--skip-worktree",
        (IndexFlag::SkipWorktree, false) => "--no-skip-worktree",
        (IndexFlag::AssumeUnchanged, true) => "--assume-unchanged",
        (IndexFlag::AssumeUnchanged, false) => "--no-assume-unchanged",
    };
    with_paths(&["update-index", opt], paths)
}

/// "Unflag as skip-worktree and assume-unchanged": both flags cleared.
pub fn clear_index_flags(paths: &[String]) -> Vec<Vec<String>> {
    vec![
        set_index_flag(IndexFlag::SkipWorktree, false, paths),
        set_index_flag(IndexFlag::AssumeUnchanged, false, paths),
    ]
}

/// "Add as Executable (+x)": `git add -f --chmod=+x -- <paths>`.
pub fn add_executable(paths: &[String]) -> Vec<String> {
    with_paths(&["add", "-f", "--chmod=+x"], paths)
}

/// "Add as Symlink": stores each file's content as a symlink target (mode
/// 120000). Hashes the files now and returns the index updates to run.
pub fn add_as_symlink(git: &Git, paths: &[String]) -> Result<Vec<Vec<String>>> {
    let mut cmds = Vec::new();
    for p in paths {
        let blob = git.run(["hash-object", "-w", "--", p])?.trim().to_owned();
        cmds.push(vec![
            s("update-index"),
            s("--add"),
            s("--cacheinfo"),
            format!("120000,{blob},{p}"),
        ]);
    }
    Ok(cmds)
}

/// "Revert to this revision": `git checkout <rev> -- <paths>`.
pub fn checkout_from(rev: &str, paths: &[String]) -> Vec<String> {
    let mut v = vec![s("checkout"), s(rev)];
    v.push(s("--"));
    v.extend(paths.iter().cloned());
    v
}

/// Drops paths from the index if present (an added file reverted to its parent).
pub fn unstage_if_present(paths: &[String]) -> Vec<String> {
    with_paths(&["rm", "--cached", "--ignore-unmatch", "-q"], paths)
}

/// "LFS Lock" / "LFS Unlock": one command per path.
pub fn lfs_lock(lock: bool, paths: &[String]) -> Vec<Vec<String>> {
    paths
        .iter()
        .map(|p| vec![s("lfs"), s(if lock { "lock" } else { "unlock" }), p.clone()])
        .collect()
}

/// Resolve a conflict with one side: `git checkout --theirs|--ours -- <p>`,
/// then `git add -- <p>` (ResolveProgressCommand).
pub fn resolve_side(theirs: bool, paths: &[String]) -> Vec<Vec<String>> {
    vec![
        with_paths(
            &["checkout", if theirs { "--theirs" } else { "--ours" }],
            paths,
        ),
        with_paths(&["add"], paths),
    ]
}

/// The contents of `path` at `rev` (`git show <rev>:<path>`).
pub fn blob_at(git: &Git, rev: &str, path: &str) -> Result<Vec<u8>> {
    let out = git.command(["show", &format!("{rev}:{path}")]).output()?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(crate::Error::GitFailed {
            args: format!("show {rev}:{path}"),
            code: out.status.code(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::tests::fixture;

    #[test]
    fn index_flags_round_trip() {
        let (_dir, repo) = fixture();
        let git = repo.git();
        let p = vec![s("a.txt")];
        git.run(set_index_flag(IndexFlag::SkipWorktree, true, &p))
            .unwrap();
        git.run(set_index_flag(IndexFlag::AssumeUnchanged, true, &p))
            .unwrap();
        let file = repo.workdir().join("a.txt");
        let fresh = crate::Repo::discover(repo.workdir()).unwrap();
        assert_eq!(fresh.index_flags(&file), (true, true));
        for c in clear_index_flags(&p) {
            git.run(c).unwrap();
        }
        let fresh = crate::Repo::discover(repo.workdir()).unwrap();
        assert_eq!(fresh.index_flags(&file), (false, false));
    }

    #[test]
    fn add_exe_symlink_checkout_and_resolve() {
        let (dir, repo) = fixture();
        let git = repo.git();
        std::fs::write(dir.path().join("run.sh"), "echo\n").unwrap();
        std::fs::write(dir.path().join("link"), "a.txt").unwrap();
        git.run(add_executable(&[s("run.sh")])).unwrap();
        for c in add_as_symlink(&git, &[s("link")]).unwrap() {
            git.run(c).unwrap();
        }
        let ls = git.run(["ls-files", "-s"]).unwrap();
        assert!(
            ls.lines()
                .any(|l| l.starts_with("100755") && l.ends_with("run.sh")),
            "{ls}"
        );
        assert!(
            ls.lines()
                .any(|l| l.starts_with("120000") && l.ends_with("link")),
            "{ls}"
        );

        std::fs::write(dir.path().join("a.txt"), "changed\n").unwrap();
        git.run(checkout_from("HEAD", &[s("a.txt")])).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "a\n"
        );
        assert_eq!(blob_at(&git, "HEAD", "a.txt").unwrap(), b"a\n");
        git.run(unstage_if_present(&[s("nope.txt")])).unwrap();

        // A conflict resolved with "theirs".
        git.run(["checkout", "-qb", "o"]).unwrap();
        std::fs::write(dir.path().join("a.txt"), "theirs\n").unwrap();
        git.run(["commit", "-qam", "o"]).unwrap();
        git.run(["checkout", "-q", "main"]).unwrap();
        std::fs::write(dir.path().join("a.txt"), "ours\n").unwrap();
        git.run(["commit", "-qam", "m"]).unwrap();
        assert!(!git.output(["merge", "o"]).unwrap().success());
        for c in resolve_side(true, &[s("a.txt")]) {
            git.run(c).unwrap();
        }
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "theirs\n"
        );
        assert_eq!(lfs_lock(true, &[s("x")])[0].join(" "), "lfs lock x");
    }
}
