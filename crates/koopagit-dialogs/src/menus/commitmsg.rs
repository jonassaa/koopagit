//! Commit message editor menu (`CSciEdit::OnContextMenu` + the Commit dialog's
//! handler entries, spec-menus.md §7).

use super::{Entry, Leaf, Menu};

menu_actions! {
    pub enum MessageAction {
        Cut = 1,
        Copy = 2,
        Paste = 3,
        SelectAll = 4,
        PickCommitHash = 5,
        PickCommitMessage = 6,
        PasteFileList = 7,
        PasteLastMessage = 8,
        /// `| index << 16`: one recent message.
        RecentMessage = 9,
    }
}

/// Longest label shown for a recent message in the submenu.
const RECENT_LABEL_CHARS: usize = 60;

#[derive(Debug, Clone, Default)]
pub struct MessageMenuInput {
    pub read_only: bool,
    /// Commit-message history, newest first (`History\commit<repo>` / `logmsgs`).
    pub history: Vec<String>,
}

pub fn recent_id(index: usize) -> i32 {
    MessageAction::RecentMessage as i32 | ((index as i32) << 16)
}

/// Splits an id into the action and the recent-message index.
pub fn decode(id: i32) -> Option<(MessageAction, usize)> {
    use super::ActionId;
    Some((MessageAction::from_id(id & 0xFFFF)?, (id >> 16) as usize))
}

/// One-line label for a history entry.
pub fn recent_label(message: &str) -> String {
    let first = message
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    let mut label: String = first.chars().take(RECENT_LABEL_CHARS).collect();
    if first.chars().count() > RECENT_LABEL_CHARS || message.trim().lines().count() > 1 {
        label.push('…');
    }
    // `&` would become an accelerator on Windows.
    label.replace('&', "&&")
}

pub fn build(i: &MessageMenuInput) -> Vec<Entry> {
    use MessageAction as A;
    let mut m = Menu::new();
    m.item_enabled(!i.read_only, A::Cut, "Cu&t");
    m.item(A::Copy, "Cop&y");
    m.item_enabled(!i.read_only, A::Paste, "&Paste");
    m.sep();
    m.item(A::SelectAll, "Select &All");
    m.sep();
    m.item(A::PickCommitHash, "Pick commit &hash");
    m.item(A::PickCommitMessage, "Pick commit &message");
    m.item(A::PasteFileList, "Paste &filename list");
    if !i.history.is_empty() {
        m.item(A::PasteLastMessage, "Paste &last commit message");
        let recent: Vec<Leaf> = i
            .history
            .iter()
            .enumerate()
            .map(|(k, msg)| Leaf {
                id: recent_id(k),
                title: recent_label(msg),
                enabled: true,
            })
            .collect();
        m.sub("Paste r&ecent message", recent);
    }
    m.finish()
}

/// "Paste filename list": `"%-10s %s\r\n"` per checked row (status name, path).
/// Unversioned rows are labelled "Added".
pub fn file_list_text<'a>(rows: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    rows.into_iter()
        .map(|(status, path)| {
            let status = if status == "Unknown" { "Added" } else { status };
            format!("{status:<10} {path}\r\n")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(e: &[Entry]) -> Vec<String> {
        e.iter()
            .map(|e| match e {
                Entry::Item(l) => l.title.replace('&', ""),
                Entry::Separator => "---".into(),
                Entry::Sub { title, .. } => format!("[{}]", title.replace('&', "")),
            })
            .collect()
    }

    #[test]
    fn menu_with_and_without_history() {
        let empty = titles(&build(&MessageMenuInput::default()));
        assert_eq!(
            empty,
            [
                "Cut",
                "Copy",
                "Paste",
                "---",
                "Select All",
                "---",
                "Pick commit hash",
                "Pick commit message",
                "Paste filename list"
            ]
        );
        let e = build(&MessageMenuInput {
            history: vec!["Fix bug\n\nbody".into(), "a & b".into()],
            ..Default::default()
        });
        let t = titles(&e);
        assert_eq!(
            t[t.len() - 2..],
            ["Paste last commit message", "[Paste recent message]"]
        );
        match e.last().unwrap() {
            Entry::Sub { children, .. } => {
                assert_eq!(children[0].title, "Fix bug…");
                assert_eq!(children[1].title, "a && b");
                assert_eq!(
                    decode(children[1].id),
                    Some((MessageAction::RecentMessage, 1))
                );
            }
            other => panic!("{other:?}"),
        }
        let ro = build(&MessageMenuInput {
            read_only: true,
            ..Default::default()
        });
        assert!(matches!(&ro[0], Entry::Item(l) if !l.enabled));
    }

    #[test]
    fn file_list_format() {
        assert_eq!(
            file_list_text([("Modified", "src/a.rs"), ("Unknown", "new.txt")]),
            "Modified   src/a.rs\r\nAdded      new.txt\r\n"
        );
    }
}
