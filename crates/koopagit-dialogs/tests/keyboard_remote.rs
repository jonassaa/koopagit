//! Keyboard parity for Rebase, Sync, Push, Pull/Fetch, Clone, the progress
//! dialog and the message box (docs/spec/spec-keyboard.md §0, §3.3-§3.6, §6).

mod common;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use common::*;
use koopagit_dialogs::*;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

fn strings(items: &[&str]) -> ModelRc<SharedString> {
    let v: Vec<SharedString> = items.iter().map(|s| (*s).into()).collect();
    ModelRc::new(VecModel::from(v))
}

/// Records every call of a callback with one argument.
fn recorder<T: Clone + 'static>() -> (Rc<RefCell<Vec<T>>>, impl Fn(T) + 'static) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let l = log.clone();
    (log, move |v| l.borrow_mut().push(v))
}

// ------------------------------------------------------------------ Rebase

#[test]
fn rebase_enter_continues_and_escape_aborts() {
    init();
    let dlg = RebaseDialog::new().unwrap();
    let (go, on_go) = counter();
    let (abort, on_abort) = counter();
    dlg.on_go(on_go);
    dlg.on_abort(on_abort);
    dlg.show().unwrap();
    press(dlg.window(), K::Return);
    assert_eq!(go.get(), 1);
    press(dlg.window(), K::Escape);
    assert_eq!(abort.get(), 1);
    ctrl(dlg.window(), K::Return);
    assert_eq!(go.get(), 2);
    // Continue disabled: neither Enter nor Ctrl+Enter press it.
    dlg.set_continue_enabled(false);
    press(dlg.window(), K::Return);
    ctrl(dlg.window(), K::Return);
    assert_eq!(go.get(), 2);
}

#[test]
fn rebase_f5_refreshes() {
    init();
    let dlg = RebaseDialog::new().unwrap();
    let (refresh, on_refresh) = counter();
    dlg.on_refresh(on_refresh);
    dlg.show().unwrap();
    press(dlg.window(), K::F5);
    assert_eq!(refresh.get(), 1);
}

#[test]
fn rebase_list_keys_set_actions_and_move_rows() {
    init();
    let dlg = RebaseDialog::new().unwrap();
    let (actions, on_action) = recorder::<i32>();
    let (moves, on_move) = recorder::<(i32, bool)>();
    let (steps, on_step) = recorder::<(i32, bool)>();
    dlg.on_set_action(on_action);
    dlg.on_move_rows(move |d, e| on_move((d, e)));
    dlg.on_key_step(move |d, e| on_step((d, e)));
    dlg.show().unwrap();
    // The commit list has the focus when the dialog opens.
    press(dlg.window(), " ");
    press(dlg.window(), "p");
    press(dlg.window(), "s");
    press(dlg.window(), "q");
    press(dlg.window(), "e");
    assert_eq!(*actions.borrow(), [4, 0, 3, 1, 2]);
    press(dlg.window(), "u");
    press(dlg.window(), "d");
    shift(dlg.window(), "U");
    shift(dlg.window(), "D");
    assert_eq!(
        *moves.borrow(),
        [(-1, false), (1, false), (-1, true), (1, true)]
    );
    ctrl(dlg.window(), "a");
    press(dlg.window(), K::DownArrow);
    assert_eq!(*steps.borrow(), [(0, false), (1, false)]);
    // Once rebasing has started the actions are fixed.
    dlg.set_editable(false);
    press(dlg.window(), "p");
    press(dlg.window(), "u");
    assert_eq!(actions.borrow().len(), 5);
    assert_eq!(moves.borrow().len(), 4);
}

#[test]
fn rebase_access_keys() {
    init();
    let dlg = RebaseDialog::new().unwrap();
    enable_access_keys!(dlg);
    let (moves, on_move) = recorder::<(i32, bool)>();
    dlg.on_move_rows(move |d, e| on_move((d, e)));
    dlg.show().unwrap();
    alt(dlg.window(), "d");
    assert_eq!(*moves.borrow(), [(1, false)]);
    alt(dlg.window(), "f");
    assert!(dlg.get_force());
    alt(dlg.window(), "o");
    assert!(dlg.get_onto_enabled());
    // "&Upstream:" and "&Up" share Alt+U: it moves the focus, never clicks Up.
    alt(dlg.window(), "u");
    alt(dlg.window(), "u");
    assert_eq!(moves.borrow().len(), 1);
}

#[test]
fn cherry_pick_access_keys() {
    init();
    let dlg = RebaseDialog::new().unwrap();
    enable_access_keys!(dlg);
    dlg.set_cherry_pick(true);
    let (moves, on_move) = recorder::<(i32, bool)>();
    dlg.on_move_rows(move |d, e| on_move((d, e)));
    dlg.show().unwrap();
    alt(dlg.window(), "a");
    assert!(dlg.get_add_cherry_picked_from());
    // Without the Upstream label, Alt+U clicks Up.
    alt(dlg.window(), "u");
    assert_eq!(*moves.borrow(), [(-1, false)]);
}

// -------------------------------------------------------------------- Sync

fn sync_dialog() -> SyncDialog {
    let dlg = SyncDialog::new().unwrap();
    dlg.set_pull_entries(strings(&["Pull", "Fetch", "Fetch & Rebase"]));
    dlg.set_pull_keys(strings(&["p", "h", "b"]));
    dlg.set_push_entries(strings(&["Push", "Push tags"]));
    dlg.set_push_keys(strings(&["h", "g"]));
    dlg
}

#[test]
fn sync_enter_and_escape_close_unless_running() {
    init();
    let dlg = sync_dialog();
    let (closed, on_close) = counter();
    dlg.on_close_dialog(on_close);
    dlg.show().unwrap();
    press(dlg.window(), K::Return);
    press(dlg.window(), K::Escape);
    assert_eq!(closed.get(), 2);
    dlg.set_running(true);
    press(dlg.window(), K::Return);
    press(dlg.window(), K::Escape);
    assert_eq!(closed.get(), 2);
}

#[test]
fn sync_f5_refreshes_unless_running() {
    init();
    let dlg = sync_dialog();
    let (refresh, on_refresh) = counter();
    dlg.on_refs_changed(on_refresh);
    dlg.show().unwrap();
    press(dlg.window(), K::F5);
    assert_eq!(refresh.get(), 1);
    dlg.set_running(true);
    press(dlg.window(), K::F5);
    assert_eq!(refresh.get(), 1);
}

#[test]
fn sync_enter_is_swallowed_in_remote_combos() {
    init();
    let dlg = sync_dialog();
    enable_access_keys!(dlg);
    let (closed, on_close) = counter();
    dlg.on_close_dialog(on_close);
    dlg.show().unwrap();
    alt(dlg.window(), "r");
    type_text(dlg.window(), "main");
    assert_eq!(dlg.get_remote_branch(), "main");
    press(dlg.window(), K::Return);
    alt(dlg.window(), "u");
    press(dlg.window(), K::Return);
    assert_eq!(closed.get(), 0);
}

#[test]
fn sync_access_keys_follow_the_menu_buttons() {
    init();
    let dlg = sync_dialog();
    enable_access_keys!(dlg);
    let (pulls, on_pull) = counter();
    let (pushes, on_push) = counter();
    let (commits, on_commit) = counter();
    dlg.on_pull(on_pull);
    dlg.on_push(on_push);
    dlg.on_commit(on_commit);
    dlg.show().unwrap();
    alt(dlg.window(), "p");
    assert_eq!(pulls.get(), 1);
    alt(dlg.window(), "h");
    assert_eq!(pushes.get(), 1);
    // Showing "Fetc&h" next to "Pus&h": Alt+H only moves the focus.
    dlg.set_pull_choice(1);
    alt(dlg.window(), "h");
    alt(dlg.window(), "p");
    assert_eq!((pulls.get(), pushes.get()), (1, 1));
    dlg.set_pull_choice(2);
    alt(dlg.window(), "b");
    assert_eq!(pulls.get(), 2);
    alt(dlg.window(), "c");
    assert_eq!(commits.get(), 1);
    alt(dlg.window(), "f");
    assert!(dlg.get_force());
}

// -------------------------------------------------------------------- Push

#[test]
fn push_enter_escape_and_f5() {
    init();
    let dlg = PushDialog::new().unwrap();
    let (ok, on_ok) = counter();
    let (cancel, on_cancel) = counter();
    let (refresh, on_refresh) = counter();
    dlg.on_ok(on_ok);
    dlg.on_cancel(on_cancel);
    dlg.on_refresh(on_refresh);
    dlg.show().unwrap();
    press(dlg.window(), K::Return);
    assert_eq!(ok.get(), 1);
    press(dlg.window(), K::Escape);
    assert_eq!(cancel.get(), 1);
    press(dlg.window(), K::F5);
    assert_eq!(refresh.get(), 1);
}

#[test]
fn push_access_keys_and_enter_in_combos() {
    init();
    let dlg = PushDialog::new().unwrap();
    enable_access_keys!(dlg);
    let (ok, on_ok) = counter();
    let (manage, on_manage) = counter();
    dlg.on_ok(on_ok);
    dlg.on_manage(on_manage);
    dlg.show().unwrap();
    alt(dlg.window(), "u");
    assert!(dlg.get_use_url());
    alt(dlg.window(), "m");
    assert!(!dlg.get_use_url());
    alt(dlg.window(), "t");
    assert!(dlg.get_tags());
    alt(dlg.window(), "g");
    assert_eq!(manage.get(), 1);
    // A history combo passes Enter on to the default button.
    alt(dlg.window(), "l");
    type_text(dlg.window(), "topic");
    assert_eq!(dlg.get_local(), "topic");
    press(dlg.window(), K::Return);
    assert_eq!(ok.get(), 1);
    alt(dlg.window(), "p");
    assert!(dlg.get_all_branches());
}

// -------------------------------------------------------------- Pull/Fetch

#[test]
fn pullfetch_keys() {
    init();
    let dlg = PullFetchDialog::new().unwrap();
    enable_access_keys!(dlg);
    let (ok, on_ok) = counter();
    let (cancel, on_cancel) = counter();
    let (remote, on_remote) = counter();
    dlg.on_ok(on_ok);
    dlg.on_cancel(on_cancel);
    dlg.on_remote_changed(on_remote);
    dlg.show().unwrap();
    press(dlg.window(), K::Return);
    press(dlg.window(), K::Escape);
    assert_eq!((ok.get(), cancel.get()), (1, 1));
    alt(dlg.window(), "u");
    assert!(dlg.get_use_url());
    alt(dlg.window(), "r");
    assert!(!dlg.get_use_url());
    assert_eq!(remote.get(), 2);
    alt(dlg.window(), "s");
    alt(dlg.window(), "m");
    alt(dlg.window(), "f");
    assert!(dlg.get_squash() && dlg.get_no_commit() && dlg.get_no_ff());
    // Fast Forward Only is disabled while No Fast Forward is checked.
    alt(dlg.window(), "n");
    assert!(!dlg.get_ff_only());
    alt(dlg.window(), "b");
    type_text(dlg.window(), "dev");
    press(dlg.window(), K::Return);
    assert_eq!(dlg.get_remote_branch(), "dev");
    assert_eq!(ok.get(), 2);
}

#[test]
fn pullfetch_tri_state_checks_are_tab_stops() {
    init();
    let dlg = PullFetchDialog::new().unwrap();
    dlg.set_tags(2);
    dlg.show().unwrap();
    for _ in 0..30 {
        press(dlg.window(), K::Tab);
        press(dlg.window(), " ");
        if dlg.get_tags() != 2 {
            // Indeterminate cycles to unchecked.
            assert_eq!(dlg.get_tags(), 0);
            return;
        }
    }
    panic!("no tab stop toggled the Tags tri-state with Space");
}

// ------------------------------------------------------------------- Clone

#[test]
fn clone_keys() {
    init();
    let dlg = CloneDialog::new().unwrap();
    enable_access_keys!(dlg);
    let (ok, on_ok) = counter();
    let (cancel, on_cancel) = counter();
    let (browse, on_browse) = counter();
    dlg.on_ok(on_ok);
    dlg.on_cancel(on_cancel);
    dlg.on_browse_directory(on_browse);
    dlg.show().unwrap();
    // The URL combo has the focus; Enter in it presses OK.
    type_text(dlg.window(), "u");
    assert_eq!(dlg.get_url(), "u");
    press(dlg.window(), K::Return);
    assert_eq!(ok.get(), 1);
    press(dlg.window(), K::Escape);
    assert_eq!(cancel.get(), 1);
    alt(dlg.window(), "w");
    assert_eq!(browse.get(), 1);
    alt(dlg.window(), "d");
    type_text(dlg.window(), "x");
    assert_eq!(dlg.get_directory(), "x");
}

// ---------------------------------------------------------------- Progress

#[test]
fn progress_escape_waits_for_git_then_closes() {
    init();
    let dlg = ProgressDialog::new().unwrap();
    enable_access_keys!(dlg);
    let (closed, on_close) = counter();
    let (posts, on_post) = recorder::<i32>();
    dlg.on_close_dialog(on_close);
    dlg.on_post_action(on_post);
    dlg.set_post_actions(strings(&["Pull...", "Push..."]));
    dlg.set_post_keys(strings(&["p", "h"]));
    dlg.show().unwrap();
    press(dlg.window(), K::Escape);
    press(dlg.window(), K::Return);
    alt(dlg.window(), "h");
    assert_eq!(closed.get(), 0);
    assert!(posts.borrow().is_empty());
    dlg.set_running(false);
    alt(dlg.window(), "h");
    assert_eq!(*posts.borrow(), [1]);
    press(dlg.window(), K::Escape);
    press(dlg.window(), K::Return);
    assert_eq!(closed.get(), 2);
}

// -------------------------------------------------------------- MessageBox

fn message_box(buttons: &[&str]) -> (MessageBox, Rc<Cell<i32>>) {
    let dlg = MessageBox::new().unwrap();
    msgbox::configure(&dlg, "KoopaGit", "Do it?", buttons);
    let clicked = Rc::new(Cell::new(-1));
    let c = clicked.clone();
    dlg.on_clicked(move |i| c.set(i));
    (dlg, clicked)
}

#[test]
fn message_box_enter_escape_and_access_keys() {
    init();
    let (dlg, clicked) = message_box(&["Yes", "No"]);
    enable_access_keys!(dlg);
    assert_eq!(dlg.get_buttons().row_data(0).unwrap(), "Yes");
    dlg.show().unwrap();
    press(dlg.window(), K::Escape);
    assert_eq!(clicked.get(), 1);
    press(dlg.window(), K::Return);
    assert_eq!(clicked.get(), 0);
    alt(dlg.window(), "n");
    assert_eq!(clicked.get(), 1);
    alt(dlg.window(), "y");
    assert_eq!(clicked.get(), 0);

    let (dlg, clicked) = message_box(&["Merge", "Rebase", "Abort"]);
    dlg.show().unwrap();
    press(dlg.window(), K::Escape);
    assert_eq!(clicked.get(), 2);

    // No negative button: Escape does nothing.
    let (dlg, clicked) = message_box(&["Commit", "Push"]);
    dlg.show().unwrap();
    press(dlg.window(), K::Escape);
    assert_eq!(clicked.get(), -1);
}

#[test]
fn message_box_copies_its_text() {
    init();
    let (dlg, _) = message_box(&["OK"]);
    let (copied, on_copy) = counter();
    dlg.on_copy_text(on_copy);
    dlg.show().unwrap();
    ctrl(dlg.window(), "c");
    ctrl(dlg.window(), K::Insert);
    assert_eq!(copied.get(), 2);
}
