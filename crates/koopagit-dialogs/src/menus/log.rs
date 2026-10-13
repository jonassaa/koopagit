//! The log commit-list context menu (`CGitLogListBase::OnContextMenu`,
//! spec-menus.md §2) and its clipboard formats (§2.4).
//!
//! The builder is pure: callers describe the selection and repository state in
//! [`LogMenuInput`], and get the menu entries in upstream order.

use crate::menu_actions;
use crate::menus::{ActionId, Entry, Leaf, Menu, leaf};

menu_actions! {
    /// Log-list actions. The values double as mask bits (`1 << id`), like
    /// upstream's `ID_*` enum; sub-entries encode an index as `id | (k << 16)`.
    pub enum LogAction {
        RebasePick = 1,
        RebaseSquash = 2,
        RebaseEdit = 3,
        RebaseSkip = 4,
        Compare = 5,
        Commit = 6,
        MergeAbort = 7,
        BlamePrevious = 8,
        GnuDiff1 = 9,
        CompareWithPrevious = 10,
        Blame = 11,
        StashSave = 12,
        StashPop = 13,
        StashList = 14,
        BisectGood = 15,
        BisectBad = 16,
        BisectSkip = 17,
        BisectReset = 18,
        Pull = 19,
        Fetch = 20,
        SubmoduleUpdate = 21,
        Cleanup = 22,
        Log = 23,
        RepoBrowse = 24,
        MergeRev = 25,
        Reset = 26,
        SwitchBranch = 27,
        SwitchToRev = 28,
        CreateBranch = 29,
        CreateTag = 30,
        RebaseToVersion = 31,
        Export = 32,
        RevertRev = 33,
        EditNote = 34,
        ReflogDel = 35,
        ReflogStashApply = 36,
        CompareTwo = 37,
        GnuDiff2 = 38,
        LogViewRange = 39,
        LogViewRangeReverse = 40,
        LogViewRangeReachable = 41,
        CompareTwoCommitChanges = 42,
        CombineCommit = 45,
        CherryPick = 46,
        CreatePatch = 47,
        BisectStart = 48,
        Push = 49,
        SvnDCommit = 50,
        PullHead = 51,
        Delete = 52,
        ToggleRollup = 53,
        FindEntry = 55,
        ShowBranches = 56,
        /// Mask bit of the "Copy to clipboard" submenu (children have no bits).
        CopyClipboard = 57,
        CopyFull = 58,
        CopyNoPaths = 59,
        CopySha = 60,
        CopyAuthorsFull = 61,
        CopyAuthorNames = 62,
        CopyAuthorEmails = 63,
        CopySubjects = 43,
        CopyMessages = 44,
        CopyRefNames = 54,
    }
}

/// Sub-entry index used for "All Parents" in the unified-diff submenu.
pub const ALL_PARENTS: u32 = 0xFFFF;
/// "Only Merged Files".
pub const ONLY_MERGED: u32 = 0xFFFE;
/// "Show extra changes after merge".
pub const EXTRA_AFTER_MERGE: u32 = 0xFFFD;
/// "All" in the delete submenu.
pub const DELETE_ALL: u32 = 0xFFFF;

/// Encodes a sub-entry (`ID | (k << 16)`).
pub fn sub_id(action: LogAction, k: u32) -> i32 {
    action.id() | ((k as i32) << 16)
}

/// Decodes a menu id into the action and sub-entry index.
pub fn decode(id: i32) -> Option<(LogAction, u32)> {
    LogAction::from_id(id & 0xFFFF).map(|a| (a, (id as u32) >> 16))
}

/// Which entries a list allows (`m_ContextMenuMask`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogMask(pub u64);

impl LogMask {
    fn bit(a: LogAction) -> u64 {
        1u64 << a.id()
    }

    /// Default mask: everything except the rebase actions, Log, Blame and
    /// Blame previous (LLB:94-100). Used by the Log dialog and Sync in/out lists.
    pub fn default_log() -> Self {
        LogMask(
            !(Self::bit(LogAction::RebasePick)
                | Self::bit(LogAction::RebaseSquash)
                | Self::bit(LogAction::RebaseEdit)
                | Self::bit(LogAction::RebaseSkip)
                | Self::bit(LogAction::Log)
                | Self::bit(LogAction::Blame)
                | Self::bit(LogAction::BlamePrevious)),
        )
    }

    /// Sync dialog In/Out Commits: the default mask (SyncDlg.cpp:1030-1089).
    pub fn sync() -> Self {
        Self::default_log()
    }

    /// RefLog list: default plus Log, minus Compare change sets (refloglist.cpp:32-33).
    pub fn reflog() -> Self {
        let m = Self::default_log().0 | Self::bit(LogAction::Log);
        LogMask(m & !Self::bit(LogAction::CompareTwoCommitChanges))
    }

    /// Rebase commit list (RebaseDlg.cpp:327-335, 1861-1865); `editable` while
    /// choosing commits adds Pick/Squash/Edit/Skip and Log.
    pub fn rebase(editable: bool) -> Self {
        let mut m = Self::default_log().0;
        for a in [
            LogAction::CherryPick,
            LogAction::SwitchToRev,
            LogAction::Reset,
            LogAction::RevertRev,
            LogAction::MergeRev,
            LogAction::RebaseToVersion,
            LogAction::CombineCommit,
            LogAction::ToggleRollup,
        ] {
            m &= !Self::bit(a);
        }
        if editable {
            for a in [
                LogAction::RebasePick,
                LogAction::RebaseSquash,
                LogAction::RebaseEdit,
                LogAction::RebaseSkip,
                LogAction::Log,
            ] {
                m |= Self::bit(a);
            }
        }
        LogMask(m)
    }

    pub fn has(self, a: LogAction) -> bool {
        self.0 & Self::bit(a) != 0
    }

    pub fn with(self, a: LogAction) -> Self {
        LogMask(self.0 | Self::bit(a))
    }

    pub fn without(self, a: LogAction) -> Self {
        LogMask(self.0 & !Self::bit(a))
    }
}

/// A ref on a selected commit, as a full name (`refs/heads/main`, ...).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuRef {
    pub full_name: String,
}

impl MenuRef {
    pub fn new(full_name: impl Into<String>) -> Self {
        MenuRef {
            full_name: full_name.into(),
        }
    }

    fn short(&self) -> &str {
        let n = &self.full_name;
        n.strip_prefix("refs/heads/")
            .or_else(|| n.strip_prefix("refs/remotes/"))
            .or_else(|| n.strip_prefix("refs/tags/"))
            .unwrap_or(n)
    }
}

/// The first selected row (`E`).
#[derive(Debug, Clone, Default)]
pub struct MenuCommit {
    /// Empty for the working-tree pseudo-row.
    pub id: String,
    /// Labels of the parents: `Parent %d: "<subject>" (<shorthash>)`.
    pub parent_labels: Vec<String>,
    pub refs: Vec<MenuRef>,
    pub is_head: bool,
    /// `refs/stash` or its index commit.
    pub is_stash: bool,
    /// Has a `refs/bisect/*` ref.
    pub has_bisect_ref: bool,
    /// Reflog selector (`stash@{0}`, `HEAD@{3}`) for reflog rows.
    pub reflog_ref: Option<String>,
    pub rolled_up: bool,
}

impl MenuCommit {
    pub fn is_wc(&self) -> bool {
        self.id.is_empty()
    }

    pub fn parents(&self) -> usize {
        self.parent_labels.len()
    }
}

/// Everything the menu depends on.
#[derive(Debug, Clone)]
pub struct LogMenuInput {
    pub mask: LogMask,
    /// Selected rows, top to bottom; `selected[0]` is `E`.
    pub selected: Vec<MenuCommit>,
    /// Selected rows form one contiguous run.
    pub contiguous: bool,
    /// The bottom-most list row is selected (Squash needs a commit below).
    pub bottom_selected: bool,
    /// Combine-to-one-commit precondition: contiguous run whose first and last
    /// rows are `HEAD~a` / `HEAD~b` (computed by the caller).
    pub combine_ok: bool,
    pub current_branch: Option<String>,
    pub has_wc: bool,
    pub merge_active: bool,
    pub bisect_active: bool,
    pub stash_exists: bool,
    pub has_submodules: bool,
    pub git_svn: bool,
    /// The log is for a single file (enables Blame entries when masked in).
    pub file_log: bool,
    /// Cherry-pick mode of the rebase dialog.
    pub cherry_pick_mode: bool,
    /// Merge-point filter on and no text filter (collapse/expand).
    pub rollup_possible: bool,
    /// Shift held when opening the menu.
    pub shift: bool,
    /// The list is the reflog (clipboard submenu differs).
    pub reflog: bool,
}

impl Default for LogMenuInput {
    fn default() -> Self {
        LogMenuInput {
            mask: LogMask::default_log(),
            selected: Vec::new(),
            contiguous: true,
            bottom_selected: false,
            combine_ok: false,
            current_branch: None,
            has_wc: true,
            merge_active: false,
            bisect_active: false,
            stash_exists: false,
            has_submodules: false,
            git_svn: false,
            file_log: false,
            cherry_pick_mode: false,
            rollup_possible: false,
            shift: false,
            reflog: false,
        }
    }
}

fn parents_sub(action: LogAction, labels: &[String]) -> Vec<Leaf> {
    labels
        .iter()
        .enumerate()
        .map(|(k, l)| Leaf {
            id: sub_id(action, k as u32 + 1),
            title: l.clone(),
            enabled: true,
        })
        .collect()
}

/// Builds the menu in upstream insertion order (LLB:1640-2348).
pub fn build(inp: &LogMenuInput) -> Vec<Entry> {
    let mut m = Menu::new();
    let Some(e) = inp.selected.first() else {
        return Vec::new();
    };
    let n = inp.selected.len();
    let mask = inp.mask;
    let has = |a| mask.has(a);
    let wc = e.is_wc();
    let cur = inp.current_branch.clone().unwrap_or_else(|| "HEAD".into());

    // A. Rebase actions.
    let any_rebase = [
        LogAction::RebasePick,
        LogAction::RebaseSquash,
        LogAction::RebaseEdit,
        LogAction::RebaseSkip,
    ]
    .iter()
    .any(|a| has(*a));
    m.item_if(has(LogAction::RebasePick), LogAction::RebasePick, "Pick");
    m.item_if(
        has(LogAction::RebaseSquash)
            && !inp.bottom_selected
            && (inp.cherry_pick_mode || e.parents() == 1),
        LogAction::RebaseSquash,
        "Squash (with commit below)",
    );
    m.item_if(has(LogAction::RebaseEdit), LogAction::RebaseEdit, "Edit");
    m.item_if(has(LogAction::RebaseSkip), LogAction::RebaseSkip, "Skip");
    if any_rebase {
        m.sep();
    }

    // B. Single selection.
    if n == 1 {
        m.item_if(
            !wc && has(LogAction::Compare) && inp.has_wc,
            LogAction::Compare,
            "Compare with &working tree",
        );
        m.item_if(
            wc && has(LogAction::Commit),
            LogAction::Commit,
            "C&ommit...",
        );
        m.item_if(
            wc && inp.merge_active && has(LogAction::MergeAbort),
            LogAction::MergeAbort,
            "Abort &Merge",
        );
        if has(LogAction::BlamePrevious) && inp.file_log && !wc {
            if e.parents() > 1 {
                m.sub(
                    "Blame previous revision",
                    parents_sub(LogAction::BlamePrevious, &e.parent_labels),
                );
            } else {
                m.item(LogAction::BlamePrevious, "Blame previous revision");
            }
        }
        if has(LogAction::GnuDiff1) && inp.has_wc {
            if e.parents() > 1 {
                let mut children = vec![
                    Leaf {
                        id: sub_id(LogAction::GnuDiff1, ALL_PARENTS),
                        title: "All Parents".into(),
                        enabled: true,
                    },
                    Leaf {
                        id: sub_id(LogAction::GnuDiff1, ONLY_MERGED),
                        title: "Only Merged Files".into(),
                        enabled: true,
                    },
                    Leaf {
                        id: sub_id(LogAction::GnuDiff1, EXTRA_AFTER_MERGE),
                        title: "Show extra changes after merge".into(),
                        enabled: true,
                    },
                ];
                children.extend(parents_sub(LogAction::GnuDiff1, &e.parent_labels));
                m.sub("&Unified diff with", children);
            } else {
                m.item(LogAction::GnuDiff1, "Show changes as &unified diff");
            }
        }
        if has(LogAction::CompareWithPrevious) {
            if e.parents() > 1 {
                m.sub(
                    "&Compare with previous revision",
                    parents_sub(LogAction::CompareWithPrevious, &e.parent_labels),
                );
            } else {
                m.item(
                    LogAction::CompareWithPrevious,
                    "&Compare with previous revision",
                );
            }
        }
        m.item_if(
            has(LogAction::Blame) && inp.file_log && !wc,
            LogAction::Blame,
            "Blame",
        );
        m.sep();

        m.item_if(
            wc && !inp.merge_active && has(LogAction::StashSave),
            LogAction::StashSave,
            "Stash changes",
        );
        let stash_rows = (wc || e.is_stash) && inp.stash_exists;
        m.item_if(
            stash_rows && has(LogAction::StashPop),
            LogAction::StashPop,
            "Stash Pop",
        );
        m.item_if(
            stash_rows && has(LogAction::StashList),
            LogAction::StashList,
            "Stash List",
        );
        m.sep();

        let bisect_mark = inp.bisect_active && !e.has_bisect_ref;
        m.item_if(
            bisect_mark && has(LogAction::BisectGood),
            LogAction::BisectGood,
            "Bisect good",
        );
        m.item_if(
            bisect_mark && has(LogAction::BisectBad),
            LogAction::BisectBad,
            "Bisect bad",
        );
        m.item_if(
            bisect_mark && has(LogAction::BisectSkip),
            LogAction::BisectSkip,
            "Bisect skip",
        );
        m.item_if(
            wc && inp.bisect_active && has(LogAction::BisectReset),
            LogAction::BisectReset,
            "Bisect reset",
        );
        m.sep();

        if wc {
            m.item_if(
                !inp.merge_active && has(LogAction::Pull),
                LogAction::Pull,
                "&Pull...",
            );
            m.item_if(has(LogAction::Fetch), LogAction::Fetch, "F&etch...");
            m.item_if(
                inp.has_submodules && has(LogAction::SubmoduleUpdate),
                LogAction::SubmoduleUpdate,
                "Submodule Update",
            );
            m.sep();
            m.item_if(has(LogAction::Cleanup), LogAction::Cleanup, "&Clean up...");
            m.sep();
        } else {
            let refs = !e.refs.is_empty();
            m.item_if(
                has(LogAction::Log) || (refs && !e.is_stash) || inp.shift,
                LogAction::Log,
                "Show log...",
            );
            m.item_if(
                has(LogAction::RepoBrowse),
                LogAction::RepoBrowse,
                "&Browse repository",
            );
            m.item_if(
                has(LogAction::MergeRev)
                    && !e.is_head
                    && inp.has_wc
                    && !inp.merge_active
                    && !e.is_stash,
                LogAction::MergeRev,
                format!("&Merge to \"{cur}\"..."),
            );
            m.item_if(
                has(LogAction::Reset) && inp.has_wc && !e.is_stash,
                LogAction::Reset,
                format!("R&eset \"{cur}\" to this..."),
            );
            if has(LogAction::SwitchBranch) && inp.has_wc && !e.is_stash {
                let branches: Vec<&MenuRef> = e
                    .refs
                    .iter()
                    .filter(|r| {
                        r.full_name.starts_with("refs/heads/")
                            && Some(r.short()) != inp.current_branch.as_deref()
                    })
                    .collect();
                match branches.as_slice() {
                    [] => {}
                    [one] => {
                        m.raw(
                            sub_id(LogAction::SwitchBranch, 0),
                            format!("&Switch/Checkout to \"{}\"", one.short()),
                            true,
                        );
                    }
                    many => {
                        let children = e
                            .refs
                            .iter()
                            .enumerate()
                            .filter(|(_, r)| many.iter().any(|b| b.full_name == r.full_name))
                            .map(|(k, r)| Leaf {
                                id: sub_id(LogAction::SwitchBranch, k as u32),
                                title: r.short().to_owned(),
                                enabled: true,
                            })
                            .collect();
                        m.sub("&Switch/Checkout to", children);
                    }
                }
            }
            m.item_if(
                has(LogAction::SwitchToRev) && !e.is_head && inp.has_wc && !e.is_stash,
                LogAction::SwitchToRev,
                "Sw&itch/Checkout to this...",
            );
            m.item_if(
                has(LogAction::CreateBranch) && !e.is_stash,
                LogAction::CreateBranch,
                "Create Br&anch at this version...",
            );
            m.item_if(
                has(LogAction::CreateTag) && !e.is_stash,
                LogAction::CreateTag,
                "Create &Tag at this version...",
            );
            m.item_if(
                has(LogAction::RebaseToVersion)
                    && !e.is_head
                    && inp.has_wc
                    && !inp.merge_active
                    && !e.is_stash,
                LogAction::RebaseToVersion,
                format!("Rebase \"{cur}\" onto this...(&G)"),
            );
            m.item_if(
                has(LogAction::Export),
                LogAction::Export,
                "E&xport this version...",
            );
            if has(LogAction::RevertRev) && inp.has_wc && !inp.merge_active && !e.is_stash {
                if e.parents() > 1 {
                    m.sub(
                        "&Revert change by this commit",
                        parents_sub(LogAction::RevertRev, &e.parent_labels),
                    );
                } else {
                    m.item(LogAction::RevertRev, "&Revert change by this commit");
                }
            }
            m.item_if(
                has(LogAction::EditNote) && !e.is_stash,
                LogAction::EditNote,
                "Edit &Notes",
            );
            m.sep();
        }
    }

    // C. Reflog rows.
    if let Some(r) = &e.reflog_ref {
        m.item(LogAction::ReflogDel, "Delete Ref...");
        m.item_if(
            n == 1 && r.starts_with("refs/stash"),
            LogAction::ReflogStashApply,
            "Stash Apply",
        );
        if n <= 2 {
            m.sep();
        }
    }

    // D. Two or more.
    if n >= 2 {
        m.item_if(
            (n == 2 || inp.contiguous) && has(LogAction::CompareTwo),
            LogAction::CompareTwo,
            "&Compare revisions",
        );
        m.item_if(
            n == 2 && has(LogAction::GnuDiff2) && inp.has_wc,
            LogAction::GnuDiff2,
            "Show changes as &unified diff",
        );
        let any_wc = inp.selected.iter().any(|c| c.is_wc());
        if n == 2 && !any_wc {
            let first = short(&inp.selected[0].id);
            let last = short(&inp.selected[1].id);
            m.item(
                LogAction::LogViewRange,
                format!("Show log of {last}..{first}"),
            );
            m.item(
                LogAction::LogViewRangeReverse,
                format!("Show log of {first}..{last}"),
            );
            m.item(
                LogAction::LogViewRangeReachable,
                format!("Show log of {last}...{first}"),
            );
        }
        m.item_if(
            n == 2 && has(LogAction::CompareTwoCommitChanges),
            LogAction::CompareTwoCommitChanges,
            "Comp&are change sets",
        );
        m.sep();
        // Upstream quirk: no separator follows this entry.
        m.item_if(
            has(LogAction::RevertRev) && inp.has_wc && !inp.merge_active,
            LogAction::RevertRev,
            "&Revert changes by these commits",
        );
        if inp.bisect_active && has(LogAction::BisectSkip) && !e.has_bisect_ref {
            m.item(LogAction::BisectSkip, "Bisect skip");
            m.sep();
        }
    }

    // F. Not the working tree.
    if !wc {
        m.item_if(
            n >= 2
                && inp.contiguous
                && has(LogAction::CombineCommit)
                && inp.has_wc
                && !inp.merge_active
                && inp.combine_ok,
            LogAction::CombineCommit,
            "Com&bine to one commit",
        );
        m.item_if(
            has(LogAction::CherryPick) && !e.is_head && inp.has_wc && !inp.merge_active,
            LogAction::CherryPick,
            if n >= 2 {
                "Cherr&y Pick selected commits..."
            } else {
                "Cherr&y Pick this commit..."
            },
        );
        m.item_if(
            !e.is_stash && (n <= 2 || inp.contiguous) && has(LogAction::CreatePatch),
            LogAction::CreatePatch,
            "&Format Patch...",
        );
        m.sep();
        if inp.has_wc
            && !inp.merge_active
            && !e.is_stash
            && has(LogAction::BisectStart)
            && n == 2
            && !inp.bisect_active
        {
            m.item(LogAction::BisectStart, "Bisect start");
            m.sep();
        }
    }

    // H. Single selection, continued.
    if n == 1 {
        let refs = !e.refs.is_empty();
        let pushable = e.refs.iter().any(|r| {
            r.full_name.starts_with("refs/heads/") || r.full_name.starts_with("refs/tags/")
        });
        if has(LogAction::Push) && ((refs && !e.is_stash) || inp.shift) && (pushable || inp.shift) {
            let label = e
                .refs
                .iter()
                .find(|r| {
                    r.full_name.starts_with("refs/heads/") || r.full_name.starts_with("refs/tags/")
                })
                .map(|r| format!("Pus&h \"{}\"...", r.short()))
                .unwrap_or_else(|| "Pus&h...".to_owned());
            m.item(LogAction::Push, label);
            m.item_if(
                has(LogAction::SvnDCommit) && inp.git_svn,
                LogAction::SvnDCommit,
                "SVN DCommit...",
            );
        }
        m.item_if(
            has(LogAction::PullHead) && e.is_head && !inp.merge_active && inp.has_wc,
            LogAction::PullHead,
            "&Pull...",
        );
        if has(LogAction::Delete) {
            let current_ref = inp
                .current_branch
                .as_ref()
                .map(|b| format!("refs/heads/{b}"));
            let deletable: Vec<(usize, &MenuRef)> = e
                .refs
                .iter()
                .enumerate()
                .filter(|(_, r)| Some(&r.full_name) != current_ref.as_ref())
                .collect();
            match deletable.as_slice() {
                [] => {}
                [(k, r)] => {
                    m.raw(
                        sub_id(LogAction::Delete, *k as u32),
                        format!("&Delete {}", r.full_name),
                        true,
                    );
                }
                many => {
                    let mut children: Vec<Leaf> = many
                        .iter()
                        .map(|(k, r)| Leaf {
                            id: sub_id(LogAction::Delete, *k as u32),
                            title: r.full_name.clone(),
                            enabled: true,
                        })
                        .collect();
                    children.push(Leaf {
                        id: sub_id(LogAction::Delete, DELETE_ALL),
                        title: "All".into(),
                        enabled: true,
                    });
                    m.sub("&Delete branch/tag", children);
                }
            }
        }
        m.sep();
        if has(LogAction::ToggleRollup) && inp.rollup_possible && !wc {
            m.item(
                LogAction::ToggleRollup,
                if e.rolled_up {
                    "Expand collapsed revisions"
                } else {
                    "Collapse revisions"
                },
            );
            m.sep();
        }
    }

    // I. Always.
    if has(LogAction::CopyClipboard) {
        let children = if inp.reflog {
            vec![
                leaf(LogAction::CopyFull, "Full data", true),
                leaf(LogAction::CopySha, "SHA-1", true),
                leaf(LogAction::CopyMessages, "Messages", true),
            ]
        } else {
            let mut c = vec![
                leaf(LogAction::CopyFull, "Full data", true),
                leaf(
                    LogAction::CopyNoPaths,
                    "Full commit data without changed paths",
                    true,
                ),
                leaf(LogAction::CopySha, "SHA-1", true),
                leaf(LogAction::CopyAuthorsFull, "Authors (full)", true),
                leaf(LogAction::CopyAuthorNames, "Author names", true),
                leaf(LogAction::CopyAuthorEmails, "Author email addresses", true),
                leaf(LogAction::CopySubjects, "Subjects", true),
                leaf(LogAction::CopyMessages, "Messages", true),
            ];
            if n == 1 && !e.refs.is_empty() {
                c.push(leaf(LogAction::CopyRefNames, "Tag/branch names", true));
            }
            c
        };
        m.sub("Copy to clipboard", children);
    }
    m.item_if(
        has(LogAction::FindEntry),
        LogAction::FindEntry,
        "Search &log messages...",
    );
    m.item_if(
        n == 1 && has(LogAction::ShowBranches) && !wc,
        LogAction::ShowBranches,
        "Show branches this commit is &on",
    );
    m.finish()
}

fn short(id: &str) -> &str {
    &id[..id.len().min(8)]
}

/// `Parent %d: "<subject ≤20 chars…>" (<shorthash>)` (LLB:1706-1724).
pub fn parent_label(k: usize, subject: &str, id: &str) -> String {
    let s: String = if subject.chars().count() > 20 {
        format!("{}…", subject.chars().take(20).collect::<String>())
    } else {
        subject.to_owned()
    };
    format!("Parent {k}: \"{s}\" ({})", short(id))
}

// ---------------------------------------------------------------- clipboard

/// One commit as the clipboard formats need it.
#[derive(Debug, Clone, Default)]
pub struct CopyCommit {
    pub id: String,
    pub author_name: String,
    pub author_email: String,
    /// Date as shown in the list.
    pub date: String,
    pub subject: String,
    /// Message body without the subject line.
    pub body: String,
    pub notes: String,
    pub tag_info: String,
    /// `<ActionName>: <path>[ (from <old>)]` lines.
    pub paths: Vec<String>,
    /// Full ref names (for "Tag/branch names").
    pub refs: Vec<String>,
    /// Reflog action, e.g. `commit (amend)`.
    pub reflog_action: String,
}

fn crlf(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\n', "\r\n")
}

/// The text for a "Copy to clipboard" entry (spec §2.4, §4.1), CRLF line ends.
pub fn copy_text(action: LogAction, commits: &[CopyCommit], reflog: bool) -> String {
    let mut out = String::new();
    match action {
        LogAction::CopyFull | LogAction::CopyNoPaths if reflog => {
            for c in commits {
                let msg = join_subject_body(c);
                let msg = if c.reflog_action.is_empty() {
                    msg
                } else {
                    format!("{}: {msg}", c.reflog_action)
                };
                out.push_str(&format!(
                    "Revision: {}\r\nDate: {}\r\nMessage: {}\r\n",
                    c.id,
                    c.date,
                    crlf(&msg)
                ));
            }
        }
        LogAction::CopyFull | LogAction::CopyNoPaths => {
            for c in commits {
                out.push_str(&format!(
                    "Revision: {}\r\nAuthor: {} <{}>\r\nDate: {}\r\nMessage:\r\n{}\r\n{}\r\n",
                    c.id,
                    c.author_name,
                    c.author_email,
                    c.date,
                    c.subject,
                    crlf(c.body.trim_end())
                ));
                if !c.notes.is_empty() {
                    out.push_str(&format!("----\r\nNotes:\r\n{}", crlf(&c.notes)));
                }
                if !c.tag_info.is_empty() {
                    out.push_str(&format!("----\r\nTag Info:\r\n{}", crlf(&c.tag_info)));
                }
                if action == LogAction::CopyFull {
                    out.push_str("----\r\n");
                    out.push_str(c.paths.join("\r\n").trim());
                    out.push_str("\r\n");
                }
                out.push_str("\r\n");
            }
        }
        LogAction::CopySha => {
            out = commits
                .iter()
                .map(|c| c.id.clone())
                .collect::<Vec<_>>()
                .join("\r\n");
        }
        LogAction::CopyAuthorsFull => {
            out = commits
                .iter()
                .map(|c| format!("{} <{}>", c.author_name, c.author_email))
                .collect::<Vec<_>>()
                .join("\r\n");
        }
        LogAction::CopyAuthorNames => {
            out = commits
                .iter()
                .map(|c| c.author_name.clone())
                .collect::<Vec<_>>()
                .join("\r\n");
        }
        LogAction::CopyAuthorEmails => {
            out = commits
                .iter()
                .map(|c| c.author_email.clone())
                .collect::<Vec<_>>()
                .join("\r\n");
        }
        LogAction::CopySubjects => {
            for c in commits {
                out.push_str(&format!("* {}\r\n\r\n", c.subject.trim()));
            }
        }
        LogAction::CopyMessages => {
            for c in commits {
                let msg = join_subject_body(c);
                if reflog && !c.reflog_action.is_empty() {
                    out.push_str(&format!("* {}: {}\r\n\r\n", c.reflog_action, crlf(&msg)));
                } else {
                    out.push_str(&format!("* {}\r\n\r\n", crlf(&msg)));
                }
            }
        }
        LogAction::CopyRefNames => {
            for c in commits {
                for r in &c.refs {
                    out.push_str(r.trim_end_matches("^{}"));
                    out.push_str("\r\n");
                }
            }
        }
        _ => {}
    }
    out
}

fn join_subject_body(c: &CopyCommit) -> String {
    let body = c.body.trim_end();
    if body.is_empty() {
        c.subject.clone()
    } else {
        format!("{}\n{body}", c.subject)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(entries: &[Entry]) -> Vec<String> {
        entries
            .iter()
            .map(|e| match e {
                Entry::Item(l) => l.title.replace('&', ""),
                Entry::Separator => "---".into(),
                Entry::Sub {
                    title, children, ..
                } => format!(
                    "[{}: {}]",
                    title.replace('&', ""),
                    children
                        .iter()
                        .map(|c| c.title.clone())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            })
            .collect()
    }

    fn commit(id: &str, parents: usize) -> MenuCommit {
        MenuCommit {
            id: id.into(),
            parent_labels: (1..=parents)
                .map(|k| parent_label(k, "parent subject", &format!("p{k}aaaaaaaa")))
                .collect(),
            ..Default::default()
        }
    }

    fn input(selected: Vec<MenuCommit>) -> LogMenuInput {
        LogMenuInput {
            selected,
            current_branch: Some("main".into()),
            ..Default::default()
        }
    }

    #[test]
    fn single_ordinary_commit() {
        let l = labels(&build(&input(vec![commit("abcdef0123", 1)])));
        assert_eq!(
            l,
            [
                "Compare with working tree",
                "Show changes as unified diff",
                "Compare with previous revision",
                "---",
                "Browse repository",
                "Merge to \"main\"...",
                "Reset \"main\" to this...",
                "Switch/Checkout to this...",
                "Create Branch at this version...",
                "Create Tag at this version...",
                "Rebase \"main\" onto this...(G)",
                "Export this version...",
                "Revert change by this commit",
                "Edit Notes",
                "---",
                "Cherry Pick this commit...",
                "Format Patch...",
                "---",
                "[Copy to clipboard: Full data, Full commit data without changed paths, SHA-1, Authors (full), Author names, Author email addresses, Subjects, Messages]",
                "Search log messages...",
                "Show branches this commit is on",
            ]
        );
    }

    #[test]
    fn head_with_refs_and_merge_parents() {
        let mut e = commit("abcdef0123", 2);
        e.is_head = true;
        e.refs = vec![
            MenuRef::new("refs/heads/main"),
            MenuRef::new("refs/heads/topic"),
            MenuRef::new("refs/tags/v1"),
        ];
        let entries = build(&input(vec![e]));
        let l = labels(&entries);
        assert!(l.iter().any(|x| x.starts_with("[Unified diff with: All Parents, Only Merged Files, Show extra changes after merge, Parent 1:")));
        assert!(
            l.iter()
                .any(|x| x.starts_with("[Compare with previous revision: Parent 1"))
        );
        assert!(
            l.iter()
                .any(|x| x.starts_with("[Revert change by this commit: Parent 1"))
        );
        assert!(
            l.contains(&"Show log...".to_owned()),
            "refs make Show log visible"
        );
        assert!(l.contains(&"Switch/Checkout to \"topic\"".to_owned()));
        assert!(
            !l.iter().any(|x| x.starts_with("Merge to")),
            "HEAD can't merge itself"
        );
        assert!(
            !l.iter().any(|x| x.starts_with("Cherry Pick")),
            "HEAD can't be picked"
        );
        assert!(l.contains(&"Push \"main\"...".to_owned()));
        assert!(l.contains(&"Pull...".to_owned()));
        assert!(l.contains(&"[Delete branch/tag: refs/heads/topic, refs/tags/v1, All]".to_owned()));
        assert!(l.last().unwrap().contains("on"));
        let copy = l.iter().find(|x| x.starts_with("[Copy")).unwrap();
        assert!(copy.ends_with("Tag/branch names]"));
        // Sub ids round-trip.
        let id = entries
            .iter()
            .find_map(|e| match e {
                Entry::Sub {
                    title, children, ..
                } if title.starts_with("&Unified") => Some(children[0].id),
                _ => None,
            })
            .unwrap();
        assert_eq!(decode(id), Some((LogAction::GnuDiff1, ALL_PARENTS)));
    }

    #[test]
    fn working_tree_row() {
        let wc = MenuCommit {
            parent_labels: vec![parent_label(1, "head", "0123456789")],
            ..Default::default()
        };
        let mut inp = input(vec![wc]);
        inp.merge_active = true;
        inp.stash_exists = true;
        inp.has_submodules = true;
        let l = labels(&build(&inp));
        assert_eq!(
            l,
            [
                "Commit...",
                "Abort Merge",
                "Show changes as unified diff",
                "Compare with previous revision",
                "---",
                "Stash Pop",
                "Stash List",
                "---",
                "Fetch...",
                "Submodule Update",
                "---",
                "Clean up...",
                "---",
                "[Copy to clipboard: Full data, Full commit data without changed paths, SHA-1, Authors (full), Author names, Author email addresses, Subjects, Messages]",
                "Search log messages...",
            ]
        );
    }

    #[test]
    fn two_selected_and_combine() {
        let mut inp = input(vec![commit("aaaaaaaa11", 1), commit("bbbbbbbb22", 1)]);
        inp.combine_ok = true;
        inp.bisect_active = false;
        let l = labels(&build(&inp));
        assert_eq!(
            l,
            [
                "Compare revisions",
                "Show changes as unified diff",
                "Show log of bbbbbbbb..aaaaaaaa",
                "Show log of aaaaaaaa..bbbbbbbb",
                "Show log of bbbbbbbb...aaaaaaaa",
                "Compare change sets",
                "---",
                "Revert changes by these commits",
                "Combine to one commit",
                "Cherry Pick selected commits...",
                "Format Patch...",
                "---",
                "Bisect start",
                "---",
                "[Copy to clipboard: Full data, Full commit data without changed paths, SHA-1, Authors (full), Author names, Author email addresses, Subjects, Messages]",
                "Search log messages...",
            ]
        );
        // Three non-contiguous: no compare, no combine, no format patch.
        let mut inp = input(vec![commit("a1", 1), commit("b2", 1), commit("c3", 1)]);
        inp.contiguous = false;
        inp.combine_ok = true;
        let l = labels(&build(&inp));
        assert!(!l.contains(&"Compare revisions".to_owned()));
        assert!(!l.contains(&"Combine to one commit".to_owned()));
        assert!(!l.contains(&"Format Patch...".to_owned()));
        assert!(l.contains(&"Cherry Pick selected commits...".to_owned()));
    }

    #[test]
    fn masks_and_states() {
        let mut inp = input(vec![commit("abc12345", 1)]);
        inp.bisect_active = true;
        inp.mask = LogMask::reflog();
        let mut e = commit("abc12345", 1);
        e.reflog_ref = Some("refs/stash@{0}".into());
        e.is_stash = true;
        inp.selected = vec![e];
        inp.reflog = true;
        inp.stash_exists = true;
        let l = labels(&build(&inp));
        assert!(l.contains(&"Show log...".to_owned()));
        assert!(l.contains(&"Delete Ref...".to_owned()));
        assert!(l.contains(&"Stash Apply".to_owned()));
        assert!(l.contains(&"Stash Pop".to_owned()));
        assert!(l.contains(&"Bisect good".to_owned()));
        assert!(
            !l.iter().any(|x| x.starts_with("Reset")),
            "stash rows can't reset"
        );
        assert!(l.contains(&"[Copy to clipboard: Full data, SHA-1, Messages]".to_owned()));

        // Rebase list, editable: pick/squash/edit/skip; no cherry-pick/reset.
        let mut inp = input(vec![commit("abc12345", 1)]);
        inp.mask = LogMask::rebase(true);
        let l = labels(&build(&inp));
        assert_eq!(
            &l[..5],
            ["Pick", "Squash (with commit below)", "Edit", "Skip", "---"]
        );
        assert!(!l.iter().any(|x| x.starts_with("Cherry")));
        assert!(!l.iter().any(|x| x.starts_with("Reset")));
        inp.bottom_selected = true;
        assert!(!labels(&build(&inp)).iter().any(|x| x.starts_with("Squash")));

        // Bare repository: no working-tree actions.
        let mut inp = input(vec![commit("abc12345", 1)]);
        inp.has_wc = false;
        let l = labels(&build(&inp));
        assert!(!l.iter().any(|x| x.starts_with("Compare with working")));
        assert!(!l.iter().any(|x| x.starts_with("Reset")));
        assert!(!l.iter().any(|x| x.starts_with("Revert")));
        assert!(l.contains(&"Create Branch at this version...".to_owned()));
        assert!(LogMask::sync().has(LogAction::CompareTwo));
        assert!(!LogMask::sync().has(LogAction::Log));
    }

    #[test]
    fn clipboard_formats() {
        let c = CopyCommit {
            id: "abc".into(),
            author_name: "Ann".into(),
            author_email: "a@x".into(),
            date: "2026-01-01 10:00".into(),
            subject: "Fix".into(),
            body: "Line one\nLine two\n".into(),
            paths: vec!["Modified: src/a.rs".into(), "Added: b (from c)".into()],
            refs: vec!["refs/tags/v1^{}".into()],
            ..Default::default()
        };
        assert_eq!(
            copy_text(LogAction::CopyFull, std::slice::from_ref(&c), false),
            "Revision: abc\r\nAuthor: Ann <a@x>\r\nDate: 2026-01-01 10:00\r\nMessage:\r\nFix\r\nLine one\r\nLine two\r\n----\r\nModified: src/a.rs\r\nAdded: b (from c)\r\n\r\n"
        );
        assert!(
            !copy_text(LogAction::CopyNoPaths, std::slice::from_ref(&c), false)
                .contains("Modified")
        );
        let two = [
            c.clone(),
            CopyCommit {
                id: "def".into(),
                ..c.clone()
            },
        ];
        assert_eq!(copy_text(LogAction::CopySha, &two, false), "abc\r\ndef");
        assert_eq!(
            copy_text(LogAction::CopyAuthorsFull, &two[..1], false),
            "Ann <a@x>"
        );
        assert_eq!(
            copy_text(LogAction::CopySubjects, &two[..1], false),
            "* Fix\r\n\r\n"
        );
        assert_eq!(
            copy_text(LogAction::CopyMessages, &two[..1], false),
            "* Fix\r\nLine one\r\nLine two\r\n\r\n"
        );
        assert_eq!(
            copy_text(LogAction::CopyRefNames, &two[..1], false),
            "refs/tags/v1\r\n"
        );
        let r = CopyCommit {
            reflog_action: "commit (amend)".into(),
            body: String::new(),
            ..c
        };
        assert_eq!(
            copy_text(LogAction::CopyFull, std::slice::from_ref(&r), true),
            "Revision: abc\r\nDate: 2026-01-01 10:00\r\nMessage: commit (amend): Fix\r\n"
        );
        assert_eq!(
            parent_label(2, "A very long subject line here", "0123456789"),
            "Parent 2: \"A very long subject …\" (01234567)"
        );
    }
}
