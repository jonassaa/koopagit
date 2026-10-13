//! `CShellExt::Initialize`: compute `ITEMIS_*` flags for a file-manager selection.

use std::path::{Path, PathBuf};

use koopagit_core::Repo;
use koopagit_core::status::WcStatus;

use crate::flags::*;
use crate::menu::Selection;

/// `GetAdminDirMask`: flags derived from the repository state around `path`.
fn admin_dir_mask(path: &Path, repo: &Repo) -> ItemFlags {
    let mut mask = INGIT | INVERSIONEDFOLDER;
    if path.is_dir() {
        mask |= FOLDERINGIT;
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_owned());
        if canonical == repo.workdir() {
            mask |= WCROOT;
            if is_submodule_root(repo) {
                mask |= SUBMODULE;
            }
        }
    }
    let common = repo.common_dir();
    if repo.has_stash() {
        mask |= STASH;
    }
    if common.join("svn").join(".metadata").exists() {
        mask |= GITSVN;
    }
    if repo.git_dir().join("BISECT_START").exists() {
        mask |= BISECT;
    }
    if repo.git_dir().join("MERGE_HEAD").exists() {
        mask |= MERGEACTIVE;
    }
    if repo.workdir().join(".gitmodules").exists() {
        mask |= SUBMODULECONTAINER;
    }
    mask
}

/// The work tree is a submodule if the enclosing repository's `.gitmodules`
/// lists it as a `path`.
fn is_submodule_root(repo: &Repo) -> bool {
    let Some(parent) = repo.workdir().parent() else {
        return false;
    };
    let Ok(outer) = Repo::discover(parent) else {
        return false;
    };
    let Some(rel) = outer.relative_path(repo.workdir()) else {
        return false;
    };
    std::fs::read_to_string(outer.workdir().join(".gitmodules")).is_ok_and(|text| {
        text.lines().any(|l| {
            l.trim()
                .strip_prefix("path")
                .and_then(|r| r.trim_start().strip_prefix('='))
                .is_some_and(|v| v.trim() == rel)
        })
    })
}

fn status_flags(status: WcStatus) -> ItemFlags {
    match status {
        WcStatus::Ignored => IGNORED,
        WcStatus::Normal => NORMAL,
        WcStatus::Conflicted => CONFLICTED,
        WcStatus::Added => ADDED,
        WcStatus::Deleted => DELETED,
        _ => 0,
    }
}

fn not_versioned(status: WcStatus) -> bool {
    matches!(
        status,
        WcStatus::Unversioned | WcStatus::Ignored | WcStatus::None
    )
}

fn is_inside_git_dir(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == ".git")
}

/// `.git` exists but the repository is not usable (TortoiseGit's "Inaccessible").
fn inaccessible(dir: &Path) -> bool {
    dir.join(".git").exists() && Repo::discover(dir).is_err() && !Repo::is_bare_repo(dir)
}

/// Flags and paths for a selection. `items` are the selected files/folders;
/// `background` is the folder whose empty area was clicked (or the drop target).
/// `extended` is whether Shift was held.
pub fn analyse(items: &[PathBuf], background: Option<&Path>, extended: bool) -> Selection {
    let items: Vec<PathBuf> = items
        .iter()
        .filter(|p| !is_inside_git_dir(p))
        .cloned()
        .collect();
    let mut sel = Selection {
        items: items.clone(),
        ..Default::default()
    };
    let mut flags: ItemFlags = 0;

    if items.iter().any(|p| {
        p.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("diff") || e.eq_ignore_ascii_case("patch"))
    }) {
        flags |= PATCHFILE;
    }

    // Status is looked up for the first item only (upstream behaviour).
    if let Some(first) = items.first() {
        if Repo::is_bare_repo(first) {
            flags = BAREREPO | (flags & PATCHFILE);
        } else {
            if let Ok(repo) = Repo::discover(first) {
                flags |= admin_dir_mask(first, &repo);
                let status = repo.path_status(first).unwrap_or(WcStatus::Normal);
                if not_versioned(status) {
                    flags &= !INGIT;
                }
                flags |= status_flags(status);
                if first.is_dir() && !not_versioned(status) {
                    flags |= FOLDERINGIT;
                }
                fill_repo_info(&mut sel, &repo);
            }
            if first.is_dir() {
                flags |= FOLDER;
            }
            if first.parent().is_some_and(|p| Repo::discover(p).is_ok()) {
                flags |= INVERSIONEDFOLDER;
            }
            if first.is_dir() && flags & (INVERSIONEDFOLDER | INGIT) == 0 && inaccessible(first) {
                flags |= INACCESSIBLE;
            }
        }
    }

    if let Some(folder) = background {
        let mut folder_flags = FOLDER;
        if Repo::is_bare_repo(folder) {
            folder_flags = BAREREPO;
        } else if let Ok(repo) = Repo::discover(folder) {
            folder_flags |= admin_dir_mask(folder, &repo);
            let status = repo.path_status(folder).unwrap_or(WcStatus::Normal);
            folder_flags |= status_flags(status);
            if sel.branch.is_none() && sel.head_id.is_none() {
                fill_repo_info(&mut sel, &repo);
            }
        } else if inaccessible(folder) {
            folder_flags |= INACCESSIBLE;
        }
        if items.is_empty() {
            flags |= ONLYONE;
        }
        // Upstream ORs the folder's flags into the item flags, so a background
        // click inside a work tree always has INGIT set.
        flags |= folder_flags;
        sel.folder = Some(folder.to_owned());
    }

    match items.len() {
        2 => flags |= TWO,
        1 => {
            flags |= ONLYONE;
            if items[0].is_dir() {
                sel.folder = Some(items[0].clone());
            }
        }
        _ => {}
    }
    if sel.folder.is_none() {
        sel.folder = items.first().and_then(|p| p.parent()).map(Path::to_owned);
    }
    if extended {
        flags |= EXTENDED;
    }
    sel.flags = flags;
    sel
}

fn fill_repo_info(sel: &mut Selection, repo: &Repo) {
    sel.branch = repo.current_branch().ok().flatten();
    sel.head_id = repo.head_id().ok().flatten();
    sel.is_submodule = is_submodule_root(repo);
    sel.has_lfs = repo.git_dir().join("lfs").exists() || repo.common_dir().join("lfs").exists();
}

#[cfg(test)]
mod tests {
    use super::*;
    use koopagit_core::git_cli::Git;

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let git = Git::new(dir.path());
        git.run(["init", "-q", "-b", "main"]).unwrap();
        git.run(["config", "user.name", "T"]).unwrap();
        git.run(["config", "user.email", "t@e"]).unwrap();
        std::fs::write(dir.path().join("a.txt"), "a").unwrap();
        git.run(["add", "."]).unwrap();
        git.run(["commit", "-qm", "i"]).unwrap();
        dir
    }

    #[test]
    fn background_of_work_tree_root() {
        let dir = repo();
        let root = dir.path().canonicalize().unwrap();
        let sel = analyse(&[], Some(&root), false);
        for f in [INGIT, FOLDER, FOLDERINGIT, WCROOT, ONLYONE, NORMAL] {
            assert_ne!(sel.flags & f, 0, "flag {f:#x} missing in {:#x}", sel.flags);
        }
        assert_eq!(sel.branch.as_deref(), Some("main"));
    }

    #[test]
    fn untracked_and_conflict_free_file() {
        let dir = repo();
        let root = dir.path().canonicalize().unwrap();
        std::fs::write(root.join("new.patch"), "").unwrap();
        let sel = analyse(&[root.join("new.patch")], None, true);
        assert_eq!(sel.flags & INGIT, 0);
        for f in [INVERSIONEDFOLDER, PATCHFILE, ONLYONE, EXTENDED] {
            assert_ne!(sel.flags & f, 0, "flag {f:#x} missing");
        }
        let tracked = analyse(&[root.join("a.txt"), root.join("new.patch")], None, false);
        assert_ne!(tracked.flags & (INGIT | NORMAL | TWO), 0);
        assert_eq!(tracked.flags & ONLYONE, 0);
    }

    #[test]
    fn plain_folder_outside_repo() {
        let dir = tempfile::tempdir().unwrap();
        let sel = analyse(&[], Some(dir.path()), false);
        assert_eq!(sel.flags, FOLDER | ONLYONE);
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        assert_ne!(
            analyse(&[], Some(dir.path()), false).flags & INACCESSIBLE,
            0
        );
    }
}
