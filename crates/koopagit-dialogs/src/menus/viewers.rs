//! TortoiseGitMerge view menus (spec-menus.md §8) and the TortoiseGitBlame
//! line menu (§9.1).

use super::{Entry, Leaf, Menu};

menu_actions! {
    pub enum ViewAction {
        // Two-pane, left view (§8.3) and right view (§8.4).
        UseLeftBlock = 1,
        UseBothLeftFirst = 2,
        UseBothRightFirst = 3,
        PrependRightToLeft = 4,
        ReplaceLeftByRight = 5,
        AppendRightToLeft = 6,
        UseLeftFile = 7,
        UseRightFile = 8,
        MarkBlock = 9,
        UnmarkBlock = 10,
        LeaveOnlyMarked = 11,
        Copy = 12,
        // Three-pane (conflict editor) block commands.
        UseTheirBlock = 20,
        UseMyBlock = 21,
        UseMineBeforeTheirs = 22,
        UseTheirsBeforeMine = 23,
        UseTheirFile = 24,
        UseMyFile = 25,
    }
}

/// Which pane was right-clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Left,
    Right,
    /// The merged result (three-pane only).
    Bottom,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DiffMenuInput {
    /// The click is inside a block of differences (`bShow`).
    pub in_block: bool,
    /// The left pane may be edited (not a temporary revision file).
    pub left_writable: bool,
    /// The right pane may be edited.
    pub right_writable: bool,
    /// The clicked block is marked (right pane).
    pub marked: bool,
}

/// Two-pane menus. `None` for `Pane::Bottom`.
pub fn two_pane(pane: Pane, i: &DiffMenuInput) -> Vec<Entry> {
    use ViewAction as A;
    let mut m = Menu::new();
    let show = i.in_block;
    match pane {
        Pane::Left => {
            if i.right_writable {
                m.item_if(show, A::UseLeftBlock, "Use th&is text block");
                m.item_if(
                    show,
                    A::UseBothLeftFirst,
                    "Use both text blocks (this one first)",
                );
                m.item_if(
                    show,
                    A::UseBothRightFirst,
                    "Use both text blocks (this one last)",
                );
            }
            if show && i.left_writable {
                m.sep();
                m.item(A::PrependRightToLeft, "Prepend right block");
                m.item(A::ReplaceLeftByRight, "Use right block");
                m.item(A::AppendRightToLeft, "Append right block");
            }
            m.sep();
            m.item_if(i.right_writable, A::UseLeftFile, "Use this &whole file");
            m.item_if(i.left_writable, A::UseRightFile, "Use whole other &file");
        }
        Pane::Right => {
            if i.right_writable {
                m.item_if(show, A::UseLeftBlock, "Use &other text block");
                m.item_if(
                    show,
                    A::UseBothRightFirst,
                    "Use both text blocks (this one first)",
                );
                m.item_if(
                    show,
                    A::UseBothLeftFirst,
                    "Use both text blocks (this one last)",
                );
            }
            if show && i.left_writable {
                m.sep();
                m.item(A::PrependRightToLeft, "Prepend this block to left");
                m.item(A::ReplaceLeftByRight, "Use this block on left");
                m.item(A::AppendRightToLeft, "Append this block to left");
            }
            m.sep();
            if i.right_writable && show {
                m.item_if(!i.marked, A::MarkBlock, "Mark this block");
                m.item_if(i.marked, A::UnmarkBlock, "Unmark this block");
            }
            m.item_if(
                i.right_writable,
                A::LeaveOnlyMarked,
                "Leave only marked blocks",
            );
            m.item_if(i.right_writable, A::UseLeftFile, "Use whole other &file");
            m.item_if(i.left_writable, A::UseRightFile, "Use this &whole file");
        }
        Pane::Bottom => return Vec::new(),
    }
    m.sep();
    m.item_enabled(show, A::Copy, "&Copy");
    m.finish()
}

/// Three-pane (conflict editor) menus: Left = Theirs, Right = Mine, Bottom = Merged.
pub fn three_pane(pane: Pane, in_block: bool) -> Vec<Entry> {
    use ViewAction as A;
    let mut m = Menu::new();
    match pane {
        Pane::Left => {
            m.item_if(in_block, A::UseTheirBlock, "Use th&is text block");
            m.item(A::UseTheirFile, "Use this &whole file");
            m.item_if(
                in_block,
                A::UseMineBeforeTheirs,
                "Use text block from r&ight before left",
            );
            m.item_if(
                in_block,
                A::UseTheirsBeforeMine,
                "Use text block from l&eft before right",
            );
        }
        Pane::Right => {
            m.item_if(in_block, A::UseMyBlock, "Use th&is text block");
            m.item(A::UseMyFile, "Use this &whole file");
            m.item_if(
                in_block,
                A::UseMineBeforeTheirs,
                "Use text block from r&ight before left",
            );
            m.item_if(
                in_block,
                A::UseTheirsBeforeMine,
                "Use text block from l&eft before right",
            );
        }
        Pane::Bottom => {
            // Without a block the bottom view has no block entries at all.
            if !in_block {
                return Vec::new();
            }
            m.item(A::UseTheirBlock, "Use text block from &left");
            m.item(A::UseMyBlock, "Use text block from &right");
            m.item(
                A::UseMineBeforeTheirs,
                "Use text block from r&ight before left",
            );
            m.item(
                A::UseTheirsBeforeMine,
                "Use text block from l&eft before right",
            );
            return m.finish();
        }
    }
    m.sep();
    m.item_enabled(in_block, A::Copy, "&Copy");
    m.finish()
}

menu_actions! {
    pub enum BlameAction {
        /// `| parent_index << 16` (1-based) when there are several parents.
        BlamePrevious = 1,
        /// `| parent_index << 16`.
        CompareWithPrevious = 2,
        ShowLog = 3,
        CopySha = 4,
        CopyLog = 5,
    }
}

pub fn with_parent(a: BlameAction, parent: usize) -> i32 {
    a as i32 | ((parent as i32) << 16)
}

pub fn decode_blame(id: i32) -> Option<(BlameAction, usize)> {
    use super::ActionId;
    Some((BlameAction::from_id(id & 0xFFFF)?, (id >> 16) as usize))
}

/// Blame text-view line menu. `parents_with_file` = number of parents in which
/// the line's file was modified or renamed (0 for an added file).
pub fn blame_line(parents_with_file: usize) -> Vec<Entry> {
    use BlameAction as A;
    let mut m = Menu::new();
    let parents = |a: A| -> Vec<Leaf> {
        (1..=parents_with_file)
            .map(|k| Leaf {
                id: with_parent(a, k),
                title: format!("Parent {k}"),
                enabled: true,
            })
            .collect()
    };
    match parents_with_file {
        0 => {}
        1 => {
            m.raw(
                with_parent(A::BlamePrevious, 1),
                "Blame previous revision",
                true,
            );
            m.raw(
                with_parent(A::CompareWithPrevious, 1),
                "Compare with previous revision",
                true,
            );
        }
        _ => {
            m.sub("Blame previous revision", parents(A::BlamePrevious));
            m.sub(
                "Compare with previous revision",
                parents(A::CompareWithPrevious),
            );
        }
    }
    m.item(A::ShowLog, "Show log");
    m.sep();
    m.item(A::CopySha, "Copy SHA-1 to clipboard");
    m.item(A::CopyLog, "Copy log to clipboard");
    m.finish()
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
    fn two_pane_left_and_right() {
        let i = DiffMenuInput {
            in_block: true,
            right_writable: true,
            ..Default::default()
        };
        assert_eq!(
            titles(&two_pane(Pane::Left, &i)),
            [
                "Use this text block",
                "Use both text blocks (this one first)",
                "Use both text blocks (this one last)",
                "---",
                "Use this whole file",
                "---",
                "Copy"
            ]
        );
        assert_eq!(
            titles(&two_pane(Pane::Right, &i)),
            [
                "Use other text block",
                "Use both text blocks (this one first)",
                "Use both text blocks (this one last)",
                "---",
                "Mark this block",
                "Leave only marked blocks",
                "Use whole other file",
                "---",
                "Copy"
            ]
        );
        let both = DiffMenuInput {
            left_writable: true,
            marked: true,
            ..i
        };
        let r = titles(&two_pane(Pane::Right, &both));
        assert!(r.contains(&"Use this block on left".to_owned()));
        assert!(r.contains(&"Unmark this block".to_owned()));
        let read_only = titles(&two_pane(Pane::Left, &DiffMenuInput::default()));
        assert_eq!(read_only, ["Copy"]);
    }

    #[test]
    fn three_pane_and_blame() {
        assert_eq!(
            titles(&three_pane(Pane::Left, true))[..4],
            [
                "Use this text block",
                "Use this whole file",
                "Use text block from right before left",
                "Use text block from left before right"
            ]
        );
        assert!(three_pane(Pane::Bottom, false).is_empty());
        assert_eq!(titles(&three_pane(Pane::Bottom, true)).len(), 4);
        assert_eq!(
            titles(&blame_line(1)),
            [
                "Blame previous revision",
                "Compare with previous revision",
                "Show log",
                "---",
                "Copy SHA-1 to clipboard",
                "Copy log to clipboard"
            ]
        );
        let t = titles(&blame_line(2));
        assert_eq!(
            t[..2],
            [
                "[Blame previous revision]",
                "[Compare with previous revision]"
            ]
        );
        assert_eq!(titles(&blame_line(0))[0], "Show log");
        assert_eq!(
            decode_blame(with_parent(BlameAction::CompareWithPrevious, 2)),
            Some((BlameAction::CompareWithPrevious, 2))
        );
    }
}
