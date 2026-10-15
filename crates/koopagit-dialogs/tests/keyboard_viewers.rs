//! Keyboard parity for the viewer windows: KoopaGitMerge (two-pane diff and
//! conflict editor), KoopaGitUDiff and KoopaGitBlame
//! (docs/spec/spec-keyboard.md §4).

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::*;
use koopagit_dialogs::*;
use slint::{ComponentHandle, ModelRc, VecModel};

fn sized(win: &slint::Window) {
    win.set_size(slint::LogicalSize::new(1100.0, 700.0));
}

#[test]
fn merge_difference_keys_match_upstream() {
    init();
    let dlg = DiffViewer::new().unwrap();
    let (next, on_next) = counter();
    let (prev, on_prev) = counter();
    dlg.on_next_difference(on_next);
    dlg.on_previous_difference(on_prev);
    dlg.show().unwrap();
    let w = dlg.window();
    press(w, K::F7);
    press(w, K::F11);
    ctrl(w, K::DownArrow);
    alt(w, K::DownArrow);
    assert_eq!((next.get(), prev.get()), (4, 0));
    shift(w, K::F7);
    shift(w, K::F11);
    ctrl(w, K::F11);
    ctrl(w, K::UpArrow);
    alt(w, K::UpArrow);
    assert_eq!((next.get(), prev.get()), (4, 5));
    // F8/Shift+F8 are next/previous *conflict*: nothing in a two-pane diff.
    press(w, K::F8);
    shift(w, K::F8);
    assert_eq!((next.get(), prev.get()), (4, 5));
}

#[test]
fn merge_exit_save_and_use_left_block() {
    init();
    let dlg = DiffViewer::new().unwrap();
    let (closed, on_close) = counter();
    let (saved, on_save) = counter();
    let (used, on_use) = counter();
    dlg.on_close_viewer(on_close);
    dlg.on_save(on_save);
    dlg.on_use_left_block(on_use);
    dlg.show().unwrap();
    let w = dlg.window();
    press(w, K::Escape);
    ctrl(w, "q");
    ctrl(w, "w");
    assert_eq!(closed.get(), 3);
    ctrl(w, "s");
    assert_eq!(saved.get(), 0, "nothing to save");
    dlg.set_can_save(true);
    ctrl(w, "s");
    assert_eq!(saved.get(), 1);
    // With unsaved edits the exit keys still ask to close; the controller
    // shows the save question (CheckForSave).
    press(w, K::Escape);
    assert_eq!(closed.get(), 4);
    // Ctrl+Shift+S is Save As, which KoopaGit does not have.
    chord(w, [K::Control, K::Shift], "S");
    assert_eq!(saved.get(), 1);
    press(w, K::F12);
    alt(w, K::RightArrow);
    assert_eq!(used.get(), 2);
    // Shift+F12 / Ctrl+F10 (use my block) need the three-pane view.
    shift(w, K::F12);
    ctrl(w, K::F10);
    assert_eq!(used.get(), 2);
}

#[test]
fn merge_scroll_keys_move_the_view() {
    init();
    let dlg = DiffViewer::new().unwrap();
    let rows: Vec<DiffRow> = (0..500).map(|_| DiffRow::default()).collect();
    dlg.set_rows(ModelRc::new(VecModel::from(rows)));
    sized(dlg.window());
    dlg.show().unwrap();
    let w = dlg.window();
    press(w, K::DownArrow);
    assert_eq!(dlg.get_scroll_y(), -18.0);
    press(w, K::UpArrow);
    press(w, K::UpArrow);
    assert_eq!(dlg.get_scroll_y(), 0.0, "clamped at the top");
    press(w, K::PageDown);
    let page = -dlg.get_scroll_y();
    assert!(page > 100.0, "page down scrolls a screenful, got {page}");
    ctrl(w, K::End);
    let end = dlg.get_scroll_y();
    assert!(end < -8000.0, "Ctrl+End reaches the end, got {end}");
    // After a jump this long, Slint's ListView re-anchors on a row boundary,
    // which can leave the view up to one row short of the bottom (fluent style).
    press(w, K::DownArrow);
    let bottom = dlg.get_scroll_y();
    assert!(
        (0.0..18.0).contains(&(end - bottom)),
        "within a row of the end: {end} -> {bottom}"
    );
    press(w, K::DownArrow);
    assert_eq!(dlg.get_scroll_y(), bottom, "clamped at the bottom");
    press(w, K::Home);
    assert_eq!(dlg.get_scroll_y(), 0.0);
}

/// Ctrl+End lands on the very bottom whatever the view height, not on the
/// row boundary Slint's ListView snaps long jumps to.
#[test]
fn merge_end_reaches_the_bottom_at_any_height() {
    init();
    let dlg = DiffViewer::new().unwrap();
    let rows: Vec<DiffRow> = (0..500).map(|_| DiffRow::default()).collect();
    dlg.set_rows(ModelRc::new(VecModel::from(rows)));
    dlg.show().unwrap();
    let w = dlg.window();
    for height in 700..718 {
        w.set_size(slint::LogicalSize::new(1100.0, height as f32));
        press(w, K::Home);
        ctrl(w, K::End);
        let bottom = dlg.get_scroll_y();
        press(w, K::DownArrow);
        assert_eq!(dlg.get_scroll_y(), bottom, "window height {height}");
    }
}

#[test]
fn conflict_editor_navigation_and_block_keys() {
    init();
    let dlg = ConflictViewer::new().unwrap();
    let (next, on_next) = counter();
    let (prev, on_prev) = counter();
    let chosen = Rc::new(RefCell::new(Vec::new()));
    dlg.on_next_conflict(on_next);
    dlg.on_previous_conflict(on_prev);
    {
        let chosen = chosen.clone();
        dlg.on_choose(move |b, which| chosen.borrow_mut().push((b, which)));
    }
    dlg.show().unwrap();
    let w = dlg.window();
    press(w, K::F8);
    press(w, K::F7);
    ctrl(w, K::DownArrow);
    assert_eq!((next.get(), prev.get()), (3, 0));
    shift(w, K::F8);
    shift(w, K::F7);
    alt(w, K::UpArrow);
    assert_eq!((next.get(), prev.get()), (3, 3));

    // No current block: the "use" keys do nothing.
    ctrl(w, K::F9);
    assert!(chosen.borrow().is_empty());
    dlg.set_current_block(1);
    ctrl(w, K::F9);
    shift(w, K::F12);
    ctrl(w, K::F10);
    chord(w, [K::Control, K::Shift], K::F9);
    chord(w, [K::Control, K::Shift], K::F10);
    assert_eq!(*chosen.borrow(), [(1, 2), (1, 1), (1, 1), (1, 4), (1, 3)]);
    // F12 (use left block) is disabled in the three-pane view upstream.
    press(w, K::F12);
    assert_eq!(chosen.borrow().len(), 5);
}

#[test]
fn conflict_editor_save_and_exit() {
    init();
    let dlg = ConflictViewer::new().unwrap();
    let (closed, on_close) = counter();
    let (saved, on_save) = counter();
    dlg.on_close_viewer(on_close);
    dlg.on_save(on_save);
    dlg.show().unwrap();
    let w = dlg.window();
    ctrl(w, "s");
    assert_eq!(saved.get(), 0, "unresolved conflicts");
    dlg.set_can_save(true);
    ctrl(w, "s");
    assert_eq!(saved.get(), 1);
    press(w, K::Escape);
    ctrl(w, "q");
    ctrl(w, "w");
    assert_eq!(closed.get(), 3);
}

#[test]
fn udiff_exits_on_ctrl_w_and_escape() {
    init();
    let dlg = UDiffViewer::new().unwrap();
    let (closed, on_close) = counter();
    dlg.on_close_viewer(on_close);
    let lines: Vec<UDiffLine> = (0..300).map(|_| UDiffLine::default()).collect();
    dlg.set_lines(ModelRc::new(VecModel::from(lines)));
    sized(dlg.window());
    dlg.show().unwrap();
    let w = dlg.window();
    // Ctrl+Q is not bound in TortoiseUDiff.
    ctrl(w, "q");
    assert_eq!(closed.get(), 0);
    ctrl(w, "w");
    press(w, K::Escape);
    assert_eq!(closed.get(), 2);
}

#[test]
fn blame_keys_move_copy_and_exit() {
    init();
    let dlg = BlameViewer::new().unwrap();
    let rows: Vec<BlameRow> = (0..200).map(|_| BlameRow::default()).collect();
    dlg.set_rows(ModelRc::new(VecModel::from(rows)));
    let selected = Rc::new(RefCell::new(Vec::new()));
    {
        let selected = selected.clone();
        dlg.on_selected(move |i| selected.borrow_mut().push(i));
    }
    let (copied, on_copy) = counter();
    let (closed, on_close) = counter();
    dlg.on_copy(on_copy);
    dlg.on_close_viewer(on_close);
    sized(dlg.window());
    dlg.show().unwrap();
    let w = dlg.window();
    press(w, K::DownArrow);
    press(w, K::DownArrow);
    assert_eq!(dlg.get_current(), 1);
    press(w, K::UpArrow);
    assert_eq!(dlg.get_current(), 0);
    press(w, K::PageDown);
    assert!(dlg.get_current() > 10);
    ctrl(w, K::End);
    assert_eq!(dlg.get_current(), 199);
    press(w, K::Home);
    assert_eq!(dlg.get_current(), 0);
    assert_eq!(selected.borrow().last(), Some(&0));

    ctrl(w, "c");
    ctrl(w, K::Insert);
    assert_eq!(copied.get(), 2);
    // Ctrl+W is not bound in TortoiseGitBlame.
    ctrl(w, "w");
    assert_eq!(closed.get(), 0);
    press(w, K::Escape);
    ctrl(w, "q");
    assert_eq!(closed.get(), 2);
}
