//! File Diff ("Changed Files") list menu (`CFileDiffDlg::OnContextMenu`,
//! `FileDiffDlg.cpp:545-760`; spec-menus §1.7).

use super::{Entry, Menu};

menu_actions! {
    pub enum FileDiffAction {
        Compare = 1,
        GnuDiff = 2,
        Revert1 = 3,
        Revert2 = 4,
        Log = 5,
        Blame = 6,
        Export = 7,
        LogSubmodule = 8,
        SaveAs = 9,
        CopyPaths = 10,
        CopyAll = 11,
    }
}

/// What the menu needs to know. `rev1`/`rev2` are empty for the working tree.
#[derive(Debug, Clone, Default)]
pub struct FileDiffMenuInput {
    pub selected: usize,
    pub bare: bool,
    pub rev1: String,
    pub rev2: String,
    /// The first selected row is a directory (submodule).
    pub first_is_dir: bool,
    /// The first selected row is deleted.
    pub first_deleted: bool,
}

/// Title of a revision in "Revert to revision ..." (upstream: the commit title).
pub fn rev_label(rev: &str) -> &str {
    rev
}

/// Builds the menu; empty when nothing is selected (upstream shows no menu).
pub fn build(i: &FileDiffMenuInput) -> Vec<Entry> {
    use FileDiffAction as A;
    if i.selected == 0 {
        return Vec::new();
    }
    let mut m = Menu::new();
    m.item(A::Compare, "&Compare revisions")
        .item(A::GnuDiff, "Show changes as &unified diff")
        .sep();
    if !i.bare {
        m.item_if(
            !i.rev1.is_empty(),
            A::Revert1,
            format!("Revert to revision {}", rev_label(&i.rev1)),
        )
        .item_if(
            !i.rev2.is_empty() && (i.rev1.is_empty() || !i.first_deleted),
            A::Revert2,
            format!("Revert to revision {}", rev_label(&i.rev2)),
        )
        .sep();
    }
    m.item(A::Log, "Show log");
    let blame = !i.first_is_dir && !i.bare;
    if blame {
        m.item(A::Blame, "&Blame revisions").sep();
    }
    m.item_if(!i.first_is_dir, A::Export, "&Export selection to...")
        .item_if(i.first_is_dir, A::LogSubmodule, "Show log of submodule")
        .sep()
        .item(A::SaveAs, "&Save list of selected files to...")
        .item(A::CopyPaths, "Copy paths to clipboard")
        .item(A::CopyAll, "Copy all information to clipboard");
    m.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menus::Entry;

    fn titles(e: &[Entry]) -> Vec<String> {
        e.iter()
            .map(|e| match e {
                Entry::Item(l) => l.title.clone(),
                Entry::Separator => "---".into(),
                Entry::Sub { title, .. } => format!("[{title}]"),
            })
            .collect()
    }

    fn input() -> FileDiffMenuInput {
        FileDiffMenuInput {
            selected: 1,
            rev1: "HEAD~1".into(),
            rev2: "HEAD".into(),
            ..Default::default()
        }
    }

    #[test]
    fn file_between_two_commits() {
        assert_eq!(
            titles(&build(&input())),
            [
                "&Compare revisions",
                "Show changes as &unified diff",
                "---",
                "Revert to revision HEAD~1",
                "Revert to revision HEAD",
                "---",
                "Show log",
                "&Blame revisions",
                "---",
                "&Export selection to...",
                "---",
                "&Save list of selected files to...",
                "Copy paths to clipboard",
                "Copy all information to clipboard",
            ]
        );
    }

    #[test]
    fn conditions() {
        assert!(build(&FileDiffMenuInput::default()).is_empty());
        // Working tree as rev2: no "revert to rev2"; deleted file with rev1 set
        // hides rev2 revert too.
        let wt = FileDiffMenuInput {
            rev2: String::new(),
            ..input()
        };
        assert!(!titles(&build(&wt)).iter().any(|t| t.ends_with("revision ")));
        let deleted = FileDiffMenuInput {
            first_deleted: true,
            ..input()
        };
        assert!(!titles(&build(&deleted)).contains(&"Revert to revision HEAD".to_owned()));
        let dir = FileDiffMenuInput {
            first_is_dir: true,
            ..input()
        };
        let t = titles(&build(&dir));
        assert!(t.contains(&"Show log of submodule".to_owned()));
        assert!(!t.contains(&"&Blame revisions".to_owned()));
        assert!(!t.contains(&"&Export selection to...".to_owned()));
        let bare = FileDiffMenuInput {
            bare: true,
            ..input()
        };
        let t = titles(&build(&bare));
        assert!(
            !t.iter()
                .any(|t| t.starts_with("Revert") || t.contains("Blame"))
        );
        assert_eq!(t[2], "---");
        assert_eq!(t[3], "Show log");
    }
}
