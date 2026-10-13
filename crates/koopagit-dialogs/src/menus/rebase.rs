//! Rebase dialog commit list menu (spec-menus.md §6): the rebase actions,
//! then the log-list entries the rebase mask keeps.

use super::{Entry, Menu, leaf};

menu_actions! {
    pub enum RebaseMenuAction {
        Pick = 1,
        Squash = 2,
        Edit = 3,
        Skip = 4,
        ShowLog = 5,
        UnifiedDiff = 6,
        CompareWithPrevious = 7,
        CreateBranch = 8,
        CreateTag = 9,
        Export = 10,
        CopyFull = 11,
        CopySha = 12,
        CopySubjects = 13,
        CopyMessages = 14,
    }
}

#[derive(Debug, Clone, Default)]
pub struct RebaseMenuInput {
    /// Selected display rows (newest first; the bottom row is the oldest commit).
    pub selected: Vec<usize>,
    pub row_count: usize,
    /// The choose-commits stage: actions can still be changed.
    pub editable: bool,
    /// The focused row is CURRENT or DONE.
    pub focused_started: bool,
    /// Parent count of the focused commit.
    pub focused_parents: usize,
    pub cherry_pick: bool,
}

pub fn build(i: &RebaseMenuInput) -> Vec<Entry> {
    use RebaseMenuAction as A;
    let mut m = Menu::new();
    let n = i.selected.len();
    if i.editable && !i.focused_started && n > 0 {
        let bottom = i.row_count.saturating_sub(1);
        let first = i.selected.iter().min().copied().unwrap_or(0);
        let last = i.selected.iter().max().copied().unwrap_or(0);
        m.item(A::Pick, "Pick");
        m.item_if(
            first != bottom && last != bottom && (i.cherry_pick || i.focused_parents == 1),
            A::Squash,
            "Squash (with commit below)",
        );
        m.item(A::Edit, "Edit");
        m.item(A::Skip, "Skip");
        m.sep();
    }
    if n == 1 {
        m.item(A::UnifiedDiff, "Show changes as &unified diff");
        m.item(A::CompareWithPrevious, "&Compare with previous revision");
        m.sep();
        m.item(A::ShowLog, "Show log...");
        m.item(A::CreateBranch, "Create Br&anch at this version...");
        m.item(A::CreateTag, "Create &Tag at this version...");
        m.item(A::Export, "E&xport this version...");
        m.sep();
    }
    if n > 0 {
        m.sub(
            "Copy to clipboard",
            vec![
                leaf(A::CopyFull, "Full data", true),
                leaf(A::CopySha, "SHA-1", true),
                leaf(A::CopySubjects, "Subjects", true),
                leaf(A::CopyMessages, "Messages", true),
            ],
        );
    }
    m.finish()
}

/// Space cycles Pick → Skip → Edit → Squash → Pick; on the bottom row Edit
/// goes straight back to Pick (`RebaseDlg::PreTranslateMessage`).
/// Actions: 0 pick, 1 squash, 2 edit, 3 skip.
pub fn cycle_action(action: i32, is_bottom: bool) -> i32 {
    match action {
        0 => 3,
        3 => 2,
        2 if is_bottom => 0,
        2 => 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(e: &[Entry]) -> Vec<String> {
        e.iter()
            .map(|e| match e {
                Entry::Item(l) => l.title.replace('&', ""),
                Entry::Separator => "---".into(),
                Entry::Sub { title, .. } => format!("[{title}]"),
            })
            .collect()
    }

    #[test]
    fn squash_needs_a_commit_below() {
        let base = RebaseMenuInput {
            selected: vec![0],
            row_count: 3,
            editable: true,
            focused_parents: 1,
            ..Default::default()
        };
        let t = titles(&build(&base));
        assert_eq!(
            t[..5],
            ["Pick", "Squash (with commit below)", "Edit", "Skip", "---"]
        );
        let bottom = titles(&build(&RebaseMenuInput {
            selected: vec![2],
            ..base.clone()
        }));
        assert!(!bottom.contains(&"Squash (with commit below)".to_owned()));
        let merge = titles(&build(&RebaseMenuInput {
            focused_parents: 2,
            ..base.clone()
        }));
        assert!(!merge.contains(&"Squash (with commit below)".to_owned()));
        let started = titles(&build(&RebaseMenuInput {
            focused_started: true,
            ..base.clone()
        }));
        assert_eq!(started[0], "Show changes as unified diff");
        let multi = titles(&build(&RebaseMenuInput {
            selected: vec![0, 1],
            ..base
        }));
        assert_eq!(multi.last().unwrap(), "[Copy to clipboard]");
        assert!(!multi.contains(&"Show log...".to_owned()));
    }

    #[test]
    fn space_cycles_actions() {
        assert_eq!(cycle_action(0, false), 3);
        assert_eq!(cycle_action(3, false), 2);
        assert_eq!(cycle_action(2, false), 1);
        assert_eq!(cycle_action(1, false), 0);
        assert_eq!(cycle_action(2, true), 0);
    }
}
