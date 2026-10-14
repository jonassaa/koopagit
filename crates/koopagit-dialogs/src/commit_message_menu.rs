//! The Commit dialog's message-editor context menu (spec-menus.md §7):
//! clipboard entries, "Pick commit hash/message", "Paste filename list",
//! "Paste last commit message" and the recent messages.

use std::path::PathBuf;
use std::rc::Rc;

use koopagit_core::launcher::Invocation;
use slint::ComponentHandle;

use crate::CommitDialog;
use crate::menus::MenuCache;
use crate::menus::commitmsg::{self as menu, MessageAction as A, MessageMenuInput, decode};

/// Inserts `text` at the caret (replacing the selection). `TextEdit` has no
/// insert API, so this goes through the clipboard and restores it afterwards.
fn insert(dlg: &CommitDialog, text: &str) {
    let mut cb = arboard::Clipboard::new().ok();
    let previous = cb.as_mut().and_then(|c| c.get_text().ok());
    let staged = cb
        .as_mut()
        .is_some_and(|c| c.set_text(text.to_owned()).is_ok());
    if staged {
        dlg.invoke_message_focus();
        dlg.invoke_message_paste();
        if let (Some(c), Some(p)) = (cb.as_mut(), previous) {
            let _ = c.set_text(p);
        }
    } else {
        // No clipboard: append instead.
        let mut m = dlg.get_message().to_string();
        m.push_str(text);
        dlg.set_message(m.into());
    }
}

/// Wires the menu. `history` is the commit-message history (newest first);
/// `checked_rows` returns `(status name, path)` of the checked file rows.
pub fn wire(
    dlg: &CommitDialog,
    workdir: PathBuf,
    history: Vec<String>,
    checked_rows: Rc<dyn Fn() -> Vec<(String, String)>>,
) {
    let cache = Rc::new(MenuCache::default());
    {
        let history = history.clone();
        let cache = cache.clone();
        // The history is fixed while the dialog is open, so one cached menu suffices.
        dlg.on_message_menu(move || {
            let history = history.clone();
            cache.get(0, move || {
                menu::build(&MessageMenuInput {
                    read_only: false,
                    history,
                })
            })
        });
    }
    let weak = dlg.as_weak();
    dlg.on_message_menu_action(move |id| {
        let Some(d) = weak.upgrade() else { return };
        let Some((action, index)) = decode(id) else {
            return;
        };
        match action {
            A::Cut => d.invoke_message_cut(),
            A::Copy => d.invoke_message_copy(),
            A::Paste => d.invoke_message_paste(),
            A::SelectAll => d.invoke_message_select_all(),
            A::PasteFileList => {
                let rows = checked_rows();
                let text = menu::file_list_text(rows.iter().map(|(s, p)| (s.as_str(), p.as_str())));
                insert(&d, &text);
            }
            A::PasteLastMessage => {
                if let Some(m) = history.first() {
                    insert(&d, m);
                }
            }
            A::RecentMessage => {
                if let Some(m) = history.get(index) {
                    // Replace the text when it still equals the template/empty.
                    if d.get_message().trim().is_empty() {
                        d.set_message(m.as_str().into());
                    } else {
                        insert(&d, m);
                    }
                }
            }
            A::PickCommitHash | A::PickCommitMessage => {
                let weak = d.as_weak();
                let wd = workdir.clone();
                let want_message = action == A::PickCommitMessage;
                // `/command:log /outfile:<tmp>`: a single-select commit picker.
                let inv = Invocation::new("log").with_paths([workdir.clone()]);
                crate::pick::launcher_picker(inv, move |id| {
                    let text = if want_message {
                        koopagit_core::git_cli::Git::new(&wd)
                            .run(["log", "-1", "--format=%B", &id])
                            .map(|m| m.trim_end().replace('\n', "\r\n"))
                            .unwrap_or_default()
                    } else {
                        id
                    };
                    if let Some(d) = weak.upgrade() {
                        insert(&d, &text);
                    }
                });
            }
        }
    });
}
