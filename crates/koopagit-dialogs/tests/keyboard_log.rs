//! Keyboard parity for the Log dialog and its companions (Statistics, Notes),
//! RefLog, Browse References, Delete remote tags, Repository Browser,
//! Revision Graph and the worktree dialogs (docs/spec/spec-keyboard.md §1.2,
//! §3.1, §3.6-§3.8, §3.10, §6).

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::*;
use koopagit_dialogs::*;
use slint::{ComponentHandle, ModelRc, VecModel};

/// Records the arguments of each call.
fn recorder<T: 'static>() -> (Rc<RefCell<Vec<T>>>, impl Fn(T) + 'static) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let l = log.clone();
    (log, move |v| l.borrow_mut().push(v))
}

fn log_dialog() -> LogDialog {
    let dlg = LogDialog::new().unwrap();
    dlg.set_rows(ModelRc::new(VecModel::from(vec![LogRow::default(); 3])));
    dlg.set_current_row(1);
    dlg
}

#[test]
fn log_enter_never_closes_and_escape_clears_the_filter_first() {
    init();
    let dlg = log_dialog();
    let (ok, on_ok) = counter();
    let (cancel, on_cancel) = counter();
    let (filtered, on_filter) = counter();
    dlg.on_ok(on_ok);
    dlg.on_cancel(on_cancel);
    dlg.on_filter_changed(on_filter);
    dlg.show().unwrap();
    press(dlg.window(), K::Return);
    assert_eq!(ok.get(), 0);
    dlg.set_filter_text("fix".into());
    press(dlg.window(), K::Escape);
    assert_eq!(dlg.get_filter_text(), "");
    assert_eq!((filtered.get(), cancel.get()), (1, 0));
    press(dlg.window(), K::Escape);
    assert_eq!(cancel.get(), 1);
    assert_eq!(ok.get(), 0);
}

#[test]
fn log_list_keys() {
    init();
    let dlg = log_dialog();
    let (moves, on_move) = recorder();
    let (all, on_all) = counter();
    let (activated, on_activated) = recorder();
    dlg.on_key_move(move |d, extend| on_move((d, extend)));
    dlg.on_select_all(on_all);
    dlg.on_row_activated(on_activated);
    dlg.show().unwrap();
    press(dlg.window(), K::DownArrow);
    shift(dlg.window(), K::UpArrow);
    press(dlg.window(), K::End);
    press(dlg.window(), K::Home);
    assert_eq!(
        *moves.borrow(),
        [(1, false), (-1, true), (3, false), (-3, false)]
    );
    ctrl(dlg.window(), "a");
    assert_eq!(all.get(), 1);
    // Enter diffs (Rust checks DiffByDoubleClickInLog) and does not close.
    press(dlg.window(), K::Return);
    assert_eq!(*activated.borrow(), [1]);
}

#[test]
fn log_accelerators() {
    init();
    let dlg = log_dialog();
    let (refreshed, on_refresh) = counter();
    let (copied, on_copy) = counter();
    let (pasted, on_paste) = counter();
    let (jumps, on_jump) = recorder();
    dlg.on_refresh(on_refresh);
    dlg.on_copy_selection(on_copy);
    dlg.on_paste_hash(on_paste);
    dlg.on_jump(on_jump);
    dlg.show().unwrap();
    let w = dlg.window();
    press(w, K::F5);
    ctrl(w, K::F5);
    shift(w, K::F5);
    assert_eq!(refreshed.get(), 3);
    ctrl(w, "c");
    ctrl(w, K::Insert);
    assert_eq!(copied.get(), 2);
    ctrl(w, "v");
    shift(w, K::Insert);
    assert_eq!(pasted.get(), 2);
    // Alt+Up/Down without access keys (macOS) go through the bubble-phase bindings.
    dlg.global::<KeyboardPrefs>().set_access_keys(false);
    alt(w, K::UpArrow);
    alt(w, K::DownArrow);
    enable_access_keys!(dlg);
    alt(w, K::UpArrow);
    alt(w, K::DownArrow);
    assert_eq!(*jumps.borrow(), [true, false, true, false]);
}

#[test]
fn log_filter_focus_keys() {
    init();
    let dlg = log_dialog();
    enable_access_keys!(dlg);
    dlg.show().unwrap();
    let w = dlg.window();
    ctrl(w, "l");
    type_text(w, "ab");
    assert_eq!(dlg.get_filter_text(), "ab");
    // Ctrl+E selects the text, so typing replaces it.
    ctrl(w, "e");
    type_text(w, "x");
    assert_eq!(dlg.get_filter_text(), "x");
    // Tab away from the filter; Alt+D, Ctrl+F (no Find dialog) and F3 each return to it.
    let keys: [fn(&slint::Window); 3] = [|w| alt(w, "d"), |w| ctrl(w, "f"), |w| press(w, K::F3)];
    for (i, key) in keys.into_iter().enumerate() {
        dlg.set_filter_text("".into());
        press(w, K::Tab);
        type_text(w, "z");
        assert_eq!(dlg.get_filter_text(), "", "key #{i}");
        key(w);
        type_text(w, "q");
        assert_eq!(dlg.get_filter_text(), "q", "key #{i}");
    }
}

#[test]
fn log_access_keys() {
    init();
    let dlg = log_dialog();
    enable_access_keys!(dlg);
    let (refreshed, on_refresh) = counter();
    let (stats, on_stats) = counter();
    dlg.on_refresh(on_refresh);
    dlg.on_statistics(on_stats);
    dlg.show().unwrap();
    alt(dlg.window(), "w");
    assert!(dlg.get_whole_project());
    alt(dlg.window(), "a");
    assert!(dlg.get_all_branches());
    assert_eq!(refreshed.get(), 2);
    alt(dlg.window(), "t");
    assert_eq!(stats.get(), 1);
    // "Show &Whole Project" is disabled when the log is for the whole project anyway.
    dlg.set_whole_project_enabled(false);
    alt(dlg.window(), "w");
    assert!(dlg.get_whole_project());
}

#[test]
fn statistics_and_notes() {
    init();
    let stats = StatisticsDialog::new().unwrap();
    let (closed, on_close) = counter();
    stats.on_dismiss(on_close);
    stats.show().unwrap();
    press(stats.window(), K::Return);
    press(stats.window(), K::Escape);
    assert_eq!(closed.get(), 2);

    let notes = NotesDialog::new().unwrap();
    let (ok, on_ok) = counter();
    let (cancel, on_cancel) = counter();
    notes.on_ok(on_ok);
    notes.on_cancel(on_cancel);
    notes.show().unwrap();
    // The editor has focus: Enter is a newline, Ctrl+Enter is OK.
    type_text(notes.window(), "a");
    press(notes.window(), K::Return);
    assert_eq!(ok.get(), 0);
    assert!(notes.get_text().contains('\n'));
    ctrl(notes.window(), K::Return);
    assert_eq!(ok.get(), 1);
    press(notes.window(), K::Escape);
    assert_eq!(cancel.get(), 1);
}

#[test]
fn reflog_keys() {
    init();
    let dlg = RefLogDialog::new().unwrap();
    enable_access_keys!(dlg);
    dlg.set_rows(ModelRc::new(VecModel::from(vec![ReflogRow::default(); 2])));
    let (ok, on_ok) = counter();
    let (refreshed, on_refresh) = counter();
    let (cleared, on_clear) = counter();
    let (steps, on_step) = recorder();
    dlg.on_ok(on_ok);
    dlg.on_refresh(on_refresh);
    dlg.on_clear_stash(on_clear);
    dlg.on_key_step(move |d, e| on_step((d, e)));
    dlg.show().unwrap();
    let w = dlg.window();
    press(w, K::F5);
    assert_eq!(refreshed.get(), 1);
    ctrl(w, "a");
    press(w, K::UpArrow);
    shift(w, K::DownArrow);
    assert_eq!(*steps.borrow(), [(0, false), (-1, false), (1, true)]);
    // "&Clear stash" only exists for the stash reflog.
    alt(w, "c");
    assert_eq!(cleared.get(), 0);
    dlg.set_is_stash(true);
    alt(w, "c");
    assert_eq!(cleared.get(), 1);
    press(w, K::Return);
    press(w, K::Escape);
    assert_eq!(ok.get(), 2);
}

#[test]
fn browse_refs_keys() {
    init();
    let dlg = BrowseRefsDialog::new().unwrap();
    let (ok, on_ok) = counter();
    let (refreshed, on_refresh) = counter();
    let (renamed, on_rename) = counter();
    let (filtered, on_filter) = counter();
    let (input_cancelled, on_input_cancel) = counter();
    let (steps, on_step) = recorder();
    dlg.on_ok(on_ok);
    dlg.on_refresh(on_refresh);
    dlg.on_rename_selected(on_rename);
    dlg.on_filter_changed(on_filter);
    dlg.on_input_cancelled(on_input_cancel);
    dlg.on_key_step(move |d, e| on_step((d, e)));
    dlg.show().unwrap();
    let w = dlg.window();
    press(w, K::F5);
    assert_eq!(refreshed.get(), 1);
    press(w, K::F2);
    assert_eq!(renamed.get(), 1);
    ctrl(w, "a");
    press(w, K::DownArrow);
    assert_eq!(*steps.borrow(), [(0, false), (1, false)]);
    // Ctrl+E: filter focused with its text selected.
    dlg.set_filter_text("old".into());
    ctrl(w, "e");
    type_text(w, "ma");
    assert_eq!(dlg.get_filter_text(), "ma");
    // F2 only renames from the lists.
    press(w, K::F2);
    assert_eq!(renamed.get(), 1);
    // Escape in the filter clears it first, then closes.
    let before = filtered.get();
    press(w, K::Escape);
    assert_eq!(dlg.get_filter_text(), "");
    assert_eq!((filtered.get(), ok.get()), (before + 1, 0));
    press(w, K::Escape);
    assert_eq!(ok.get(), 1);
    // An open rename prompt takes Escape.
    dlg.set_input_label("Rename".into());
    press(w, K::Escape);
    assert_eq!((input_cancelled.get(), ok.get()), (1, 1));
}

#[test]
fn delete_remote_tag_keys() {
    init();
    let dlg = DeleteRemoteTagDialog::new().unwrap();
    enable_access_keys!(dlg);
    let (ok, on_ok) = counter();
    let (cancel, on_cancel) = counter();
    let (refreshed, on_refresh) = counter();
    let (all, on_all) = recorder();
    dlg.on_ok(on_ok);
    dlg.on_cancel(on_cancel);
    dlg.on_refresh(on_refresh);
    dlg.on_select_all(on_all);
    dlg.show().unwrap();
    let w = dlg.window();
    press(w, K::F5);
    assert_eq!(refreshed.get(), 1);
    alt(w, "a");
    alt(w, "a");
    assert_eq!(*all.borrow(), [true, false]);
    alt(w, "d");
    assert_eq!(ok.get(), 1);
    press(w, K::Escape);
    assert_eq!(cancel.get(), 1);
}

#[test]
fn repo_browser_keys() {
    init();
    let dlg = RepoBrowserDialog::new().unwrap();
    let (ok, on_ok) = counter();
    let (cancel, on_cancel) = counter();
    let (refreshed, on_refresh) = counter();
    let (entered, on_enter) = counter();
    let (keys, on_key) = recorder();
    dlg.on_ok(on_ok);
    dlg.on_cancel(on_cancel);
    dlg.on_refresh(on_refresh);
    dlg.on_entry_enter(on_enter);
    dlg.on_entry_key(move |d, e| on_key((d, e)));
    dlg.show().unwrap();
    let w = dlg.window();
    press(w, K::F5);
    assert_eq!(refreshed.get(), 1);
    // Enter outside the file list is OK.
    press(w, K::Return);
    assert_eq!(ok.get(), 1);
    // Tab to the file list (Ctrl+A answers there).
    let mut found = false;
    for _ in 0..10 {
        press(w, K::Tab);
        ctrl(w, "a");
        if !keys.borrow().is_empty() {
            found = true;
            break;
        }
    }
    assert!(found, "no tab stop reached the file list");
    press(w, K::DownArrow);
    assert_eq!(*keys.borrow(), [(0, false), (1, false)]);
    // Enter in the list opens the entry and keeps the dialog open.
    press(w, K::Return);
    assert_eq!((entered.get(), ok.get()), (1, 1));
    press(w, K::Escape);
    assert_eq!(cancel.get(), 1);
}

#[test]
fn revision_graph_keys() {
    init();
    let dlg = RevisionGraphDialog::new().unwrap();
    dlg.set_graph_width(3000.0);
    dlg.set_graph_height(3000.0);
    let (exited, on_exit) = counter();
    let (reloaded, on_reload) = counter();
    dlg.on_exit(on_exit);
    dlg.on_reload(on_reload);
    dlg.show().unwrap();
    let w = dlg.window();
    for k in [
        K::DownArrow,
        K::RightArrow,
        K::PageDown,
        K::UpArrow,
        K::LeftArrow,
        K::PageUp,
    ] {
        press(w, k);
    }
    press(w, K::F5);
    assert_eq!(reloaded.get(), 1);
    // OnOK only applies the zoom: Enter does not close.
    press(w, K::Return);
    assert_eq!(exited.get(), 0);
    press(w, K::Escape);
    assert_eq!(exited.get(), 1);
}

#[test]
fn worktree_list_keys() {
    init();
    let dlg = WorktreeListDialog::new().unwrap();
    enable_access_keys!(dlg);
    let (ok, on_ok) = counter();
    let (refreshed, on_refresh) = counter();
    let (added, on_add) = counter();
    let (pruned, on_prune) = counter();
    dlg.on_ok(on_ok);
    dlg.on_refresh(on_refresh);
    dlg.on_add(on_add);
    dlg.on_prune(on_prune);
    dlg.show().unwrap();
    let w = dlg.window();
    press(w, K::F5);
    alt(w, "a");
    alt(w, "p");
    assert_eq!((refreshed.get(), added.get(), pruned.get()), (1, 1, 1));
    press(w, K::Return);
    press(w, K::Escape);
    assert_eq!(ok.get(), 2);
}

#[test]
fn new_worktree_keys() {
    init();
    let dlg = NewWorktreeDialog::new().unwrap();
    enable_access_keys!(dlg);
    let (ok, on_ok) = counter();
    let (cancel, on_cancel) = counter();
    let (browsed, on_browse) = counter();
    let (toggled, on_toggle) = counter();
    dlg.on_ok(on_ok);
    dlg.on_cancel(on_cancel);
    dlg.on_browse(on_browse);
    dlg.on_branch_toggled(on_toggle);
    dlg.show().unwrap();
    let w = dlg.window();
    press(w, K::Return);
    assert_eq!(ok.get(), 1);
    press(w, K::Escape);
    assert_eq!(cancel.get(), 1);
    alt(w, "n");
    assert!(dlg.get_new_branch());
    assert_eq!(toggled.get(), 1);
    alt(w, "f");
    assert!(dlg.get_force());
    alt(w, "d");
    assert!(dlg.get_detach());
    alt(w, "b");
    assert_eq!(dlg.get_mode(), 1);
    alt(w, "w");
    assert_eq!(browsed.get(), 1);
    // "&Location" focuses the directory edit; Enter there is OK.
    alt(w, "l");
    type_text(w, "wt");
    assert_eq!(dlg.get_dir(), "wt");
    press(w, K::Return);
    assert_eq!(ok.get(), 2);
}
