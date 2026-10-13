//! Model helpers for the GitStatusListCtrl-like [`crate::FileRow`] list.

use std::collections::HashMap;

use koopagit_core::status::{Action, StatusEntry};

use crate::FileRow;

/// One list row with its backing status entry.
#[derive(Debug, Clone)]
pub struct Item {
    pub entry: StatusEntry,
    pub checked: bool,
    pub is_dir: bool,
}

/// The "Check:" links of the commit dialog, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckLink {
    All,
    None,
    Unversioned,
    Versioned,
    Added,
    Deleted,
    Modified,
    Files,
    Submodules,
}

impl CheckLink {
    pub const ALL: [CheckLink; 9] = [
        CheckLink::All,
        CheckLink::None,
        CheckLink::Unversioned,
        CheckLink::Versioned,
        CheckLink::Added,
        CheckLink::Deleted,
        CheckLink::Modified,
        CheckLink::Files,
        CheckLink::Submodules,
    ];

    pub fn matches(self, item: &Item) -> bool {
        let a = item.entry.action();
        match self {
            CheckLink::All | CheckLink::None => true,
            CheckLink::Unversioned => a == Action::Unversioned,
            CheckLink::Versioned => !matches!(a, Action::Unversioned | Action::Ignored),
            CheckLink::Added => matches!(a, Action::Added | Action::Copied),
            CheckLink::Deleted => matches!(a, Action::Deleted | Action::Missing),
            CheckLink::Modified => matches!(a, Action::Modified | Action::Renamed),
            CheckLink::Files => !item.is_dir,
            CheckLink::Submodules => item.is_dir,
        }
    }

    /// Clicking checks the matching rows ("None" unchecks all); Shift+click unchecks them.
    pub fn apply(self, items: &mut [Item], shift: bool) {
        for item in items.iter_mut().filter(|i| self.matches(i)) {
            item.checked = match self {
                CheckLink::None => false,
                _ => !shift,
            };
        }
    }
}

/// Colour class used by the Slint list (see `common.slint`).
fn kind(a: Action) -> i32 {
    match a {
        Action::Added | Action::Copied => 1,
        Action::Deleted | Action::Missing => 2,
        Action::Modified => 3,
        Action::Unmerged => 4,
        Action::Renamed => 5,
        Action::Unversioned | Action::Ignored => 6,
    }
}

/// Path column text: renames show `new (from old)`.
pub fn path_text(e: &StatusEntry) -> String {
    match &e.old_path {
        Some(old) => format!("{} (from {old})", e.path),
        None => e.path.clone(),
    }
}

pub fn extension(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name.rfind('.') {
        Some(i) if i > 0 => name[i..].to_owned(),
        _ => String::new(),
    }
}

pub fn to_row(item: &Item, numstat: &HashMap<String, (u32, u32)>) -> FileRow {
    let a = item.entry.action();
    let (add, del) = numstat
        .get(&item.entry.path)
        .map(|(a, d)| (a.to_string(), d.to_string()))
        .unwrap_or_default();
    FileRow {
        checked: item.checked,
        path: path_text(&item.entry).into(),
        extension: extension(&item.entry.path).into(),
        status: a.name().into(),
        added: add.into(),
        removed: del.into(),
        kind: kind(a),
        // Set by `FileListMenu::after_rebuild` from the list's selection.
        selected: false,
    }
}

/// `%1!d! files selected, %2!d! files total`.
pub fn stats_text(items: &[Item]) -> String {
    let selected = items.iter().filter(|i| i.checked).count();
    format!("{selected} files selected, {} files total", items.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(path: &str, unstaged: Action, is_dir: bool) -> Item {
        Item {
            entry: StatusEntry {
                path: path.into(),
                old_path: None,
                staged: None,
                unstaged: Some(unstaged),
            },
            checked: false,
            is_dir,
        }
    }

    #[test]
    fn links_and_texts() {
        let mut items = vec![
            item("a.rs", Action::Modified, false),
            item("n.txt", Action::Unversioned, false),
            item("sub", Action::Modified, true),
        ];
        CheckLink::Versioned.apply(&mut items, false);
        assert_eq!(
            items.iter().map(|i| i.checked).collect::<Vec<_>>(),
            [true, false, true]
        );
        CheckLink::Submodules.apply(&mut items, true);
        assert!(!items[2].checked);
        assert_eq!(stats_text(&items), "1 files selected, 3 files total");
        CheckLink::None.apply(&mut items, false);
        assert!(items.iter().all(|i| !i.checked));
        assert_eq!(extension("dir.x/file.tar.gz"), ".gz");
        assert_eq!(extension(".gitignore"), "");
        let row = to_row(&items[1], &HashMap::new());
        assert_eq!(row.status.as_str(), "Unknown");
    }
}
