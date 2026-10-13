//! Repository Browser context menu (`CRepositoryBrowser::ShowContextMenu`,
//! `RepositoryBrowser.cpp:727-915`).

use super::{Entry, Menu};

menu_actions! {
    pub enum RepoBrowserAction {
        Open = 1,
        OpenWith = 2,
        OpenWithAlternativeEditor = 3,
        CompareWorkingTree = 4,
        ShowLog = 5,
        ShowLogSubmodule = 6,
        Blame = 7,
        SaveAs = 8,
        Revert = 9,
        MarkForComparison = 10,
        DiffWithMarked = 11,
        CopyPaths = 12,
        CopyHash = 13,
    }
}

/// `eSelectionType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionType {
    OnlyFiles,
    OnlyFilesSubmodules,
    OnlyFolders,
    Mixed,
}

impl SelectionType {
    /// Classifies a selection from `(is_folder, is_submodule)` pairs
    /// (`OnContextMenu_RepoList`). Submodules count as files.
    pub fn of(items: &[(bool, bool)]) -> Self {
        let folders = items.iter().any(|(f, _)| *f);
        let files = items.iter().any(|(f, _)| !*f);
        let submodules = items.iter().any(|(_, s)| *s);
        if folders && files {
            SelectionType::Mixed
        } else if folders {
            SelectionType::OnlyFolders
        } else if submodules {
            SelectionType::OnlyFilesSubmodules
        } else {
            SelectionType::OnlyFiles
        }
    }

    fn files(self) -> bool {
        matches!(
            self,
            SelectionType::OnlyFiles | SelectionType::OnlyFilesSubmodules
        )
    }
}

/// Inputs of the menu builder.
#[derive(Debug, Clone, Default)]
pub struct MenuState {
    pub count: usize,
    pub first_is_submodule: bool,
    /// The repository has a working tree (`m_bHasWC`).
    pub has_wc: bool,
    /// Text for "Diff with "<marked>"" when something is marked.
    pub marked: Option<String>,
}

pub fn menu(sel: SelectionType, st: &MenuState) -> Vec<Entry> {
    let mut m = Menu::new();
    if st.count == 0 {
        return Vec::new();
    }
    let only_files = sel == SelectionType::OnlyFiles;
    if st.count == 1 {
        m.item(RepoBrowserAction::Open, "&Open");
        if sel.files() {
            m.item(RepoBrowserAction::OpenWith, "Open with...").item(
                RepoBrowserAction::OpenWithAlternativeEditor,
                "View revision in alternative editor",
            );
        }
        m.sep();
        if st.has_wc && sel.files() {
            m.item(
                RepoBrowserAction::CompareWorkingTree,
                "Compare with &working tree",
            )
            .sep();
        }
        m.item(RepoBrowserAction::ShowLog, "Show &log");
        if st.first_is_submodule {
            m.item(RepoBrowserAction::ShowLogSubmodule, "Show log of submodule");
        }
        if only_files {
            if st.has_wc {
                m.item(RepoBrowserAction::Blame, "Blame");
            }
            m.sep()
                .item(RepoBrowserAction::SaveAs, "Save revision &to...");
        }
    }
    if only_files && st.has_wc {
        m.item(RepoBrowserAction::Revert, "R&evert to this revision");
    }
    m.sep();
    if st.count == 1 && only_files {
        m.item(RepoBrowserAction::MarkForComparison, "Mark for comparison");
        if let Some(marked) = &st.marked {
            m.item(
                RepoBrowserAction::DiffWithMarked,
                format!("Diff with \"{marked}\""),
            );
        }
        m.sep();
    }
    m.item(RepoBrowserAction::CopyPaths, "Copy paths to clipboard")
        .item(RepoBrowserAction::CopyHash, "Copy SHA-1 to clipboard");
    m.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menus::{ActionId, Entry};

    fn titles(e: &[Entry]) -> Vec<String> {
        e.iter()
            .map(|e| match e {
                Entry::Item(l) => l.title.clone(),
                Entry::Separator => "---".into(),
                Entry::Sub { title, .. } => title.clone(),
            })
            .collect()
    }

    #[test]
    fn single_file_with_working_tree() {
        let st = MenuState {
            count: 1,
            has_wc: true,
            marked: Some("a.txt:1234".into()),
            ..Default::default()
        };
        assert_eq!(
            titles(&menu(SelectionType::OnlyFiles, &st)),
            [
                "&Open",
                "Open with...",
                "View revision in alternative editor",
                "---",
                "Compare with &working tree",
                "---",
                "Show &log",
                "Blame",
                "---",
                "Save revision &to...",
                "R&evert to this revision",
                "---",
                "Mark for comparison",
                "Diff with \"a.txt:1234\"",
                "---",
                "Copy paths to clipboard",
                "Copy SHA-1 to clipboard",
            ]
        );
    }

    #[test]
    fn folders_and_multi_selection() {
        let one_folder = MenuState {
            count: 1,
            has_wc: true,
            ..Default::default()
        };
        assert_eq!(
            titles(&menu(SelectionType::OnlyFolders, &one_folder)),
            [
                "&Open",
                "---",
                "Show &log",
                "---",
                "Copy paths to clipboard",
                "Copy SHA-1 to clipboard"
            ]
        );
        let many = MenuState {
            count: 3,
            has_wc: true,
            ..Default::default()
        };
        assert_eq!(
            titles(&menu(SelectionType::OnlyFiles, &many)),
            [
                "R&evert to this revision",
                "---",
                "Copy paths to clipboard",
                "Copy SHA-1 to clipboard"
            ]
        );
        assert_eq!(
            SelectionType::of(&[(true, false), (false, false)]),
            SelectionType::Mixed
        );
        assert_eq!(
            SelectionType::of(&[(false, true)]),
            SelectionType::OnlyFilesSubmodules
        );
        assert_eq!(
            RepoBrowserAction::from_id(13),
            Some(RepoBrowserAction::CopyHash)
        );
    }
}
