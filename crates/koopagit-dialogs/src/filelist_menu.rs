//! The file status list context menu (`CGitStatusListCtrl::OnContextMenuList`,
//! spec-menus §1.4–1.5). Pure: the caller describes the selection and the
//! owning dialog, [`build`] returns the entries in upstream order.

use koopagit_core::status::Action as StatusAction;

use crate::menus::{Entry, Menu, leaf};

crate::menu_actions! {
    /// `IDGITLC_*` entries. Each id is also a bit of [`PopMask`] (`1 << id`).
    pub enum FileAction {
        Revert = 1,
        Compare = 2,
        CompareWc = 3,
        CompareParentWc = 4,
        GnuDiff1 = 5,
        CompareTwoRevisions = 6,
        GnuDiff2Revisions = 7,
        CompareTwoFiles = 8,
        Commit = 9,
        Add = 10,
        AddExe = 11,
        AddLink = 12,
        EditConflict = 13,
        ResolveConflict = 14,
        ResolveTheirs = 15,
        ResolveMine = 16,
        SkipWorktree = 17,
        AssumeValid = 18,
        UnsetIgnoreLocalChanges = 19,
        CreateRestore = 20,
        RestorePath = 21,
        LfsLock = 22,
        LfsUnlock = 23,
        RevertToRev = 24,
        RevertToParent = 25,
        Log = 26,
        LogSubmodule = 27,
        LogOldName = 28,
        Blame = 29,
        Export = 30,
        SaveAs = 31,
        ViewRev = 32,
        Open = 33,
        OpenWith = 34,
        Explore = 35,
        PrepareDiff = 36,
        PrepareDiffCompare = 37,
        Delete = 38,
        Ignore = 39,
        IgnoreMask = 40,
        IgnoreFolder = 41,
        CopyFull = 42,
        CopyRelPaths = 43,
        CopyFileNames = 44,
        CopyExt = 45,
        /// Changelists (`IDGITLC_CHECKGROUP`); only used as a mask bit.
        Changelists = 46,
        /// `IDGITLC_POPRESTORE`; only used as a mask bit.
        RestoreBit = 47,
    }
}

/// `dwContextMenus`: which entries a dialog allows (`GITSLC_POP*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PopMask(pub u64);

const fn bit(a: FileAction) -> u64 {
    1u64 << (a as u32)
}

impl PopMask {
    pub const ALL: PopMask = PopMask(u64::MAX);

    pub const COMPARE_WITH_BASE: u64 = bit(FileAction::Compare);
    pub const COMPARE: u64 = bit(FileAction::CompareWc);
    pub const GNU_DIFF: u64 = bit(FileAction::GnuDiff1);
    pub const REVERT: u64 = bit(FileAction::Revert);
    pub const SHOW_LOG: u64 = bit(FileAction::Log);
    pub const SHOW_LOG_SUBMODULE: u64 = bit(FileAction::LogSubmodule);
    pub const SHOW_LOG_OLD_NAME: u64 = bit(FileAction::LogOldName);
    pub const OPEN: u64 = bit(FileAction::Open);
    pub const DELETE: u64 = bit(FileAction::Delete);
    pub const ADD: u64 = bit(FileAction::Add);
    pub const IGNORE: u64 = bit(FileAction::Ignore);
    pub const CONFLICT: u64 = bit(FileAction::EditConflict);
    pub const RESOLVE: u64 = bit(FileAction::ResolveConflict);
    pub const EXPLORE: u64 = bit(FileAction::Explore);
    pub const COMMIT: u64 = bit(FileAction::Commit);
    pub const CHANGELISTS: u64 = bit(FileAction::Changelists);
    pub const BLAME: u64 = bit(FileAction::Blame);
    pub const SAVE_AS: u64 = bit(FileAction::SaveAs);
    pub const COMPARE_TWO_FILES: u64 = bit(FileAction::CompareTwoFiles);
    pub const RESTORE: u64 = bit(FileAction::RestoreBit);
    pub const ASSUME_VALID: u64 = bit(FileAction::AssumeValid);
    pub const SKIP_WORKTREE: u64 = bit(FileAction::SkipWorktree);
    pub const EXPORT: u64 = bit(FileAction::Export);
    pub const UNSET_IGNORE_LOCAL_CHANGES: u64 = bit(FileAction::UnsetIgnoreLocalChanges);
    pub const PREPARE_DIFF: u64 = bit(FileAction::PrepareDiff);
    pub const LFS_LOCK: u64 = bit(FileAction::LfsLock);
    pub const LFS_UNLOCK: u64 = bit(FileAction::LfsUnlock);

    const fn all_but(bits: u64) -> PopMask {
        PopMask(u64::MAX ^ bits)
    }

    /// `CommitDlg.cpp:251`: POPALL ^ (COMMIT | SAVEAS | PREPAREDIFF).
    pub const COMMIT_DLG: PopMask =
        Self::all_but(Self::COMMIT | Self::SAVE_AS | Self::PREPARE_DIFF);
    /// `AddDlg.cpp:63`: POPALL ^ (ADD | COMMIT | CHANGELISTS | PREPAREDIFF).
    pub const ADD_DLG: PopMask =
        Self::all_but(Self::ADD | Self::COMMIT | Self::CHANGELISTS | Self::PREPARE_DIFF);
    /// `RevertDlg.cpp:65`: POPALL ^ (COMMIT | RESTORE | CHANGELISTS).
    pub const REVERT_DLG: PopMask = Self::all_but(Self::COMMIT | Self::RESTORE | Self::CHANGELISTS);
    /// `ResolveDlg.cpp:60`.
    pub const RESOLVE_DLG: PopMask = Self::all_but(
        Self::IGNORE
            | Self::ADD
            | Self::COMMIT
            | Self::EXPORT
            | Self::RESTORE
            | Self::SAVE_AS
            | Self::PREPARE_DIFF
            | Self::CHANGELISTS,
    );
    /// `ChangedDlg.cpp:101` (Check for modifications / Working Tree).
    pub const CHANGED_DLG: PopMask =
        Self::all_but(Self::SAVE_AS | Self::RESTORE | Self::PREPARE_DIFF);
    /// `LogDlg.cpp:308`: the Log dialog's changed-files pane.
    pub const LOG_DLG: PopMask = Self::all_but(Self::RESTORE | Self::CHANGELISTS);
    /// `RebaseDlg.cpp:243`.
    pub const REBASE_DLG: PopMask = Self::all_but(Self::COMMIT | Self::RESTORE | Self::CHANGELISTS);
    /// `SyncDlg.cpp:1052-1055`: in/out changelists.
    pub const SYNC_CHANGELIST: PopMask =
        PopMask(bit(FileAction::CompareTwoRevisions) | bit(FileAction::GnuDiff2Revisions));
    /// `SyncDlg.cpp:1068-1070`: conflict list.
    pub const SYNC_CONFLICT: PopMask =
        PopMask(Self::EXPLORE | Self::OPEN | Self::SHOW_LOG | Self::CONFLICT | Self::RESOLVE);
    /// `LFSLocksDlg.cpp:61`.
    pub const LFS_LOCKS_DLG: PopMask = PopMask(Self::LFS_LOCK | Self::LFS_UNLOCK);

    pub fn has(self, bits: u64) -> bool {
        self.0 & bits != 0
    }

    pub fn allows(self, a: FileAction) -> bool {
        self.has(bit(a))
    }
}

/// `CTGitPath::LOGACTIONS_*` bits of one row.
pub mod st {
    pub const ADD: u32 = 0x1;
    pub const MODIFIED: u32 = 0x2;
    pub const REPLACED: u32 = 0x4;
    pub const DEL: u32 = 0x8;
    pub const UNM: u32 = 0x10;
    pub const COPY: u32 = 0x40;
    pub const ASSUMEVALID: u32 = 0x200;
    pub const SKIPWORKTREE: u32 = 0x400;
    pub const MISS: u32 = 0x1000;
    pub const IGN: u32 = 0x4000_0000;
    pub const UNV: u32 = 0x8000_0000;
}

/// Maps a status to `LOGACTIONS_*` (a missing file is `MISSING | DELETED`).
pub fn status_flags(action: StatusAction) -> u32 {
    match action {
        StatusAction::Added => st::ADD,
        StatusAction::Modified => st::MODIFIED,
        StatusAction::Deleted => st::DEL,
        StatusAction::Renamed => st::REPLACED,
        StatusAction::Copied => st::COPY,
        StatusAction::Unmerged => st::UNM,
        StatusAction::Missing => st::MISS | st::DEL,
        StatusAction::Unversioned => st::UNV,
        StatusAction::Ignored => st::IGN,
    }
}

/// One selected row, as the menu needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RowInfo {
    /// Repository-relative path.
    pub path: String,
    pub old_path: Option<String>,
    pub flags: u32,
    pub is_dir: bool,
    /// A "Restore after commit" backup exists for this path.
    pub has_backup: bool,
    /// LFS lock state when the lock column is known.
    pub lfs_locked: Option<bool>,
}

/// Dialog-level state (`m_CurrentVersion`, `m_amend`, `m_Rev1/2`, ...).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListInfo {
    /// The revision the list shows; `None` = working tree (`WT`).
    pub current_version: Option<String>,
    pub has_wc: bool,
    pub amend: bool,
    /// SyncDlg's `m_Rev1`/`m_Rev2` are both set.
    pub has_two_revisions: bool,
    /// The repository has no commits yet (`IsInitRepos`).
    pub initial_repo: bool,
    /// Rebase in progress: mine/theirs swap (`m_bIsRevertTheirMy`).
    pub rebase_active: bool,
    pub uses_lfs: bool,
    /// Shift was held when the menu opened (reveals "Add as...").
    pub shift: bool,
    /// `subject (shorthash)` of `current_version`'s parent, if it resolves.
    pub parent_title: Option<(String, String)>,
    /// What "Mark for comparison" stored: the label shown in `Diff with "X"`.
    pub marked_label: Option<String>,
}

impl ListInfo {
    fn wt(&self) -> bool {
        self.current_version.is_none()
    }
}

fn short_subject(subject: &str) -> String {
    if subject.chars().count() > 20 {
        format!("{}...", subject.chars().take(20).collect::<String>())
    } else {
        subject.to_owned()
    }
}

/// Extension including the dot (`.txt`), or empty.
fn extension(path: &str) -> String {
    crate::filelist::extension(path)
}

/// Builds the item menu. `sel` are the selected rows; `focused` is the row the
/// menu was opened on (its flags decide, like upstream's selection mark).
pub fn build(sel: &[RowInfo], focused: &RowInfo, mask: PopMask, info: &ListInfo) -> Vec<Entry> {
    use FileAction as A;
    let mut m = Menu::new();
    let n = sel.len();
    if n == 0 {
        return Vec::new();
    }
    let s = focused.flags;
    let has = |bits: u32| s & bits != 0;
    let wt = info.wt();

    // Conflicts.
    if has(st::UNM) {
        if n == 1 && mask.allows(A::EditConflict) {
            m.item(A::EditConflict, "Edit conflicts");
        }
        if mask.allows(A::ResolveConflict) {
            m.item(A::ResolveConflict, "Resolved");
            let (first, second) = if info.rebase_active {
                ((A::ResolveMine, "mine"), (A::ResolveTheirs, "theirs"))
            } else {
                ((A::ResolveTheirs, "theirs"), (A::ResolveMine, "mine"))
            };
            m.item(first.0, format!("Resolve conflict using \"{}\"", first.1));
            m.item(second.0, format!("Resolve conflict using \"{}\"", second.1));
        }
        if mask.has(PopMask::CONFLICT | PopMask::RESOLVE) {
            m.sep();
        }
    }

    // Unversioned / ignored.
    if has(st::UNV | st::IGN) {
        if mask.allows(A::Add) {
            m.item(A::Add, "Add");
            if !focused.is_dir && info.shift {
                m.item(A::AddExe, "Add as Executable (+x)");
                m.item(A::AddLink, "Add as Symlink");
            }
        }
        if mask.allows(A::Commit) {
            m.item(A::Commit, "Commit...");
        }
    }

    // Versioned diffs (condition V).
    let versioned = !has(st::UNV | st::IGN)
        && (!has(st::MISS) || s == st::MISS | st::DEL || s == st::MISS | st::DEL | st::MODIFIED);
    let mut compare_added = false;
    if versioned {
        if mask.allows(A::Compare) {
            m.item(A::Compare, "Compare with base");
            compare_added = true;
        }
        if !info.initial_repo && mask.allows(A::GnuDiff1) {
            m.item(A::GnuDiff1, "Show changes as unified diff");
            compare_added = true;
        }
        if mask.allows(A::CompareWc) && info.has_wc && !wt {
            m.item(A::CompareWc, "Compare with working tree");
            compare_added = true;
            if let Some((subject, short)) = &info.parent_title {
                m.item(
                    A::CompareParentWc,
                    format!(
                        "Compare parent with working tree: \"{}\" ({short})",
                        short_subject(subject)
                    ),
                );
            }
        }
    }
    if compare_added {
        m.sep();
    }

    if info.has_two_revisions && mask.allows(A::CompareTwoRevisions) {
        m.item(A::CompareTwoRevisions, "Compare revisions");
        m.item(A::GnuDiff2Revisions, "Show changes as unified diff");
    }
    if n == 2 && mask.allows(A::CompareTwoFiles) && sel.iter().all(|r| !r.is_dir) {
        m.item(A::CompareTwoFiles, "Compare two files");
    }

    // Working-tree actions (condition W).
    let w = !has(st::UNV | st::IGN) && info.has_wc;
    if w {
        if mask.allows(A::Commit) && wt && !has(st::SKIPWORKTREE | st::ASSUMEVALID) {
            m.item(A::Commit, "Commit...");
        }
        if mask.allows(A::Revert) && wt {
            m.item(A::Revert, "Revert...");
        }
        if wt && mask.allows(A::SkipWorktree) && !has(st::ADD | st::UNM | st::SKIPWORKTREE) {
            m.item(A::SkipWorktree, "Skip worktree");
        }
        if wt && mask.allows(A::AssumeValid) && !has(st::ADD | st::DEL | st::UNM | st::ASSUMEVALID)
        {
            m.item(A::AssumeValid, "Assume Unchanged");
        }
        if wt && mask.allows(A::UnsetIgnoreLocalChanges) && has(st::SKIPWORKTREE | st::ASSUMEVALID)
        {
            m.item(
                A::UnsetIgnoreLocalChanges,
                "Unflag as skip-worktree and assume-unchanged",
            );
        }
        if mask.has(PopMask::RESTORE) && !focused.is_dir {
            if focused.has_backup {
                m.item(A::RestorePath, "Restore");
            } else {
                m.item(A::CreateRestore, "Restore after commit");
            }
        }
        if wt
            && mask.has(PopMask::LFS_LOCK | PopMask::LFS_UNLOCK)
            && !has(st::UNM)
            && info.uses_lfs
            && sel.iter().all(|r| !r.is_dir)
        {
            let known: Vec<bool> = sel.iter().filter_map(|r| r.lfs_locked).collect();
            let column_hidden = known.len() != n;
            if column_hidden || known.iter().all(|l| !l) {
                m.item(A::LfsLock, "LFS Lock");
            }
            if column_hidden || known.iter().all(|l| *l) {
                m.item(A::LfsUnlock, "LFS Unlock");
            }
        }
        if !wt && mask.allows(A::RevertToRev) && !has(st::DEL) {
            m.item(A::RevertToRev, "Revert to this revision");
        }
        if !wt
            && mask.allows(A::RevertToParent)
            && let Some((subject, short)) = &info.parent_title
        {
            m.item(
                A::RevertToParent,
                format!(
                    "Revert to parent revision: \"{}\" ({short})",
                    short_subject(subject)
                ),
            );
        }
    }

    // History.
    if n == 1 && !has(st::UNV | st::IGN) {
        if mask.allows(A::Log) {
            m.item(A::Log, "Show log");
        }
        if mask.allows(A::LogSubmodule) && focused.is_dir {
            m.item(A::LogSubmodule, "Show log of submodule");
        }
        if mask.allows(A::LogOldName) && has(st::REPLACED | st::COPY) && focused.old_path.is_some()
        {
            m.item(A::LogOldName, "Show log before rename/copy");
        }
        let added_in_wt = has(st::ADD) && wt;
        if mask.allows(A::Blame) && !focused.is_dir && !has(st::DEL) && !added_in_wt && info.has_wc
        {
            m.item(A::Blame, "Blame");
        }
    }

    // Files.
    if mask.allows(A::Export) && !has(st::DEL | st::MISS) {
        m.item(A::Export, "Export selection to...");
    }
    if n == 1 && mask.allows(A::SaveAs) && !focused.is_dir && !has(st::DEL) {
        m.item(A::SaveAs, "Save revision to...");
    }
    if n == 1 && mask.allows(A::Open) && !focused.is_dir && !has(st::DEL | st::MISS) {
        m.item(A::ViewRev, "View revision in alternative editor");
        m.item(A::Open, "Open");
        m.item(A::OpenWith, "Open with...");
    }
    if n == 1 && mask.allows(A::Explore) && !has(st::DEL) && info.has_wc {
        m.item(A::Explore, "Explore to");
    }
    if n == 1 && mask.allows(A::PrepareDiff) && !has(st::DEL) {
        m.sep();
        m.item(A::PrepareDiff, "Mark for comparison");
        if let Some(label) = &info.marked_label {
            m.item(A::PrepareDiffCompare, format!("Diff with \"{label}\""));
        }
    }

    if has(st::UNV | st::IGN | st::MISS) && mask.allows(A::Delete) {
        m.item(A::Delete, "Delete");
    }

    // Ignore (§1.5).
    if has(st::UNV | st::DEL) && mask.allows(A::Ignore) {
        let first_ext = extension(&sel[0].path);
        let same_ext = sel.iter().all(|r| extension(&r.path) == first_ext);
        let by_name = if n == 1 {
            sel[0]
                .path
                .rsplit('/')
                .next()
                .unwrap_or(&sel[0].path)
                .to_owned()
        } else {
            format!("Ignore {n} items by name")
        };
        if same_ext {
            let mut children = vec![leaf(A::Ignore, by_name, true)];
            if !first_ext.is_empty() {
                children.push(leaf(A::IgnoreMask, format!("*{first_ext}"), true));
            }
            if n == 1
                && let Some((dir, _)) = sel[0].path.rsplit_once('/')
            {
                children.push(leaf(A::IgnoreFolder, dir.to_owned(), true));
            }
            m.sub("Add to ignore list", children);
        } else {
            m.item(
                A::Ignore,
                if n == 1 {
                    "Add to ignore list".to_owned()
                } else {
                    format!("Ignore {n} items by name")
                },
            );
            if !first_ext.is_empty() {
                m.item(A::IgnoreMask, format!("Ignore {n} items by extension"));
            }
        }
    }

    m.sep();
    m.sub(
        "Copy to clipboard",
        vec![
            leaf(A::CopyFull, "full paths", true),
            leaf(A::CopyRelPaths, "relative paths", true),
            leaf(A::CopyFileNames, "file/foldernames", true),
            leaf(A::CopyExt, "Copy all information to clipboard", true),
        ],
    );
    m.finish()
}

#[cfg(test)]
#[allow(clippy::cloned_ref_to_slice_refs)]
mod tests {
    use super::*;
    use crate::menus::{ActionId, Entry};

    fn row(path: &str, flags: u32) -> RowInfo {
        RowInfo {
            path: path.into(),
            flags,
            ..Default::default()
        }
    }

    fn wt() -> ListInfo {
        ListInfo {
            has_wc: true,
            ..Default::default()
        }
    }

    /// Flattened titles; submenus as `[title: a|b]`, separators as `---`.
    fn titles(e: &[Entry]) -> Vec<String> {
        e.iter()
            .map(|e| match e {
                Entry::Item(l) => l.title.clone(),
                Entry::Separator => "---".into(),
                Entry::Sub {
                    title, children, ..
                } => format!(
                    "[{title}: {}]",
                    children
                        .iter()
                        .map(|c| c.title.as_str())
                        .collect::<Vec<_>>()
                        .join("|")
                ),
            })
            .collect()
    }

    fn ids(e: &[Entry]) -> Vec<FileAction> {
        e.iter()
            .filter_map(|e| match e {
                Entry::Item(l) => FileAction::from_id(l.id),
                _ => None,
            })
            .collect()
    }

    const COPY: &str = "[Copy to clipboard: full paths|relative paths|file/foldernames|Copy all information to clipboard]";

    #[test]
    fn modified_file_in_commit_dialog() {
        let r = row("src/a.rs", st::MODIFIED);
        let e = build(&[r.clone()], &r, PopMask::COMMIT_DLG, &wt());
        assert_eq!(
            titles(&e),
            [
                "Compare with base",
                "Show changes as unified diff",
                "---",
                "Revert...",
                "Skip worktree",
                "Assume Unchanged",
                "Restore after commit",
                "Show log",
                "Blame",
                "Export selection to...",
                "View revision in alternative editor",
                "Open",
                "Open with...",
                "Explore to",
                "---",
                COPY,
            ]
        );
    }

    #[test]
    fn modified_file_in_working_tree_dialog_has_commit_and_mark() {
        let r = row("a.txt", st::MODIFIED);
        let e = ids(&build(&[r.clone()], &r, PopMask::CHANGED_DLG, &wt()));
        assert!(e.contains(&FileAction::Commit));
        // ChangedDlg excludes SAVEAS | RESTORE | PREPAREDIFF.
        assert!(!e.contains(&FileAction::PrepareDiff));
        assert!(!e.contains(&FileAction::CreateRestore));
        assert!(!e.contains(&FileAction::SaveAs));
        assert!(!e.contains(&FileAction::PrepareDiffCompare));
        let marked = ListInfo {
            marked_label: Some("b.txt".into()),
            ..wt()
        };
        let e = titles(&build(&[r.clone()], &r, PopMask::LOG_DLG, &marked));
        assert!(e.contains(&"Mark for comparison".to_owned()));
        assert!(e.contains(&"Diff with \"b.txt\"".to_owned()));
    }

    #[test]
    fn unversioned_file() {
        let r = row("dir/new.txt", st::UNV);
        let e = build(&[r.clone()], &r, PopMask::COMMIT_DLG, &wt());
        assert_eq!(
            titles(&e),
            [
                "Add",
                "Export selection to...",
                "View revision in alternative editor",
                "Open",
                "Open with...",
                "Explore to",
                "Delete",
                "[Add to ignore list: new.txt|*.txt|dir]",
                "---",
                COPY,
            ]
        );
        let shift = ListInfo {
            shift: true,
            ..wt()
        };
        let e = titles(&build(&[r.clone()], &r, PopMask::CHANGED_DLG, &shift));
        assert_eq!(
            &e[..4],
            [
                "Add",
                "Add as Executable (+x)",
                "Add as Symlink",
                "Commit..."
            ]
        );
        // The Add dialog hides Add and Commit.
        let e = ids(&build(&[r.clone()], &r, PopMask::ADD_DLG, &wt()));
        assert!(!e.contains(&FileAction::Add) && !e.contains(&FileAction::Commit));
    }

    #[test]
    fn conflicted_file_and_rebase_swap() {
        let r = row("c.txt", st::UNM);
        let e = titles(&build(&[r.clone()], &r, PopMask::RESOLVE_DLG, &wt()));
        assert_eq!(
            &e[..5],
            [
                "Edit conflicts",
                "Resolved",
                "Resolve conflict using \"theirs\"",
                "Resolve conflict using \"mine\"",
                "---"
            ]
        );
        assert!(!e.iter().any(|t| t.starts_with("[Add to ignore")));
        assert!(!e.contains(&"Export selection to...".to_owned()));
        let rebase = ListInfo {
            rebase_active: true,
            ..wt()
        };
        let e = build(&[r.clone()], &r, PopMask::RESOLVE_DLG, &rebase);
        assert_eq!(ids(&e)[2], FileAction::ResolveMine);
        // Two conflicted rows: no Edit conflicts.
        let e = ids(&build(
            &[r.clone(), r.clone()],
            &r,
            PopMask::RESOLVE_DLG,
            &wt(),
        ));
        assert!(!e.contains(&FileAction::EditConflict));
    }

    #[test]
    fn missing_and_deleted_files() {
        let missing = row("gone.txt", st::MISS | st::DEL);
        let e = ids(&build(
            &[missing.clone()],
            &missing,
            PopMask::CHANGED_DLG,
            &wt(),
        ));
        assert!(e.contains(&FileAction::Compare));
        assert!(e.contains(&FileAction::Delete));
        assert!(!e.contains(&FileAction::Blame));
        assert!(!e.contains(&FileAction::Open));
        assert!(!e.contains(&FileAction::Export));
        assert!(!e.contains(&FileAction::AssumeValid));
        let staged_delete = row("d.txt", st::DEL);
        let e = titles(&build(
            &[staged_delete.clone()],
            &staged_delete,
            PopMask::CHANGED_DLG,
            &wt(),
        ));
        assert!(e.iter().any(|t| t.starts_with("[Add to ignore list")));
        assert!(!e.contains(&"Delete".to_owned()));
        assert!(!e.contains(&"Mark for comparison".to_owned()));
    }

    #[test]
    fn added_and_flagged_files() {
        let added = row("n.rs", st::ADD);
        let e = ids(&build(
            &[added.clone()],
            &added,
            PopMask::CHANGED_DLG,
            &wt(),
        ));
        assert!(
            !e.contains(&FileAction::Blame),
            "added file in WT has no blame"
        );
        assert!(!e.contains(&FileAction::SkipWorktree));
        assert!(!e.contains(&FileAction::AssumeValid));
        let flagged = row("f.rs", st::SKIPWORKTREE);
        let e = ids(&build(
            &[flagged.clone()],
            &flagged,
            PopMask::CHANGED_DLG,
            &wt(),
        ));
        assert!(e.contains(&FileAction::UnsetIgnoreLocalChanges));
        assert!(!e.contains(&FileAction::Commit));
        assert!(!e.contains(&FileAction::SkipWorktree));
        assert!(e.contains(&FileAction::AssumeValid));
    }

    #[test]
    fn renamed_file_and_log_pane() {
        let mut r = row("new.rs", st::REPLACED);
        r.old_path = Some("old.rs".into());
        let info = ListInfo {
            current_version: Some("abc123".into()),
            parent_title: Some(("A rather long subject line here".into(), "1234567".into())),
            ..wt()
        };
        let e = titles(&build(&[r.clone()], &r, PopMask::LOG_DLG, &info));
        assert_eq!(
            &e[..6],
            [
                "Compare with base",
                "Show changes as unified diff",
                "Compare with working tree",
                "Compare parent with working tree: \"A rather long subjec...\" (1234567)",
                "---",
                "Revert to this revision",
            ]
        );
        assert!(e.contains(&"Show log before rename/copy".to_owned()));
        assert!(e.contains(&"Save revision to...".to_owned()));
        assert!(!e.contains(&"Revert...".to_owned()), "not WT");
        assert!(!e.contains(&"Commit...".to_owned()));
        // A bare repository has no working copy.
        let bare = ListInfo {
            has_wc: false,
            ..info
        };
        let e = ids(&build(&[r.clone()], &r, PopMask::LOG_DLG, &bare));
        assert!(!e.contains(&FileAction::RevertToRev));
        assert!(!e.contains(&FileAction::Explore));
        assert!(!e.contains(&FileAction::CompareWc));
    }

    #[test]
    fn multiple_selection() {
        let a = row("a.txt", st::MODIFIED);
        let b = row("b.rs", st::MODIFIED);
        let e = titles(&build(
            &[a.clone(), b.clone()],
            &a,
            PopMask::CHANGED_DLG,
            &wt(),
        ));
        assert!(e.contains(&"Compare two files".to_owned()));
        assert!(!e.contains(&"Show log".to_owned()));
        assert!(!e.contains(&"Open".to_owned()));
        let three = [a.clone(), b.clone(), row("c.txt", st::MODIFIED)];
        let e = titles(&build(&three, &a, PopMask::CHANGED_DLG, &wt()));
        assert!(!e.contains(&"Compare two files".to_owned()));
        // Mixed extensions: flat ignore entries.
        let u1 = row("x.txt", st::UNV);
        let u2 = row("y.rs", st::UNV);
        let e = titles(&build(&[u1.clone(), u2], &u1, PopMask::CHANGED_DLG, &wt()));
        assert!(e.contains(&"Ignore 2 items by name".to_owned()));
        assert!(e.contains(&"Ignore 2 items by extension".to_owned()));
        // Same extension: submenu without the folder entry.
        let u3 = row("z.txt", st::UNV);
        let e = titles(&build(&[u1.clone(), u3], &u1, PopMask::CHANGED_DLG, &wt()));
        assert!(e.contains(&"[Add to ignore list: Ignore 2 items by name|*.txt]".to_owned()));
    }

    #[test]
    fn sync_masks_and_lfs() {
        let r = row("a.txt", st::MODIFIED);
        let info = ListInfo {
            has_two_revisions: true,
            ..Default::default()
        };
        let e = titles(&build(&[r.clone()], &r, PopMask::SYNC_CHANGELIST, &info));
        assert_eq!(
            e,
            [
                "Compare revisions",
                "Show changes as unified diff",
                "---",
                COPY
            ]
        );
        let lfs = ListInfo {
            uses_lfs: true,
            ..wt()
        };
        let mut locked = r.clone();
        locked.lfs_locked = Some(true);
        let e = ids(&build(
            &[locked.clone()],
            &locked,
            PopMask::CHANGED_DLG,
            &lfs,
        ));
        assert!(e.contains(&FileAction::LfsUnlock) && !e.contains(&FileAction::LfsLock));
        let e = ids(&build(&[r.clone()], &r, PopMask::CHANGED_DLG, &lfs));
        assert!(e.contains(&FileAction::LfsUnlock) && e.contains(&FileAction::LfsLock));
        let e = ids(&build(&[r.clone()], &r, PopMask::LFS_LOCKS_DLG, &lfs));
        assert_eq!(e, [FileAction::LfsLock, FileAction::LfsUnlock]);
    }

    #[test]
    fn every_dialog_mask_builds_for_every_status() {
        let masks = [
            PopMask::COMMIT_DLG,
            PopMask::ADD_DLG,
            PopMask::REVERT_DLG,
            PopMask::RESOLVE_DLG,
            PopMask::CHANGED_DLG,
            PopMask::LOG_DLG,
            PopMask::REBASE_DLG,
            PopMask::SYNC_CHANGELIST,
            PopMask::SYNC_CONFLICT,
        ];
        let flags = [
            st::ADD,
            st::MODIFIED,
            st::REPLACED,
            st::DEL,
            st::UNM,
            st::MISS | st::DEL,
            st::UNV,
            st::IGN,
        ];
        for mask in masks {
            for f in flags {
                for n in 1..=3 {
                    let rows: Vec<RowInfo> = (0..n).map(|i| row(&format!("f{i}.txt"), f)).collect();
                    let e = build(&rows, &rows[0], mask, &wt());
                    assert!(
                        crate::menus::pack(&e).len() <= crate::menus::MAX_BLOCKS,
                        "too many blocks"
                    );
                    // Every item id is a known action.
                    for e in &e {
                        if let Entry::Item(l) = e {
                            assert!(FileAction::from_id(l.id).is_some());
                        }
                    }
                }
            }
        }
        assert!(build(&[], &row("a", 0), PopMask::ALL, &wt()).is_empty());
    }
}
