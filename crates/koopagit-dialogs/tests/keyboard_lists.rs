//! Keyboard parity for the file status list and the dialogs built around it:
//! Commit, Add/Revert/Resolve, Clean, Ignore, Working Tree, Git Init, the input
//! box and Changed Files (docs/spec/spec-keyboard.md §1.1, §2, §3.2, §3.6, §3.9, §6).

mod common;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use common::*;
use koopagit_core::status::{Action, StatusEntry};
use koopagit_dialogs::filelist::{Item, to_row};
use koopagit_dialogs::filelist_actions::{FileListEnv, FileListHost, FileListMenu};
use koopagit_dialogs::filelist_menu::PopMask;
use koopagit_dialogs::*;
use slint::{ComponentHandle, Model, ModelRc, VecModel};

fn item(path: &str, a: Action) -> Item {
    Item {
        entry: StatusEntry {
            path: path.into(),
            old_path: None,
            staged: None,
            unstaged: Some(a),
        },
        checked: false,
        is_dir: false,
    }
}

/// Recorded calls of a callback with arguments.
fn recorder<T: 'static>() -> (Rc<RefCell<Vec<T>>>, impl Fn(T) + 'static) {
    let v = Rc::new(RefCell::new(Vec::new()));
    let c = v.clone();
    (v, move |x| c.borrow_mut().push(x))
}

#[test]
fn file_list_keys_drive_selection_checks_and_activation() {
    init();
    let dir = tempfile::tempdir().unwrap();
    let dlg = FileListDialog::new().unwrap();
    let items = vec![
        item("a.txt", Action::Modified),
        item("b.txt", Action::Modified),
        item("c.txt", Action::Modified),
    ];
    let model = Rc::new(VecModel::from(
        items
            .iter()
            .map(|i| to_row(i, &HashMap::new()))
            .collect::<Vec<_>>(),
    ));
    dlg.set_rows(ModelRc::from(model.clone()));
    let menu = FileListMenu::new(
        FileListEnv::working_tree(0, dir.path(), PopMask::ADD_DLG, "Add"),
        FileListHost {
            items: Box::new(move || items.clone()),
            model: model.clone(),
            refresh: Box::new(|| {}),
        },
    );
    FileListMenu::install(&dlg.global::<FileListBridge>(), std::slice::from_ref(&menu));
    let (activated, on_activated) = recorder();
    dlg.on_row_activated(on_activated);
    let (toggled, on_toggled) = recorder();
    dlg.on_row_toggled(move |i, c| on_toggled((i, c)));
    {
        let weak = dlg.as_weak();
        menu.set_on_activate(move |r| weak.unwrap().invoke_row_activated(r as i32));
        let weak = dlg.as_weak();
        menu.set_on_toggle(move |r, c| weak.unwrap().invoke_row_toggled(r as i32, c));
    }
    let (cancel, on_cancel) = counter();
    dlg.on_cancel(on_cancel);
    let (ok, on_ok) = counter();
    dlg.on_ok(on_ok);
    dlg.show().unwrap();

    // The list has the focus: Down selects the first row, Shift+Down extends.
    press(dlg.window(), K::DownArrow);
    shift(dlg.window(), K::DownArrow);
    assert_eq!(menu.selected_rows(), [0, 1]);
    // Space checks every selected row; Enter diffs every selected row.
    press(dlg.window(), " ");
    assert_eq!(*toggled.borrow(), [(0, true), (1, true)]);
    assert!(model.row_data(0).unwrap().checked && !model.row_data(2).unwrap().checked);
    press(dlg.window(), K::Return);
    assert_eq!(*activated.borrow(), [0, 1]);
    assert_eq!(ok.get(), 0, "Enter in the list does not press OK");
    ctrl(dlg.window(), "a");
    assert_eq!(menu.selected_rows(), [0, 1, 2]);
    assert!(model.row_data(2).unwrap().selected);
    // Delete on a versioned row does nothing.
    press(dlg.window(), K::Delete);
    // Escape closes the owning dialog.
    press(dlg.window(), K::Escape);
    assert_eq!(cancel.get(), 1);
}

#[test]
fn file_list_routes_copy_and_delete_to_the_bridge() {
    init();
    let dlg = FileListDialog::new().unwrap();
    let bridge = dlg.global::<FileListBridge>();
    let (copies, on_copy) = recorder();
    bridge.on_copy(move |list, status| on_copy((list, status)));
    let (deletes, on_delete) = recorder();
    bridge.on_delete(move |list, permanent| on_delete((list, permanent)));
    let (spaces, on_space) = recorder();
    bridge.on_space(on_space);
    dlg.show().unwrap();
    ctrl(dlg.window(), "c");
    chord(dlg.window(), [K::Control, K::Shift], "C");
    ctrl(dlg.window(), K::Insert);
    assert_eq!(*copies.borrow(), [(0, false), (0, true), (0, false)]);
    press(dlg.window(), K::Delete);
    shift(dlg.window(), K::Delete);
    assert_eq!(*deletes.borrow(), [(0, false), (0, true)]);
    press(dlg.window(), " ");
    assert_eq!(*spaces.borrow(), [0]);
}

#[test]
fn add_revert_resolve_keys() {
    init();
    let dlg = FileListDialog::new().unwrap();
    enable_access_keys!(dlg);
    dlg.set_extra_check_text("Include ignored files".into());
    let (ok, on_ok) = counter();
    dlg.on_ok(on_ok);
    let (cancel, on_cancel) = counter();
    dlg.on_cancel(on_cancel);
    let (refresh, on_refresh) = counter();
    dlg.on_refresh(on_refresh);
    let (all, on_all) = recorder();
    dlg.on_select_all(on_all);
    let (extra, on_extra) = counter();
    dlg.on_extra_toggled(on_extra);
    dlg.show().unwrap();
    ctrl(dlg.window(), K::Return);
    assert_eq!(ok.get(), 1);
    press(dlg.window(), K::F5);
    assert_eq!(refresh.get(), 1);
    alt(dlg.window(), "a");
    alt(dlg.window(), "a");
    assert_eq!(*all.borrow(), [true, false]);
    alt(dlg.window(), "i");
    assert!(dlg.get_extra_checked());
    assert_eq!(extra.get(), 1);
    // Ctrl+Enter works from the focused checkbox too.
    ctrl(dlg.window(), K::Return);
    assert_eq!(ok.get(), 2);
    press(dlg.window(), K::Escape);
    assert_eq!(cancel.get(), 1);
}

#[test]
fn commit_ctrl_enter_escape_and_f5() {
    init();
    let dlg = CommitDialog::new().unwrap();
    dlg.set_ok_enabled(true);
    let (commit, on_commit) = counter();
    dlg.on_commit(on_commit);
    let (cancel, on_cancel) = counter();
    dlg.on_cancel(on_cancel);
    let (refresh, on_refresh) = counter();
    dlg.on_refresh(on_refresh);
    dlg.show().unwrap();
    // The message editor has the focus: Enter is a newline, Ctrl+Enter commits.
    type_text(dlg.window(), "fix");
    press(dlg.window(), K::Return);
    assert_eq!(dlg.get_message().as_str(), "fix\n");
    assert_eq!(commit.get(), 0);
    ctrl(dlg.window(), K::Return);
    assert_eq!(commit.get(), 1);
    assert_eq!(dlg.get_message().as_str(), "fix\n");
    dlg.set_ok_enabled(false);
    ctrl(dlg.window(), K::Return);
    assert_eq!(commit.get(), 1, "Ctrl+Enter needs an enabled OK button");
    press(dlg.window(), K::F5);
    assert_eq!(refresh.get(), 1);
    press(dlg.window(), K::Escape);
    assert_eq!(cancel.get(), 1);
}

#[test]
fn commit_tab_leaves_the_message_editor() {
    init();
    let dlg = CommitDialog::new().unwrap();
    dlg.show().unwrap();
    type_text(dlg.window(), "a");
    press(dlg.window(), K::Tab);
    type_text(dlg.window(), "b");
    assert_eq!(dlg.get_message().as_str(), "a");
}

#[test]
fn commit_bug_id_enter_moves_to_the_message() {
    init();
    let dlg = CommitDialog::new().unwrap();
    dlg.set_bug_id_visible(true);
    dlg.set_ok_enabled(true);
    let (commit, on_commit) = counter();
    dlg.on_commit(on_commit);
    dlg.show().unwrap();
    // Shift+Tab from the message editor reaches the Bug-ID edit.
    shift(dlg.window(), K::Tab);
    type_text(dlg.window(), "42");
    assert_eq!(dlg.get_bug_id().as_str(), "42");
    press(dlg.window(), K::Return);
    assert_eq!(commit.get(), 0);
    type_text(dlg.window(), "m");
    assert_eq!(dlg.get_message().as_str(), "m");
}

#[test]
fn commit_access_keys() {
    init();
    let dlg = CommitDialog::new().unwrap();
    enable_access_keys!(dlg);
    dlg.set_ok_enabled(true);
    let (commit, on_commit) = counter();
    dlg.on_commit(on_commit);
    let (refresh, on_refresh) = counter();
    dlg.on_refresh(on_refresh);
    let (amend, on_amend) = counter();
    dlg.on_amend_toggled(on_amend);
    let (links, on_links) = recorder();
    dlg.on_check_links(move |i, shift| on_links((i, shift)));
    dlg.show().unwrap();

    alt(dlg.window(), "l");
    assert!(dlg.get_amend());
    assert_eq!(amend.get(), 1);
    alt(dlg.window(), "u");
    assert!(dlg.get_show_unversioned());
    assert_eq!(refresh.get(), 1);
    alt(dlg.window(), "y");
    assert!(dlg.get_message_only());
    alt(dlg.window(), "t");
    assert!(dlg.get_set_author());
    // &All / &None; Shift inverts.
    alt(dlg.window(), "a");
    chord(dlg.window(), [K::Alt, K::Shift], "A");
    alt(dlg.window(), "n");
    assert_eq!(*links.borrow(), [(0, false), (0, true), (1, false)]);
    // Alt+F focuses the file list, Alt+M the message.
    alt(dlg.window(), "f");
    type_text(dlg.window(), "x");
    assert_eq!(dlg.get_message().as_str(), "");
    alt(dlg.window(), "m");
    type_text(dlg.window(), "x");
    assert_eq!(dlg.get_message().as_str(), "x");
    // Split-button entries select the action and commit.
    alt(dlg.window(), "p");
    assert_eq!(dlg.get_commit_action(), 2);
    alt(dlg.window(), "r");
    assert_eq!(dlg.get_commit_action(), 1);
    alt(dlg.window(), "o");
    assert_eq!(dlg.get_commit_action(), 0);
    assert_eq!(commit.get(), 3);
    dlg.set_ok_enabled(false);
    alt(dlg.window(), "p");
    assert_eq!(commit.get(), 3);
    assert_eq!(dlg.get_commit_action(), 0);
}

#[test]
fn commit_check_links_are_tab_stops() {
    init();
    let dlg = CommitDialog::new().unwrap();
    let (links, on_links) = recorder();
    dlg.on_check_links(move |i, shift| on_links((i, shift)));
    dlg.show().unwrap();
    for _ in 0..20 {
        press(dlg.window(), K::Tab);
        press(dlg.window(), " ");
        if !links.borrow().is_empty() {
            break;
        }
    }
    assert_eq!(*links.borrow(), [(0, false)], "Space on the \"All\" link");
    // Enter on a focused link activates it; Shift+Space inverts.
    press(dlg.window(), K::Return);
    shift(dlg.window(), " ");
    assert_eq!(*links.borrow(), [(0, false), (0, false), (0, true)]);
}

#[test]
fn input_dialog_keys() {
    init();
    let dlg = InputDialog::new().unwrap();
    let (ok, on_ok) = counter();
    dlg.on_ok(on_ok);
    let (cancel, on_cancel) = counter();
    dlg.on_cancel(on_cancel);
    dlg.show().unwrap();
    type_text(dlg.window(), "v");
    ctrl(dlg.window(), K::Return);
    press(dlg.window(), K::Return);
    assert_eq!(ok.get(), 2);
    press(dlg.window(), K::Escape);
    assert_eq!(cancel.get(), 1);
    assert_eq!(dlg.get_value().as_str(), "v");
}

#[test]
fn clean_ignore_and_init_dialogs() {
    init();
    let clean = CleanDialog::new().unwrap();
    enable_access_keys!(clean);
    let (ok, on_ok) = counter();
    clean.on_ok(on_ok);
    let (cancel, on_cancel) = counter();
    clean.on_cancel(on_cancel);
    clean.set_clean_type(2);
    clean.show().unwrap();
    alt(clean.window(), "n");
    assert_eq!(clean.get_clean_type(), 1);
    alt(clean.window(), "a");
    assert_eq!(clean.get_clean_type(), 0);
    press(clean.window(), K::Return);
    press(clean.window(), K::Escape);
    assert_eq!((ok.get(), cancel.get()), (1, 1));

    let ignore = IgnoreDialog::new().unwrap();
    let (ok, on_ok) = counter();
    ignore.on_ok(on_ok);
    let (cancel, on_cancel) = counter();
    ignore.on_cancel(on_cancel);
    ignore.show().unwrap();
    press(ignore.window(), K::Return);
    press(ignore.window(), K::Escape);
    assert_eq!((ok.get(), cancel.get()), (1, 1));

    let init_dlg = CreateRepoDialog::new().unwrap();
    let (ok, on_ok) = counter();
    init_dlg.on_ok(on_ok);
    let (cancel, on_cancel) = counter();
    init_dlg.on_cancel(on_cancel);
    init_dlg.show().unwrap();
    press(init_dlg.window(), K::Return);
    press(init_dlg.window(), K::Escape);
    assert_eq!((ok.get(), cancel.get()), (1, 1));
}

#[test]
fn working_tree_keys() {
    init();
    let dlg = WorkingTreeDialog::new().unwrap();
    enable_access_keys!(dlg);
    let (refresh, on_refresh) = counter();
    dlg.on_refresh(on_refresh);
    let (close, on_close) = counter();
    dlg.on_close_dialog(on_close);
    dlg.show().unwrap();
    press(dlg.window(), K::F5);
    assert_eq!(refresh.get(), 1);
    alt(dlg.window(), "s");
    assert!(!dlg.get_show_staged());
    alt(dlg.window(), "g");
    assert!(dlg.get_show_ignored());
    alt(dlg.window(), "r");
    assert_eq!(refresh.get(), 4);
    press(dlg.window(), K::Escape);
    assert_eq!(close.get(), 1);
}

#[test]
fn file_diff_keys() {
    init();
    let dlg = FileDiffDialog::new().unwrap();
    let (refresh, on_refresh) = counter();
    dlg.on_rev_entered(on_refresh);
    let (close, on_close) = counter();
    dlg.on_close_dialog(on_close);
    let (filtered, on_filtered) = counter();
    dlg.on_filter_edited(on_filtered);
    let bridge = dlg.global::<FileListBridge>();
    let (copies, on_copy) = recorder();
    bridge.on_copy(move |list, status| on_copy((list, status)));
    let (activations, on_activate) = recorder();
    bridge.on_activate(move |list| {
        on_activate(list);
        true
    });
    dlg.show().unwrap();
    // The file list has the focus.
    press(dlg.window(), K::Return);
    ctrl(dlg.window(), "c");
    assert_eq!(*activations.borrow(), [1]);
    assert_eq!(*copies.borrow(), [(1, false)]);
    press(dlg.window(), K::F5);
    assert_eq!(refresh.get(), 1);
    // Shift+Tab twice: Refresh button, then the filter edit.
    shift(dlg.window(), K::Tab);
    shift(dlg.window(), K::Tab);
    type_text(dlg.window(), "src");
    assert_eq!(dlg.get_filter_text().as_str(), "src");
    // Enter outside the list does not close (OnOK is empty).
    press(dlg.window(), K::Return);
    assert_eq!(close.get(), 0);
    // Escape clears a non-empty filter first, then closes.
    press(dlg.window(), K::Escape);
    assert_eq!(dlg.get_filter_text().as_str(), "");
    assert_eq!(close.get(), 0);
    assert!(filtered.get() >= 1);
    press(dlg.window(), K::Escape);
    assert_eq!(close.get(), 1);
}
