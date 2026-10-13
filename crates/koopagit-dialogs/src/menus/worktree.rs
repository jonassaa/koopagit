//! Worktree List context menu (`CWorktreeListDlg::ShowContextMenu`,
//! `WorktreeListDlg.cpp:279-430`).

use super::{Entry, Menu};

menu_actions! {
    pub enum WorktreeAction {
        Open = 1,
        Lock = 2,
        Unlock = 3,
        Remove = 4,
        RemoveWithForce = 5,
    }
}

/// What the menu needs to know about one selected row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorktreeRow {
    pub is_main: bool,
    pub locked: bool,
}

/// Builds the menu for the selected rows. With several rows every action is
/// offered (rows it doesn't apply to are skipped when it runs).
pub fn menu(selected: &[WorktreeRow]) -> Vec<Entry> {
    let mut m = Menu::new();
    let Some(first) = selected.first() else {
        return Vec::new();
    };
    let (mut show_lock, mut show_unlock, mut show_remove) = (true, true, true);
    if selected.len() == 1 {
        show_unlock = first.locked;
        show_lock = !show_unlock;
        if first.is_main {
            show_remove = false;
        }
        m.item(WorktreeAction::Open, "Explore to");
    }
    m.item_if(show_lock, WorktreeAction::Lock, "Lock")
        .item_if(show_unlock, WorktreeAction::Unlock, "Unlock")
        .item_if(show_remove, WorktreeAction::Remove, "&Remove")
        .item_if(show_remove, WorktreeAction::RemoveWithForce, "Force remove");
    m.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menus::ActionId;

    fn ids(e: &[Entry]) -> Vec<i32> {
        e.iter()
            .filter_map(|e| match e {
                Entry::Item(l) => Some(l.id),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn single_and_multi_selection() {
        let main = WorktreeRow {
            is_main: true,
            locked: false,
        };
        let locked = WorktreeRow {
            is_main: false,
            locked: true,
        };
        assert_eq!(
            ids(&menu(&[main])),
            [WorktreeAction::Open.id(), WorktreeAction::Lock.id()]
        );
        assert_eq!(
            ids(&menu(&[locked])),
            [
                WorktreeAction::Open.id(),
                WorktreeAction::Unlock.id(),
                WorktreeAction::Remove.id(),
                WorktreeAction::RemoveWithForce.id()
            ]
        );
        assert_eq!(ids(&menu(&[main, locked])).len(), 4);
        assert!(menu(&[]).is_empty());
    }
}
