//! RefLog / Stash List row menu (`CRefLogList`: the log-list menu with the
//! reflog mask; spec-menus.md §4.1 and §2.2).

use super::{Entry, Leaf, Menu, leaf};

menu_actions! {
    pub enum ReflogAction {
        CompareWithWorkingTree = 1,
        UnifiedDiff = 2,
        CompareWithPrevious = 3,
        ShowLog = 4,
        BrowseRepository = 5,
        Merge = 6,
        Reset = 7,
        SwitchToRev = 8,
        CreateBranch = 9,
        CreateTag = 10,
        RebaseOnto = 11,
        Export = 12,
        Revert = 13,
        EditNotes = 14,
        DeleteRef = 15,
        StashApply = 16,
        CompareRevisions = 17,
        UnifiedDiff2 = 18,
        LogRange = 19,
        LogRangeReverse = 20,
        LogRangeReachableFromOnlyOne = 21,
        CherryPick = 22,
        FormatPatch = 23,
        CopyFull = 24,
        CopySha = 25,
        CopyMessages = 26,
        ShowBranches = 27,
    }
}

/// What the menu depends on. "First" is the top-most (newest) selected row,
/// "last" the bottom-most.
#[derive(Debug, Clone, Default)]
pub struct ReflogMenuInput {
    pub count: usize,
    /// The list shows `refs/stash`.
    pub is_stash: bool,
    /// The first selected entry is the commit HEAD points to.
    pub is_head: bool,
    pub merge_active: bool,
    pub has_wc: bool,
    pub current_branch: String,
    /// Selected rows form one contiguous run.
    pub contiguous: bool,
    /// Short hashes of the first and last selected entries (for the range labels).
    pub first_short: String,
    pub last_short: String,
}

pub fn build(i: &ReflogMenuInput) -> Vec<Entry> {
    use ReflogAction as A;
    let mut m = Menu::new();
    let one = i.count == 1;
    let stash = i.is_stash;
    if one {
        m.item_if(
            i.has_wc,
            A::CompareWithWorkingTree,
            "Compare with &working tree",
        );
        m.item_if(i.has_wc, A::UnifiedDiff, "Show changes as &unified diff");
        m.item(A::CompareWithPrevious, "&Compare with previous revision");
        m.sep();
        // The reflog makes "Show log..." the default (first) item.
        m.item(A::ShowLog, "Show log...");
        m.item(A::BrowseRepository, "&Browse repository");
        m.item_if(
            !i.is_head && i.has_wc && !i.merge_active && !stash,
            A::Merge,
            format!("&Merge to \"{}\"...", i.current_branch),
        );
        m.item_if(
            i.has_wc && !stash,
            A::Reset,
            format!("R&eset \"{}\" to this...", i.current_branch),
        );
        m.item_if(
            !i.is_head && i.has_wc && !stash,
            A::SwitchToRev,
            "Sw&itch/Checkout to this...",
        );
        m.item_if(!stash, A::CreateBranch, "Create Br&anch at this version...");
        m.item_if(!stash, A::CreateTag, "Create &Tag at this version...");
        m.item_if(
            !i.is_head && i.has_wc && !i.merge_active && !stash,
            A::RebaseOnto,
            format!("Rebase \"{}\" onto this...(&G)", i.current_branch),
        );
        m.item(A::Export, "E&xport this version...");
        m.item_if(
            i.has_wc && !i.merge_active && !stash,
            A::Revert,
            "&Revert change by this commit",
        );
        m.item_if(!stash, A::EditNotes, "Edit &Notes");
        m.sep();
    }
    // Reflog rows (no mask check).
    m.item(A::DeleteRef, "Delete Ref...");
    m.item_if(one && stash, A::StashApply, "Stash Apply");
    if i.count <= 2 {
        m.sep();
    }
    if i.count >= 2 {
        m.item_if(
            i.count == 2 || i.contiguous,
            A::CompareRevisions,
            "&Compare revisions",
        );
        m.item_if(
            i.count == 2 && i.has_wc,
            A::UnifiedDiff2,
            "Show changes as &unified diff",
        );
        if i.count == 2 {
            let (f, l) = (&i.first_short, &i.last_short);
            m.item(A::LogRange, format!("Show log of {l}..{f}"));
            m.item(A::LogRangeReverse, format!("Show log of {f}..{l}"));
            m.item(
                A::LogRangeReachableFromOnlyOne,
                format!("Show log of {l}...{f}"),
            );
        }
        m.sep();
        m.item_if(
            i.has_wc && !i.merge_active,
            A::Revert,
            "&Revert changes by these commits",
        );
    }
    m.item_if(
        !(one && i.is_head) && i.has_wc && !i.merge_active,
        A::CherryPick,
        if one {
            "Cherr&y Pick this commit..."
        } else {
            "Cherr&y Pick selected commits..."
        },
    );
    m.item_if(
        !stash && (i.count <= 2 || i.contiguous),
        A::FormatPatch,
        "&Format Patch...",
    );
    m.sep();
    let copy: Vec<Leaf> = vec![
        leaf(A::CopyFull, "Full data", true),
        leaf(A::CopySha, "SHA-1", true),
        leaf(A::CopyMessages, "Messages", true),
    ];
    m.sub("Copy to clipboard", copy);
    m.item_if(one, A::ShowBranches, "Show branches this commit is &on");
    m.finish()
}

/// One reflog row for the clipboard formats (`refloglist.cpp:146-184`).
pub struct ClipRow<'a> {
    pub hash: &'a str,
    pub date: &'a str,
    pub action: &'a str,
    pub message: &'a str,
}

fn with_action(action: &str, message: &str) -> String {
    if action.is_empty() {
        message.to_owned()
    } else {
        format!("{action}: {message}")
    }
}

/// "Full data": `Revision: ..\r\nDate: ..\r\nMessage: <action>: <message>\r\n`.
pub fn copy_full(rows: &[ClipRow<'_>]) -> String {
    rows.iter()
        .map(|r| {
            format!(
                "Revision: {}\r\nDate: {}\r\nMessage: {}\r\n",
                r.hash,
                r.date,
                with_action(r.action, r.message)
            )
        })
        .collect()
}

/// "SHA-1": full hashes joined with CRLF.
pub fn copy_sha(rows: &[ClipRow<'_>]) -> String {
    rows.iter().map(|r| r.hash).collect::<Vec<_>>().join("\r\n")
}

/// "Messages": `* <action>: <message>\r\n\r\n` per row.
pub fn copy_messages(rows: &[ClipRow<'_>]) -> String {
    rows.iter()
        .map(|r| format!("* {}\r\n\r\n", with_action(r.action, r.message)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menus::ActionId;

    fn titles(e: &[Entry]) -> Vec<String> {
        e.iter()
            .map(|e| match e {
                Entry::Item(l) => l.title.replace('&', ""),
                Entry::Separator => "---".into(),
                Entry::Sub { title, .. } => format!("[{title}]"),
            })
            .collect()
    }

    fn input() -> ReflogMenuInput {
        ReflogMenuInput {
            count: 1,
            has_wc: true,
            current_branch: "main".into(),
            contiguous: true,
            first_short: "aaaa".into(),
            last_short: "bbbb".into(),
            ..Default::default()
        }
    }

    #[test]
    fn single_head_row() {
        let t = titles(&build(&ReflogMenuInput {
            is_head: true,
            ..input()
        }));
        assert_eq!(
            t,
            [
                "Compare with working tree",
                "Show changes as unified diff",
                "Compare with previous revision",
                "---",
                "Show log...",
                "Browse repository",
                "Reset \"main\" to this...",
                "Create Branch at this version...",
                "Create Tag at this version...",
                "Export this version...",
                "Revert change by this commit",
                "Edit Notes",
                "---",
                "Delete Ref...",
                "---",
                "Format Patch...",
                "---",
                "[Copy to clipboard]",
                "Show branches this commit is on",
            ]
        );
    }

    #[test]
    fn stash_and_multi_selection() {
        let t = titles(&build(&ReflogMenuInput {
            is_stash: true,
            ..input()
        }));
        assert!(t.contains(&"Stash Apply".to_owned()));
        assert!(
            !t.iter()
                .any(|x| x.starts_with("Reset") || x.starts_with("Merge"))
        );
        let t = titles(&build(&ReflogMenuInput {
            count: 2,
            ..input()
        }));
        assert!(t.contains(&"Show log of bbbb..aaaa".to_owned()));
        assert!(t.contains(&"Show log of bbbb...aaaa".to_owned()));
        assert!(t.contains(&"Cherry Pick selected commits...".to_owned()));
        assert!(!t.contains(&"Stash Apply".to_owned()));
        let three = titles(&build(&ReflogMenuInput {
            count: 3,
            contiguous: false,
            ..input()
        }));
        assert!(!three.contains(&"Compare revisions".to_owned()));
        assert!(!three.contains(&"Format Patch...".to_owned()));
        assert_eq!(
            ReflogAction::from_id(ReflogAction::DeleteRef.id()),
            Some(ReflogAction::DeleteRef)
        );
    }

    #[test]
    fn clipboard_formats() {
        let rows = [
            ClipRow {
                hash: "h1",
                date: "d1",
                action: "commit",
                message: "m1",
            },
            ClipRow {
                hash: "h2",
                date: "d2",
                action: "",
                message: "m2",
            },
        ];
        assert_eq!(
            copy_full(&rows),
            "Revision: h1\r\nDate: d1\r\nMessage: commit: m1\r\nRevision: h2\r\nDate: d2\r\nMessage: m2\r\n"
        );
        assert_eq!(copy_sha(&rows), "h1\r\nh2");
        assert_eq!(copy_messages(&rows), "* commit: m1\r\n\r\n* m2\r\n\r\n");
    }
}
