//! The TortoiseGit context menu (`MenuInfo.cpp` table + `CShellExt::QueryContextMenu`).

use std::path::{Path, PathBuf};

use koopagit_core::launcher::Invocation;
use koopagit_core::settings::{Settings, keys};

use crate::flags::*;

/// Bits of `TGitContextMenuEntries`, shared by the top-level, Shift-only and hidden masks.
pub mod bits {
    pub const SYNC: u64 = 0x2;
    pub const COMMIT: u64 = 0x4;
    pub const ADD: u64 = 0x8;
    pub const REVERT: u64 = 0x10;
    pub const CLEANUP: u64 = 0x20;
    pub const RESOLVE: u64 = 0x40;
    pub const SWITCH: u64 = 0x80;
    pub const SENDMAIL: u64 = 0x100;
    pub const EXPORT: u64 = 0x200;
    pub const CREATE_REPO: u64 = 0x400;
    pub const BRANCH: u64 = 0x800;
    pub const MERGE: u64 = 0x1000;
    pub const REMOVE: u64 = 0x2000;
    pub const RENAME: u64 = 0x4000;
    pub const SUBMODULE_UPDATE: u64 = 0x8000;
    pub const DIFF: u64 = 0x10000;
    pub const LOG: u64 = 0x20000;
    pub const CONFLICT_EDITOR: u64 = 0x40000;
    pub const REF_BROWSER: u64 = 0x80000;
    pub const SHOW_CHANGED: u64 = 0x100000;
    pub const IGNORE: u64 = 0x200000;
    pub const REF_LOG: u64 = 0x400000;
    pub const BLAME: u64 = 0x800000;
    pub const REPO_BROWSER: u64 = 0x1000000;
    pub const APPLY_PATCH: u64 = 0x2000000;
    pub const REMOVE_KEEP: u64 = 0x4000000;
    pub const SVN_REBASE: u64 = 0x8000000;
    pub const SVN_DCOMMIT: u64 = 0x10000000;
    pub const SVN_IGNORE: u64 = 0x40000000;
    pub const LOG_SUBMODULE: u64 = 0x1_0000_0000;
    pub const PREV_DIFF: u64 = 0x2_0000_0000;
    pub const PULL: u64 = 0x8_0000_0000;
    pub const PUSH: u64 = 0x10_0000_0000;
    pub const CLONE: u64 = 0x20_0000_0000;
    pub const TAG: u64 = 0x40_0000_0000;
    pub const FORMAT_PATCH: u64 = 0x80_0000_0000;
    pub const IMPORT_PATCH: u64 = 0x100_0000_0000;
    pub const DIFF_LATER: u64 = 0x200_0000_0000;
    pub const FETCH: u64 = 0x400_0000_0000;
    pub const REBASE: u64 = 0x800_0000_0000;
    pub const STASH_SAVE: u64 = 0x1000_0000_0000;
    pub const STASH_APPLY: u64 = 0x2000_0000_0000;
    pub const STASH_LIST: u64 = 0x4000_0000_0000;
    pub const SUBMODULE_ADD: u64 = 0x8000_0000_0000;
    pub const SUBMODULE_SYNC: u64 = 0x1_0000_0000_0000;
    pub const STASH_POP: u64 = 0x2_0000_0000_0000;
    pub const BISECT: u64 = 0x8_0000_0000_0000;
    pub const INACCESSIBLE: u64 = 0x10_0000_0000_0000;
    pub const SVN_FETCH: u64 = 0x80_0000_0000_0000;
    pub const REVISION_GRAPH: u64 = 0x100_0000_0000_0000;
    pub const DAEMON: u64 = 0x200_0000_0000_0000;
    pub const WORKTREE: u64 = 0x400_0000_0000_0000;
    pub const LFS: u64 = 0x1000_0000_0000_0000;
    pub const SETTINGS: u64 = 0x2000_0000_0000_0000;
    pub const HELP: u64 = 0x4000_0000_0000_0000;
    pub const ABOUT: u64 = 0x8000_0000_0000_0000;

    /// `defaultTopMenuEntries`: Sync | CreateRepo | Clone | Commit.
    pub const DEFAULT_TOP: u64 = SYNC | CREATE_REPO | CLONE | COMMIT;
    /// `defaultExtMenuEntries`: SVNIgnore | StashApply | SubmoduleSync.
    pub const DEFAULT_EXT: u64 = SVN_IGNORE | STASH_APPLY | SUBMODULE_SYNC;
}

/// How a row passes paths to the launcher (see spec §1.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathArg {
    /// `path(f)`: the first selected item, else the folder.
    First,
    /// `path(d)`: the selected folder or the background folder.
    Folder,
    /// `pathfile`: all selected items (or the folder).
    All,
    /// `pathfile(dirs)`: all selected directories (or the folder).
    AllDirs,
    /// `repostatus`: all items if more than one is selected, else `First`.
    AllOrFirst,
    None,
}

/// Special behaviour beyond "launch `/command:`".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Special {
    None,
    Commit,
    Diff,
    DiffLater,
    PrevDiff,
    ConflictEditor,
    Rename,
    Remove,
    SubmoduleBkPath,
    IgnoreSub,
    DeleteIgnoreSub,
    Lfs,
    ApplyPatch,
}

type Cond = (ItemFlags, ItemFlags);

struct Row {
    bit: u64,
    label: &'static str,
    help: &'static str,
    command: &'static str,
    args: &'static [&'static str],
    path: PathArg,
    conds: &'static [Cond],
    special: Special,
}

enum Entry {
    Sep,
    Item(Row),
}

// Positional on purpose: each table row reads like a `menuInfo[]` entry.
#[allow(clippy::too_many_arguments)]
const fn item(
    bit: u64,
    label: &'static str,
    help: &'static str,
    command: &'static str,
    args: &'static [&'static str],
    path: PathArg,
    conds: &'static [Cond],
    special: Special,
) -> Entry {
    Entry::Item(Row {
        bit,
        label,
        help,
        command,
        args,
        path,
        conds,
        special,
    })
}

use PathArg as P;
use Special as S;
use bits as B;

const LOG_COND: &[Cond] = &[
    (INGIT | ONLYONE, ADDED),
    (FOLDER | FOLDERINGIT | ONLYONE, ADDED),
    (FOLDERINGIT | ONLYONE, ADDED),
    (BAREREPO, 0),
];
const ALWAYS: &[Cond] = &[(FOLDER, 0), (0, FOLDER)];
const FOLDER_ONE: &[Cond] = &[(FOLDERINGIT | ONLYONE, 0)];
const FOLDER_ONE_OR_BARE: &[Cond] = &[(FOLDERINGIT | ONLYONE, 0), (BAREREPO, 0)];
const NOT_BUSY: ItemFlags = BISECT | MERGEACTIVE;
const STASHED: &[Cond] = &[(FOLDERINGIT | ONLYONE | STASH, 0)];
const BISECTING: &[Cond] = &[(FOLDERINGIT | ONLYONE | BISECT, 0)];

/// `menuInfo[]` in table order (`MenuInfo.cpp:27-258`).
static TABLE: &[Entry] = &[
    item(
        B::INACCESSIBLE,
        "Inaccessible...",
        "Shows information about why the repository is inaccessible",
        "inaccessible",
        &[],
        P::Folder,
        &[(INACCESSIBLE, INGIT | FOLDERINGIT | BAREREPO)],
        S::None,
    ),
    item(
        B::CLONE,
        "Clone...",
        "Clones a repository into a new folder",
        "clone",
        &[],
        P::Folder,
        &[
            (FOLDER, INGIT | FOLDERINGIT | BAREREPO | INACCESSIBLE),
            (FOLDER | IGNORED, 0),
            (FOLDER | EXTENDED, 0),
        ],
        S::None,
    ),
    item(
        B::PULL,
        "&Pull...",
        "Pulls changes from a remote repository",
        "pull",
        &[],
        P::AllDirs,
        &[(FOLDERINGIT | ONLYONE, NOT_BUSY), (WCROOT, NOT_BUSY)],
        S::None,
    ),
    item(
        B::FETCH,
        "F&etch...",
        "Fetches changes from a remote repository",
        "fetch",
        &[],
        P::AllDirs,
        &[(FOLDERINGIT | ONLYONE, 0), (BAREREPO, 0), (WCROOT, 0)],
        S::None,
    ),
    item(
        B::PUSH,
        "Pus&h...",
        "Pushes local changes to a remote repository",
        "push",
        &[],
        P::AllDirs,
        &[(FOLDERINGIT | ONLYONE, 0), (BAREREPO, 0), (WCROOT, 0)],
        S::None,
    ),
    item(
        B::SYNC,
        "Sync...",
        "Synchronizes with a remote repository",
        "sync",
        &[],
        P::Folder,
        &[(FOLDERINGIT | ONLYONE, NOT_BUSY)],
        S::None,
    ),
    Entry::Sep,
    item(
        B::COMMIT,
        "&Commit...",
        "Commits your changes to the repository",
        "commit",
        &[],
        P::All,
        &[(INGIT, 0), (FOLDERINGIT, 0)],
        S::Commit,
    ),
    item(
        B::SVN_DCOMMIT,
        "SVN DCommit...",
        "Commits your changes to the SVN repository",
        "svndcommit",
        &[],
        P::First,
        &[(INGIT | GITSVN, NOT_BUSY), (FOLDERINGIT | GITSVN, NOT_BUSY)],
        S::None,
    ),
    item(
        B::SVN_REBASE,
        "SVN Rebase",
        "Rebases onto the SVN repository",
        "svnrebase",
        &[],
        P::Folder,
        &[(FOLDERINGIT | GITSVN | ONLYONE, NOT_BUSY)],
        S::None,
    ),
    item(
        B::SVN_FETCH,
        "SVN Fetch",
        "Fetches from the SVN repository",
        "svnfetch",
        &[],
        P::Folder,
        &[(FOLDERINGIT | GITSVN | ONLYONE, 0)],
        S::None,
    ),
    item(
        B::SVN_IGNORE,
        "Import SVN Ignore ...",
        "Imports svn:ignore properties",
        "svnignore",
        &[],
        P::Folder,
        &[(FOLDERINGIT | GITSVN | ONLYONE, 0)],
        S::None,
    ),
    Entry::Sep,
    item(
        B::DIFF,
        "&Diff",
        "Shows the changes you made",
        "diff",
        &[],
        P::First,
        &[(INGIT | ONLYONE, 0), (TWO, FOLDER)],
        S::Diff,
    ),
    item(
        B::DIFF_LATER,
        "Diff later",
        "Remembers this file to diff it later",
        "diff",
        &[],
        P::First,
        &[(ONLYONE, FOLDER)],
        S::DiffLater,
    ),
    item(
        B::PREV_DIFF,
        "&Diff with previous version",
        "Compares with the previous version",
        "prevdiff",
        &[],
        P::First,
        &[(INGIT | ONLYONE, ADDED)],
        S::PrevDiff,
    ),
    Entry::Sep,
    item(
        B::LOG,
        "Show &log",
        "Shows the log",
        "log",
        &[],
        P::First,
        LOG_COND,
        S::None,
    ),
    item(
        B::LOG_SUBMODULE,
        "Show log of this folder",
        "Shows the log of this folder in the superproject",
        "log",
        &["submodule"],
        P::First,
        &[(FOLDERINGIT | ONLYONE | WCROOT | SUBMODULE, 0)],
        S::None,
    ),
    item(
        B::REF_LOG,
        "Show &RefLog",
        "Shows the reflog",
        "reflog",
        &[],
        P::Folder,
        FOLDER_ONE_OR_BARE,
        S::None,
    ),
    item(
        B::REF_BROWSER,
        "Browse References",
        "Browses the references",
        "refbrowse",
        &[],
        P::Folder,
        FOLDER_ONE_OR_BARE,
        S::None,
    ),
    item(
        B::DAEMON,
        "Daemon",
        "Starts git daemon for this repository",
        "daemon",
        &[],
        P::First,
        LOG_COND,
        S::None,
    ),
    item(
        B::REVISION_GRAPH,
        "Revision &graph",
        "Shows the revision graph",
        "revisiongraph",
        &[],
        P::First,
        LOG_COND,
        S::None,
    ),
    item(
        B::REPO_BROWSER,
        "&Repo-browser",
        "Browses the repository",
        "repobrowser",
        &[],
        P::Folder,
        &[(FOLDERINGIT | ONLYONE, 0), (BAREREPO | ONLYONE, 0)],
        S::None,
    ),
    item(
        B::SHOW_CHANGED,
        "Check for modi&fications",
        "Shows all changed files",
        "repostatus",
        &[],
        P::AllOrFirst,
        &[(INGIT, 0), (FOLDER | FOLDERINGIT, 0)],
        S::None,
    ),
    item(
        B::REBASE,
        "Rebase...",
        "Rebases the current branch",
        "rebase",
        &[],
        P::Folder,
        &[(FOLDERINGIT | ONLYONE, NOT_BUSY)],
        S::None,
    ),
    item(
        B::STASH_SAVE,
        "Stash changes",
        "Stashes away the local changes",
        "stashsave",
        &[],
        P::First,
        &[(INGIT | ONLYONE, MERGEACTIVE)],
        S::None,
    ),
    item(
        B::STASH_APPLY,
        "Stash Apply",
        "Applies the latest stash",
        "stashapply",
        &[],
        P::Folder,
        STASHED,
        S::None,
    ),
    item(
        B::STASH_POP,
        "Stash Pop",
        "Applies and drops the latest stash",
        "stashpop",
        &[],
        P::Folder,
        STASHED,
        S::None,
    ),
    item(
        B::STASH_LIST,
        "Stash List",
        "Lists the stashes",
        "reflog",
        &["ref:refs/stash"],
        P::Folder,
        STASHED,
        S::None,
    ),
    Entry::Sep,
    item(
        B::BISECT,
        "Bisect start",
        "Starts a bisect session",
        "bisect",
        &["start"],
        P::Folder,
        &[(FOLDERINGIT | ONLYONE, NOT_BUSY)],
        S::None,
    ),
    item(
        B::BISECT,
        "Bisect good",
        "Marks the current revision as good",
        "bisect",
        &["good"],
        P::Folder,
        BISECTING,
        S::None,
    ),
    item(
        B::BISECT,
        "Bisect bad",
        "Marks the current revision as bad",
        "bisect",
        &["bad"],
        P::Folder,
        BISECTING,
        S::None,
    ),
    item(
        B::BISECT,
        "Bisect skip",
        "Skips the current revision",
        "bisect",
        &["skip"],
        P::Folder,
        BISECTING,
        S::None,
    ),
    item(
        B::BISECT,
        "Bisect reset",
        "Ends the bisect session",
        "bisect",
        &["reset"],
        P::Folder,
        BISECTING,
        S::None,
    ),
    Entry::Sep,
    item(
        B::CONFLICT_EDITOR,
        "&Edit conflicts",
        "Opens the merge tool to resolve conflicts",
        "conflicteditor",
        &[],
        P::First,
        &[(INGIT | CONFLICTED, FOLDER)],
        S::ConflictEditor,
    ),
    item(
        B::RESOLVE,
        "Res&olve...",
        "Marks conflicted files as resolved",
        "resolve",
        &[],
        P::All,
        &[
            (INGIT | CONFLICTED, 0),
            (INGIT | FOLDER, 0),
            (FOLDERINGIT, 0),
        ],
        S::None,
    ),
    item(
        B::MERGE,
        "Abort &Merge",
        "Aborts the merge in progress",
        "merge",
        &["abort"],
        P::First,
        &[(INGIT | MERGEACTIVE, 0), (FOLDERINGIT | MERGEACTIVE, 0)],
        S::None,
    ),
    item(
        B::RENAME,
        "Re&name...",
        "Renames files or folders",
        "rename",
        &[],
        P::First,
        &[
            (INGIT | ONLYONE | INVERSIONEDFOLDER, WCROOT),
            (WCROOT | SUBMODULE, 0),
        ],
        S::Rename,
    ),
    item(
        B::REMOVE,
        "&Delete",
        "Deletes files or folders from version control",
        "remove",
        &[],
        P::All,
        &[
            (INGIT | INVERSIONEDFOLDER, ADDED | WCROOT),
            (FOLDERINGIT | WCROOT | SUBMODULE, 0),
        ],
        S::Remove,
    ),
    item(
        B::REMOVE_KEEP,
        "&Delete (keep local)",
        "Removes from version control but keeps the local files",
        "remove",
        &["keep"],
        P::All,
        &[(INGIT | INVERSIONEDFOLDER, ADDED | WCROOT)],
        S::None,
    ),
    item(
        B::REVERT,
        "Re&vert...",
        "Reverts all changes you made",
        "revert",
        &[],
        P::All,
        &[(INGIT, NORMAL), (FOLDERINGIT, 0)],
        S::None,
    ),
    item(
        B::CLEANUP,
        "&Clean up...",
        "Removes untracked files from the working tree",
        "cleanup",
        &[],
        P::All,
        &[(FOLDERINGIT | FOLDER, 0)],
        S::None,
    ),
    Entry::Sep,
    item(
        B::LFS,
        "LFS",
        "Git LFS commands",
        "",
        &[],
        P::None,
        &[(INVERSIONEDFOLDER | INGIT, 0)],
        S::Lfs,
    ),
    Entry::Sep,
    item(
        B::SWITCH,
        "S&witch/Checkout...",
        "Switches to another branch",
        "switch",
        &[],
        P::Folder,
        FOLDER_ONE,
        S::None,
    ),
    item(
        B::MERGE,
        "&Merge...",
        "Merges another branch into the current branch",
        "merge",
        &[],
        P::Folder,
        &[(FOLDERINGIT | ONLYONE, NOT_BUSY)],
        S::None,
    ),
    item(
        B::BRANCH,
        "Create &Branch...",
        "Creates a new branch",
        "branch",
        &[],
        P::Folder,
        FOLDER_ONE,
        S::None,
    ),
    item(
        B::TAG,
        "Create &Tag...",
        "Creates a new tag",
        "tag",
        &[],
        P::Folder,
        FOLDER_ONE,
        S::None,
    ),
    item(
        B::EXPORT,
        "E&xport...",
        "Exports the repository to an archive",
        "export",
        &[],
        P::Folder,
        FOLDER_ONE_OR_BARE,
        S::None,
    ),
    Entry::Sep,
    item(
        B::CREATE_REPO,
        "Create repositor&y here...",
        "Creates a repository in this folder",
        "repocreate",
        &[],
        P::Folder,
        &[
            (FOLDER, INGIT | FOLDERINGIT | BAREREPO | INACCESSIBLE),
            (FOLDER | IGNORED, 0),
            (FOLDER | EXTENDED, INGIT),
        ],
        S::None,
    ),
    item(
        B::ADD,
        "&Add...",
        "Adds files to version control",
        "add",
        &[],
        P::All,
        &[
            (INVERSIONEDFOLDER, INGIT),
            (INGIT | FOLDER, 0),
            (IGNORED, 0),
            (DELETED, FOLDER | ONLYONE),
        ],
        S::None,
    ),
    item(
        B::BLAME,
        "&Blame",
        "Shows who changed each line",
        "blame",
        &[],
        P::First,
        &[(INGIT | ONLYONE, FOLDER | ADDED)],
        S::None,
    ),
    item(
        B::IGNORE,
        "Add to &ignore list",
        "Adds the selected items to .gitignore",
        "ignore",
        &[],
        P::All,
        &[(INVERSIONEDFOLDER, IGNORED | INGIT | WCROOT)],
        S::IgnoreSub,
    ),
    item(
        B::IGNORE,
        "Delete and add to &ignore list",
        "Deletes the selected items and adds them to .gitignore",
        "ignore",
        &["delete"],
        P::All,
        &[(INVERSIONEDFOLDER | INGIT, IGNORED | WCROOT)],
        S::DeleteIgnoreSub,
    ),
    Entry::Sep,
    item(
        B::WORKTREE,
        "Worktrees",
        "Lists the worktrees",
        "worktreelist",
        &[],
        P::Folder,
        FOLDER_ONE_OR_BARE,
        S::None,
    ),
    item(
        B::SUBMODULE_ADD,
        "Submodule Add...",
        "Adds a submodule",
        "subadd",
        &[],
        P::Folder,
        FOLDER_ONE,
        S::None,
    ),
    item(
        B::SUBMODULE_UPDATE,
        "Submodule &Update...",
        "Updates submodules",
        "subupdate",
        &[],
        P::All,
        &[(FOLDERINGIT | SUBMODULECONTAINER, 0)],
        S::SubmoduleBkPath,
    ),
    item(
        B::SUBMODULE_SYNC,
        "Submodule Sync",
        "Synchronizes submodule URLs",
        "subsync",
        &[],
        P::All,
        &[(FOLDERINGIT | SUBMODULECONTAINER, 0)],
        S::SubmoduleBkPath,
    ),
    Entry::Sep,
    item(
        B::FORMAT_PATCH,
        "Create Patch Serial...",
        "Creates patch files",
        "formatpatch",
        &[],
        P::Folder,
        FOLDER_ONE,
        S::None,
    ),
    item(
        B::IMPORT_PATCH,
        "Apply Patch Serial...",
        "Applies patch files",
        "importpatch",
        &[],
        P::All,
        &[(PATCHFILE, 0), (FOLDERINGIT | ONLYONE, 0)],
        S::None,
    ),
    item(
        B::APPLY_PATCH,
        "Review/apply single &patch...",
        "Reviews and applies a patch file",
        "",
        &[],
        P::First,
        &[(PATCHFILE | ONLYONE, 0), (EXTENDED | ONLYONE, FOLDER)],
        S::ApplyPatch,
    ),
    item(
        B::SENDMAIL,
        "Send Mail...",
        "Sends patches by mail",
        "sendmail",
        &[],
        P::All,
        &[(PATCHFILE, 0), (EXTENDED, FOLDER)],
        S::None,
    ),
    Entry::Sep,
    item(
        B::SETTINGS,
        "&Settings",
        "Opens the settings",
        "settings",
        &[],
        P::First,
        &[(FOLDER, 0), (0, FOLDER)],
        S::None,
    ),
    item(
        B::HELP,
        "&Help",
        "Opens the help",
        "help",
        &[],
        P::None,
        ALWAYS,
        S::None,
    ),
    item(
        B::ABOUT,
        "A&bout",
        "Shows information about KoopaGit",
        "about",
        &[],
        P::None,
        ALWAYS,
        S::None,
    ),
];

/// `ShouldInsertItem`: a row is shown if any `(yes, no)` pair matches.
fn should_insert(conds: &[Cond], flags: ItemFlags) -> bool {
    conds.iter().any(|&(yes, no)| match (yes, no) {
        (0, 0) => false,
        (y, 0) => flags & y == y,
        (0, n) => flags & n == 0,
        (y, n) => flags & y == y && flags & n == 0,
    })
}

/// What clicking an entry does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Start the launcher with this invocation.
    Launch(Invocation),
    /// Start the diff/merge viewer (`TortoiseGitMerge`) with these arguments.
    LaunchMerge(Vec<String>),
    /// "Diff later": remember this path (stored in the `DiffLater` setting).
    RememberDiffPath(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Item {
        /// Label with Windows `&` accelerators (use [`strip_accelerators`] where unsupported).
        label: String,
        /// Status-bar help text.
        help: String,
        action: Action,
    },
    Submenu {
        label: String,
        children: Vec<Node>,
    },
    Separator,
}

/// The menu to insert into the file manager's context menu.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContextMenu {
    /// Entries placed directly in the file manager's menu (already `Git `-prefixed).
    pub top: Vec<Node>,
    /// Entries of the "KoopaGit" submenu (named [`SUBMENU_LABEL`]).
    pub submenu: Vec<Node>,
}

pub const SUBMENU_LABEL: &str = "&KoopaGit";

/// Masks and stored state that influence the menu.
#[derive(Debug, Clone)]
pub struct MenuSettings {
    pub top: u64,
    pub extended_only: u64,
    pub hidden: u64,
    /// Path remembered by "Diff later".
    pub diff_later: Option<PathBuf>,
}

impl Default for MenuSettings {
    fn default() -> Self {
        MenuSettings {
            top: bits::DEFAULT_TOP,
            extended_only: bits::DEFAULT_EXT,
            hidden: 0,
            diff_later: None,
        }
    }
}

pub const DIFF_LATER_KEY: &str = "DiffLater";

impl MenuSettings {
    pub fn from_settings(s: &Settings) -> Self {
        let pair = |lo: &str, hi: &str, def: u64| {
            u64::from(s.dword(lo, def as u32)) | (u64::from(s.dword(hi, (def >> 32) as u32)) << 32)
        };
        let diff_later = s.string(DIFF_LATER_KEY, "");
        MenuSettings {
            top: pair(
                keys::CONTEXT_MENU_ENTRIES,
                keys::CONTEXT_MENU_ENTRIES_HIGH,
                bits::DEFAULT_TOP,
            ),
            extended_only: pair(
                keys::CONTEXT_MENU_EXT_ENTRIES_LOW,
                keys::CONTEXT_MENU_EXT_ENTRIES_HIGH,
                bits::DEFAULT_EXT,
            ),
            hidden: pair("ContextMenuEntriesMaskLow", "ContextMenuEntriesMaskHigh", 0),
            diff_later: (!diff_later.is_empty()).then(|| PathBuf::from(diff_later)),
        }
    }
}

/// The analysed selection: flags plus the paths commands receive.
#[derive(Debug, Clone, Default)]
pub struct Selection {
    pub flags: ItemFlags,
    /// Selected items (empty for a folder-background click).
    pub items: Vec<PathBuf>,
    /// The selected single folder, or the background folder (`folder_`).
    pub folder: Option<PathBuf>,
    /// Current branch for the "Commit -> "branch"..." label.
    pub branch: Option<String>,
    /// Detached HEAD id, used when `branch` is `None`.
    pub head_id: Option<String>,
    pub is_submodule: bool,
    /// Whether `.git/lfs` exists.
    pub has_lfs: bool,
}

impl Selection {
    fn first(&self) -> Option<&Path> {
        self.items
            .first()
            .map(PathBuf::as_path)
            .or(self.folder.as_deref())
    }

    fn all(&self, dirs_only: bool) -> Vec<PathBuf> {
        let v: Vec<PathBuf> = self
            .items
            .iter()
            .filter(|p| !dirs_only || p.is_dir())
            .cloned()
            .collect();
        if v.is_empty() {
            self.folder.iter().cloned().collect()
        } else {
            v
        }
    }
}

/// Builds the context menu like `QueryContextMenu`: table order, lazy separators,
/// top-level vs submenu placement, Shift-only and hidden masks.
pub fn build(sel: &Selection, ms: &MenuSettings) -> ContextMenu {
    let flags = sel.flags;
    let mut menu = ContextMenu::default();
    let mut add_separator = false;
    let mut entry_added = false;
    for entry in TABLE {
        let row = match entry {
            Entry::Sep => {
                if !menu.submenu.is_empty() || entry_added {
                    add_separator = true;
                }
                continue;
            }
            Entry::Item(row) => row,
        };
        if row.bit & ms.extended_only != 0 && flags & EXTENDED == 0 && row.bit != bits::INACCESSIBLE
        {
            continue;
        }
        if row.bit & ms.hidden == row.bit {
            continue;
        }
        if !should_insert(row.conds, flags) {
            continue;
        }
        let is_top = row.bit & ms.top != 0 || row.bit == bits::INACCESSIBLE;
        let Some(node) = make_node(row, sel, ms) else {
            continue;
        };
        if is_top {
            menu.top.push(prefix_git(node));
        } else {
            if add_separator && entry_added && !menu.submenu.is_empty() {
                menu.submenu.push(Node::Separator);
            }
            add_separator = false;
            menu.submenu.push(node);
        }
        entry_added = true;
    }
    menu
}

fn prefix_git(node: Node) -> Node {
    match node {
        Node::Item {
            label,
            help,
            action,
        } => Node::Item {
            label: format!("Git {label}"),
            help,
            action,
        },
        Node::Submenu { label, children } => Node::Submenu {
            label: format!("Git {label}"),
            children,
        },
        Node::Separator => Node::Separator,
    }
}

fn launch(row: &Row, sel: &Selection) -> Invocation {
    let mut inv = Invocation::new(row.command);
    let paths: Vec<PathBuf> = match row.path {
        P::First => sel.first().map(Path::to_owned).into_iter().collect(),
        P::Folder => sel.folder.iter().cloned().collect(),
        P::All => sel.all(false),
        P::AllDirs => sel.all(true),
        P::AllOrFirst if sel.items.len() > 1 => sel.all(false),
        P::AllOrFirst => sel.first().map(Path::to_owned).into_iter().collect(),
        P::None => Vec::new(),
    };
    if !paths.is_empty() {
        inv = inv.with_paths(paths);
    }
    for a in row.args {
        inv = match a.split_once(':') {
            Some((k, v)) => inv.with(k, v),
            None => inv.with_switch(a),
        };
    }
    inv
}

fn make_node(row: &Row, sel: &Selection, ms: &MenuSettings) -> Option<Node> {
    let flags = sel.flags;
    let mut label = row.label.to_owned();
    let mut inv = launch(row, sel);
    let action = match row.special {
        S::Commit => {
            if sel.is_submodule {
                label = "&Commit submodule...".to_owned();
            }
            let shown = match (&sel.branch, &sel.head_id) {
                (Some(b), _) if b.chars().count() > 64 => {
                    Some(format!("{}...", b.chars().take(64).collect::<String>()))
                }
                (Some(b), _) => Some(b.clone()),
                (None, Some(id)) => Some(id.chars().take(8).collect()),
                _ => None,
            };
            if let Some(name) = shown {
                let base = label.trim_end_matches("...");
                label = format!("{base} -> \"{}\"...", name.replace('&', "&&"));
            }
            Action::Launch(inv)
        }
        S::Diff => {
            if sel.items.len() == 2 {
                inv = Invocation::new("diff")
                    .with("path", sel.items[0].to_string_lossy())
                    .with("path2", sel.items[1].to_string_lossy());
            }
            if flags & EXTENDED != 0 {
                inv = inv.with_switch("alternative");
            }
            Action::Launch(inv)
        }
        S::PrevDiff | S::ConflictEditor => {
            if flags & EXTENDED != 0 {
                inv = inv.with_switch("alternative");
            }
            Action::Launch(inv)
        }
        S::DiffLater => {
            let first = sel.first()?.to_owned();
            match &ms.diff_later {
                Some(stored) => {
                    label = format!("Diff with \"{}\"", compact_path(stored, 39));
                    let mut inv = Invocation::new("diff")
                        .with("path", first.to_string_lossy())
                        .with("path2", stored.to_string_lossy());
                    if flags & EXTENDED != 0 {
                        inv = inv.with_switch("alternative");
                    }
                    Action::Launch(inv)
                }
                None => Action::RememberDiffPath(first),
            }
        }
        S::Rename | S::Remove => {
            if flags & SUBMODULE != 0 {
                inv = inv.with_switch("submodule");
            }
            Action::Launch(inv)
        }
        S::SubmoduleBkPath => {
            if flags & SUBMODULECONTAINER != 0
                && let Some(f) = &sel.folder
            {
                inv = inv.with("bkpath", f.to_string_lossy());
            }
            Action::Launch(inv)
        }
        S::IgnoreSub | S::DeleteIgnoreSub => return ignore_submenu(row, sel),
        S::Lfs => return lfs_submenu(sel),
        S::ApplyPatch => {
            let first = sel.first()?;
            let mut args = Vec::new();
            if flags & PATCHFILE != 0 {
                args.push(format!("/diff:{}", first.to_string_lossy()));
                if flags & FOLDERINGIT != 0
                    && let Some(f) = &sel.folder
                {
                    args.push(format!("/patchpath:{}", f.to_string_lossy()));
                }
                if flags & INVERSIONEDFOLDER != 0 {
                    args.push("/wc".to_owned());
                }
            } else {
                args.push(format!("/patchpath:{}", first.to_string_lossy()));
            }
            Action::LaunchMerge(args)
        }
        S::None => Action::Launch(inv),
    };
    Some(Node::Item {
        label,
        help: row.help.to_owned(),
        action,
    })
}

/// `InsertIgnoreSubmenus`: by name / by extension, single or multiple items.
fn ignore_submenu(row: &Row, sel: &Selection) -> Option<Node> {
    if sel.items.is_empty() {
        return None;
    }
    let flags = sel.flags;
    if flags & IGNORED != 0 {
        // Upstream's unignore branch never has data, so no popup appears.
        return None;
    }
    let in_git = flags & INGIT != 0;
    // Only the popup matching the item's state is shown.
    if (row.special == S::DeleteIgnoreSub) != in_git {
        return None;
    }
    let base = Invocation::new("ignore").with_paths(sel.items.clone());
    let base = if in_git {
        base.with_switch("delete")
    } else {
        base
    };
    let mut children = Vec::new();
    if flags & ONLYONE != 0 {
        let item = &sel.items[0];
        let name = item.file_name()?.to_string_lossy().into_owned();
        children.push(Node::Item {
            label: name,
            help: row.help.to_owned(),
            action: Action::Launch(base.clone()),
        });
        if !item.is_dir()
            && let Some(ext) = item.extension()
        {
            children.push(Node::Item {
                label: format!("*.{}", ext.to_string_lossy()),
                help: row.help.to_owned(),
                action: Action::Launch(base.with_switch("onlymask")),
            });
        }
    } else {
        let n = sel.items.len();
        let count = if n >= 16 {
            "multiple".to_owned()
        } else {
            n.to_string()
        };
        let (by_name, by_ext) = if in_git {
            (
                format!("Delete and &ignore {count} items by name"),
                format!("Delete and ignore {count} items by &extension"),
            )
        } else {
            (
                format!("&Ignore {count} items by name"),
                format!("Ignore {count} items by &extension"),
            )
        };
        children.push(Node::Item {
            label: by_name,
            help: row.help.to_owned(),
            action: Action::Launch(base.clone()),
        });
        children.push(Node::Item {
            label: by_ext,
            help: row.help.to_owned(),
            action: Action::Launch(base.with_switch("onlymask")),
        });
    }
    Some(Node::Submenu {
        label: row.label.to_owned(),
        children,
    })
}

fn lfs_submenu(sel: &Selection) -> Option<Node> {
    if !sel.has_lfs {
        return None;
    }
    let flags = sel.flags;
    let all = sel.all(false);
    let mut children = Vec::new();
    if should_insert(&[(FOLDERINGIT | ONLYONE, 0)], flags) {
        children.push(("Show locked files", "lfslocks"));
    }
    if should_insert(&[(INGIT | INVERSIONEDFOLDER, FOLDER)], flags) {
        children.push(("Lock", "lfslock"));
        children.push(("Unlock", "lfsunlock"));
    }
    if children.is_empty() {
        return None;
    }
    Some(Node::Submenu {
        label: "LFS".to_owned(),
        children: children
            .into_iter()
            .map(|(label, cmd)| Node::Item {
                label: label.to_owned(),
                help: String::new(),
                action: Action::Launch(Invocation::new(cmd).with_paths(all.clone())),
            })
            .collect(),
    })
}

/// The rows of the settings dialog's "Context Menu" checklists, in table order:
/// `(bit, label without accelerators)`. Separators, Abort Merge, Inaccessible
/// and consecutive rows sharing a bit are left out (`SetLookAndFeelPage.cpp:30-59`).
pub fn settings_entries() -> Vec<(u64, String)> {
    let mut out: Vec<(u64, String)> = Vec::new();
    for entry in TABLE {
        let Entry::Item(row) = entry else { continue };
        if row.bit == bits::INACCESSIBLE || (row.bit == bits::MERGE && row.args == ["abort"]) {
            continue;
        }
        if out.last().is_some_and(|(b, _)| *b == row.bit) {
            continue;
        }
        out.push((row.bit, strip_accelerators(row.label)));
    }
    out
}

/// Removes Windows `&` accelerator markers (`&&` becomes `&`).
pub fn strip_accelerators(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut chars = label.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            if chars.peek() == Some(&'&') {
                out.push('&');
                chars.next();
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// `PathCompactPathEx`-like shortening keeping the file name.
fn compact_path(p: &Path, max: usize) -> String {
    let s = p.to_string_lossy();
    if s.chars().count() <= max {
        return s.into_owned();
    }
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let keep = max.saturating_sub(name.chars().count() + 4);
    let head: String = s.chars().take(keep).collect();
    format!("{head}...{}{name}", std::path::MAIN_SEPARATOR)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(nodes: &[Node]) -> Vec<String> {
        nodes
            .iter()
            .map(|n| match n {
                Node::Item { label, .. } => strip_accelerators(label),
                Node::Submenu { label, .. } => format!("[{}]", strip_accelerators(label)),
                Node::Separator => "---".to_owned(),
            })
            .collect()
    }

    fn sel(flags: ItemFlags) -> Selection {
        Selection {
            flags,
            folder: Some(PathBuf::from("/repo")),
            branch: Some("main".into()),
            ..Default::default()
        }
    }

    #[test]
    fn folder_outside_repo_offers_clone_and_create() {
        let m = build(&sel(FOLDER | ONLYONE), &MenuSettings::default());
        assert_eq!(
            labels(&m.top),
            ["Git Clone...", "Git Create repository here..."]
        );
        assert_eq!(labels(&m.submenu), ["Settings", "Help", "About"]);
    }

    #[test]
    fn repo_background_menu_order_and_separators() {
        let flags = INGIT | INVERSIONEDFOLDER | FOLDER | FOLDERINGIT | WCROOT | ONLYONE | NORMAL;
        let m = build(&sel(flags), &MenuSettings::default());
        assert_eq!(labels(&m.top), ["Git Sync...", "Git Commit -> \"main\"..."]);
        assert_eq!(
            labels(&m.submenu),
            [
                "Pull...",
                "Fetch...",
                "Push...",
                "---",
                "Diff",
                "Diff with previous version",
                "---",
                "Show log",
                "Show RefLog",
                "Browse References",
                "Daemon",
                "Revision graph",
                "Repo-browser",
                "Check for modifications",
                "Rebase...",
                "Stash changes",
                "---",
                "Bisect start",
                // Rename/Delete are excluded on the work-tree root (-WCROOT).
                "---",
                "Resolve...",
                "Revert...",
                "Clean up...",
                "---",
                "Switch/Checkout...",
                "Merge...",
                "Create Branch...",
                "Create Tag...",
                "Export...",
                "---",
                "Add...",
                "---",
                "Worktrees",
                "Submodule Add...",
                "---",
                "Create Patch Serial...",
                "Apply Patch Serial...",
                "---",
                "Settings",
                "Help",
                "About",
            ]
        );
    }

    #[test]
    fn shift_reveals_extended_entries() {
        let flags = INGIT | INVERSIONEDFOLDER | FOLDER | FOLDERINGIT | ONLYONE | STASH;
        let normal = labels(&build(&sel(flags), &MenuSettings::default()).submenu);
        assert!(normal.contains(&"Stash Pop".to_owned()));
        assert!(!normal.contains(&"Stash Apply".to_owned()));
        let ext = labels(&build(&sel(flags | EXTENDED), &MenuSettings::default()).submenu);
        assert!(ext.contains(&"Stash Apply".to_owned()));
    }

    #[test]
    fn modified_file_entries_and_commands() {
        let mut s = sel(INGIT | INVERSIONEDFOLDER | ONLYONE);
        s.items = vec![PathBuf::from("/repo/a.txt")];
        s.folder = Some(PathBuf::from("/repo"));
        let m = build(&s, &MenuSettings::default());
        let sub = labels(&m.submenu);
        assert!(sub.contains(&"Blame".to_owned()));
        assert!(sub.contains(&"[Delete and add to ignore list]".to_owned()));
        let log = m
            .submenu
            .iter()
            .find_map(|n| match n {
                Node::Item {
                    label,
                    action: Action::Launch(inv),
                    ..
                } if label == "Show &log" => Some(inv.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(log.to_args(), ["/command:log", "/path:/repo/a.txt"]);
        let ignore = m
            .submenu
            .iter()
            .find_map(|n| match n {
                Node::Submenu { children, .. } => Some(children.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(labels(&ignore), ["a.txt", "*.txt"]);
    }

    #[test]
    fn masks_move_and_hide_entries() {
        let flags = INGIT | INVERSIONEDFOLDER | FOLDER | FOLDERINGIT | ONLYONE;
        let ms = MenuSettings {
            top: bits::LOG,
            hidden: bits::ABOUT,
            ..Default::default()
        };
        let m = build(&sel(flags), &ms);
        assert_eq!(labels(&m.top), ["Git Show log"]);
        assert!(!labels(&m.submenu).contains(&"About".to_owned()));
        assert!(labels(&m.submenu).contains(&"Sync...".to_owned()));
    }

    #[test]
    fn diff_later_round_trip() {
        let mut s = sel(INGIT | ONLYONE);
        s.items = vec![PathBuf::from("/repo/a.txt")];
        let m = build(&s, &MenuSettings::default());
        assert!(m.submenu.iter().any(|n| matches!(n,
            Node::Item { action: Action::RememberDiffPath(p), .. } if p == Path::new("/repo/a.txt"))));
        let ms = MenuSettings {
            diff_later: Some(PathBuf::from("/x/b.txt")),
            ..Default::default()
        };
        let m = build(&s, &ms);
        assert!(labels(&m.submenu).contains(&"Diff with \"/x/b.txt\"".to_owned()));
    }

    #[test]
    fn settings_checklist_rows() {
        let rows = settings_entries();
        let labels: Vec<&str> = rows.iter().map(|(_, l)| l.as_str()).collect();
        assert_eq!(&labels[..3], ["Clone...", "Pull...", "Fetch..."]);
        assert_eq!(labels.iter().filter(|l| l.starts_with("Bisect")).count(), 1);
        assert!(!labels.contains(&"Abort Merge"));
        assert!(labels.contains(&"Merge..."));
        assert_eq!(
            labels.iter().filter(|l| l.contains("ignore list")).count(),
            1
        );
        assert_eq!(labels.last(), Some(&"About"));
    }

    #[test]
    fn settings_masks_parse() {
        let mut st = Settings::default();
        st.set(keys::CONTEXT_MENU_ENTRIES, 0x406);
        st.set(keys::CONTEXT_MENU_ENTRIES_HIGH, 0x20);
        assert_eq!(MenuSettings::from_settings(&st).top, bits::DEFAULT_TOP);
        assert_eq!(
            MenuSettings::from_settings(&Settings::default()).extended_only,
            bits::DEFAULT_EXT
        );
        assert_eq!(strip_accelerators("A&&B &x"), "A&B x");
    }
}
