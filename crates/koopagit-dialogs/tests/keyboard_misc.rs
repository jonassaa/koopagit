//! Keyboard parity for the ref dialogs (Switch, Create Branch/Tag, Merge, Abort
//! Merge, Stash), Export, Format Patch, Apply Patches, Submodule Add, Request Pull,
//! the git-svn dialogs, Send Mail, References commit is on, the First Start Wizard
//! and Settings (docs/spec/spec-keyboard.md §3.6, §5, §6).

mod common;

use std::cell::Cell;
use std::rc::Rc;

use common::*;
use koopagit_dialogs::*;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

/// Records the last int a callback received (-1 = never called).
fn last_int() -> (Rc<Cell<i32>>, impl Fn(i32) + 'static) {
    let v = Rc::new(Cell::new(-1));
    let c = v.clone();
    (v, move |i| c.set(i))
}

fn strings(items: &[&str]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        items
            .iter()
            .map(|s| SharedString::from(*s))
            .collect::<Vec<_>>(),
    ))
}

// ---------------------------------------------------------------- Switch/Checkout

#[test]
fn switch_enter_escape_and_access_keys() {
    init();
    let dlg = SwitchDialog::new().unwrap();
    enable_access_keys!(dlg);
    let (ok, on_ok) = counter();
    let (cancel, on_cancel) = counter();
    dlg.on_ok(on_ok);
    dlg.on_cancel(on_cancel);
    dlg.set_track_enabled(true);
    dlg.show().unwrap();

    press(dlg.window(), K::Return);
    assert_eq!(ok.get(), 1);
    press(dlg.window(), K::Escape);
    assert_eq!(cancel.get(), 1);

    alt(dlg.window(), "c");
    assert_eq!(dlg.get_mode(), 3);
    alt(dlg.window(), "t");
    assert_eq!(dlg.get_mode(), 2);
    alt(dlg.window(), "f");
    assert!(dlg.get_force());
    alt(dlg.window(), "m");
    assert!(dlg.get_merge());

    // T&rack and &Override need Create &New Branch.
    dlg.set_track(0);
    alt(dlg.window(), "r");
    assert_eq!(dlg.get_track(), 0);
    alt(dlg.window(), "n");
    assert!(dlg.get_create_branch());
    alt(dlg.window(), "r");
    assert_eq!(dlg.get_track(), 2);
    alt(dlg.window(), "o");
    assert!(dlg.get_override_branch());
    // Unchecking Create New Branch clears the dependent options.
    alt(dlg.window(), "n");
    assert!(!dlg.get_create_branch());
    assert!(!dlg.get_override_branch());
    assert_eq!(dlg.get_track(), 0);
}

#[test]
fn switch_tri_state_track_is_a_space_tab_stop() {
    init();
    let dlg = SwitchDialog::new().unwrap();
    dlg.set_create_branch(true);
    dlg.set_track_enabled(true);
    dlg.set_track(1);
    dlg.show().unwrap();
    for _ in 0..20 {
        press(dlg.window(), K::Tab);
        press(dlg.window(), " ");
        if dlg.get_track() != 1 {
            assert_eq!(dlg.get_track(), 0);
            return;
        }
    }
    panic!("no tab stop cycled the Track check box with Space");
}

// ---------------------------------------------------------------- Create Branch/Tag

#[test]
fn branch_name_edit_has_focus_and_enter_is_ok() {
    init();
    let dlg = BranchTagDialog::new().unwrap();
    let (ok, on_ok) = counter();
    let (cancel, on_cancel) = counter();
    dlg.on_ok(on_ok);
    dlg.on_cancel(on_cancel);
    dlg.show().unwrap();
    type_text(dlg.window(), "topic");
    assert_eq!(dlg.get_name(), "topic");
    press(dlg.window(), K::Return);
    assert_eq!(ok.get(), 1);
    press(dlg.window(), K::Escape);
    assert_eq!(cancel.get(), 1);
}

#[test]
fn branch_access_keys() {
    init();
    let dlg = BranchTagDialog::new().unwrap();
    enable_access_keys!(dlg);
    dlg.set_mode(1);
    dlg.set_track_enabled(true);
    dlg.set_track(0);
    dlg.show().unwrap();
    alt(dlg.window(), "h");
    assert_eq!(dlg.get_mode(), 0);
    alt(dlg.window(), "c");
    assert_eq!(dlg.get_mode(), 3);
    alt(dlg.window(), "f");
    assert!(dlg.get_force());
    alt(dlg.window(), "s");
    assert!(dlg.get_switch_to());
    alt(dlg.window(), "k");
    assert_eq!(dlg.get_track(), 2);
    // Tag-only keys do nothing for a branch.
    alt(dlg.window(), "p");
    assert!(!dlg.get_push());
    // &Name focuses the name edit.
    alt(dlg.window(), "n");
    type_text(dlg.window(), "x");
    assert_eq!(dlg.get_name(), "x");
}

#[test]
fn tag_access_keys_and_message_keeps_enter() {
    init();
    let dlg = BranchTagDialog::new().unwrap();
    enable_access_keys!(dlg);
    let (ok, on_ok) = counter();
    dlg.on_ok(on_ok);
    dlg.set_is_tag(true);
    dlg.set_sign_enabled(true);
    dlg.show().unwrap();
    alt(dlg.window(), "g");
    assert!(dlg.get_sign());
    alt(dlg.window(), "p");
    assert!(dlg.get_push());
    // Branch-only keys do nothing for a tag.
    alt(dlg.window(), "s");
    assert!(!dlg.get_switch_to());
    // &Message focuses the multi-line editor, where Enter is a newline.
    alt(dlg.window(), "m");
    type_text(dlg.window(), "a");
    press(dlg.window(), K::Return);
    type_text(dlg.window(), "b");
    assert_eq!(dlg.get_message(), "a\nb");
    assert_eq!(ok.get(), 0);
}

// ---------------------------------------------------------------- Merge

#[test]
fn merge_enter_escape_and_access_keys() {
    init();
    let dlg = MergeDialog::new().unwrap();
    enable_access_keys!(dlg);
    let (ok, on_ok) = counter();
    let (cancel, on_cancel) = counter();
    dlg.on_ok(on_ok);
    dlg.on_cancel(on_cancel);
    dlg.show().unwrap();
    press(dlg.window(), K::Return);
    assert_eq!(ok.get(), 1);
    press(dlg.window(), K::Escape);
    assert_eq!(cancel.get(), 1);

    alt(dlg.window(), "t");
    assert_eq!(dlg.get_mode(), 2);
    alt(dlg.window(), "f");
    assert!(dlg.get_no_ff());
    // Fast Forward O&nly is disabled while No Fast Forward is checked.
    alt(dlg.window(), "n");
    assert!(!dlg.get_ff_only());
    // &Squash clears No Fast Forward.
    alt(dlg.window(), "s");
    assert!(dlg.get_squash());
    assert!(!dlg.get_no_ff());
    alt(dlg.window(), "s");
    alt(dlg.window(), "n");
    assert!(dlg.get_ff_only());
}

#[test]
fn merge_shared_m_cycles_focus_without_toggling() {
    init();
    let dlg = MergeDialog::new().unwrap();
    enable_access_keys!(dlg);
    dlg.set_message("".into());
    dlg.show().unwrap();
    // No Co&mmit, then Merge &Message.
    alt(dlg.window(), "m");
    assert!(!dlg.get_no_commit());
    alt(dlg.window(), "m");
    type_text(dlg.window(), "msg");
    assert_eq!(dlg.get_message(), "msg");
    assert!(!dlg.get_no_commit());
}

// ---------------------------------------------------------------- Abort Merge

#[test]
fn abort_merge_keys() {
    init();
    let dlg = AbortMergeDialog::new().unwrap();
    enable_access_keys!(dlg);
    let (ok, on_ok) = counter();
    let (cancel, on_cancel) = counter();
    let (shown, on_show) = counter();
    dlg.on_ok(on_ok);
    dlg.on_cancel(on_cancel);
    dlg.on_show_modified(on_show);
    dlg.show().unwrap();
    alt(dlg.window(), "h");
    assert_eq!(dlg.get_reset_type(), 2);
    alt(dlg.window(), "m");
    assert_eq!(dlg.get_reset_type(), 1);
    alt(dlg.window(), "e");
    assert_eq!(dlg.get_reset_type(), 0);
    alt(dlg.window(), "w");
    assert_eq!(shown.get(), 1);
    press(dlg.window(), K::Return);
    assert_eq!(ok.get(), 1);
    press(dlg.window(), K::Escape);
    assert_eq!(cancel.get(), 1);
}

// ---------------------------------------------------------------- Stash

#[test]
fn stash_message_has_focus_and_access_keys() {
    init();
    let dlg = StashDialog::new().unwrap();
    enable_access_keys!(dlg);
    let (ok, on_ok) = counter();
    let (cancel, on_cancel) = counter();
    dlg.on_ok(on_ok);
    dlg.on_cancel(on_cancel);
    dlg.show().unwrap();
    type_text(dlg.window(), "wip");
    assert_eq!(dlg.get_message(), "wip");
    press(dlg.window(), K::Return);
    assert_eq!(ok.get(), 1);

    alt(dlg.window(), "u");
    assert!(dlg.get_include_untracked());
    // --&all is disabled while include untracked is checked.
    alt(dlg.window(), "a");
    assert!(!dlg.get_all());
    alt(dlg.window(), "u");
    alt(dlg.window(), "a");
    assert!(dlg.get_all());
    press(dlg.window(), K::Escape);
    assert_eq!(cancel.get(), 1);
}

// ---------------------------------------------------------------- Export / Format Patch

#[test]
fn export_enter_needs_a_file() {
    init();
    let dlg = ExportDialog::new().unwrap();
    enable_access_keys!(dlg);
    let (ok, on_ok) = counter();
    let (cancel, on_cancel) = counter();
    dlg.on_ok(on_ok);
    dlg.on_cancel(on_cancel);
    dlg.show().unwrap();
    press(dlg.window(), K::Return);
    assert_eq!(ok.get(), 0);
    // The zip file edit has the focus.
    type_text(dlg.window(), "out.zip");
    assert_eq!(dlg.get_file(), "out.zip");
    press(dlg.window(), K::Return);
    assert_eq!(ok.get(), 1);
    alt(dlg.window(), "w");
    assert!(dlg.get_whole_project());
    alt(dlg.window(), "b");
    assert_eq!(dlg.get_mode(), 1);
    alt(dlg.window(), "h");
    assert_eq!(dlg.get_mode(), 0);
    press(dlg.window(), K::Escape);
    assert_eq!(cancel.get(), 1);
}

#[test]
fn format_patch_enter_and_escape() {
    init();
    let dlg = FormatPatchDialog::new().unwrap();
    let (ok, on_ok) = counter();
    let (cancel, on_cancel) = counter();
    dlg.on_ok(on_ok);
    dlg.on_cancel(on_cancel);
    dlg.set_ok_enabled(false);
    dlg.show().unwrap();
    press(dlg.window(), K::Return);
    assert_eq!(ok.get(), 0);
    dlg.set_ok_enabled(true);
    press(dlg.window(), K::Return);
    assert_eq!(ok.get(), 1);
    press(dlg.window(), K::Escape);
    assert_eq!(cancel.get(), 1);
}

// ---------------------------------------------------------------- Apply Patches

fn patch_rows(n: usize) -> ModelRc<PatchRow> {
    ModelRc::new(VecModel::from(
        (0..n)
            .map(|i| PatchRow {
                checked: true,
                name: format!("000{i}.patch").into(),
                state: 0,
            })
            .collect::<Vec<_>>(),
    ))
}

#[test]
fn apply_patch_buttons_and_check_boxes() {
    init();
    let dlg = ApplyPatchDialog::new().unwrap();
    enable_access_keys!(dlg);
    let (apply, on_apply) = counter();
    let (cancel, on_cancel) = counter();
    let (add, on_add) = counter();
    let (remove, on_remove) = counter();
    let (down, on_down) = counter();
    dlg.on_apply(on_apply);
    dlg.on_cancel(on_cancel);
    dlg.on_add(on_add);
    dlg.on_remove(on_remove);
    dlg.on_down(on_down);
    dlg.show().unwrap();

    // A&pply needs patches.
    press(dlg.window(), K::Return);
    assert_eq!(apply.get(), 0);
    dlg.set_rows(patch_rows(2));
    press(dlg.window(), K::Return);
    assert_eq!(apply.get(), 1);
    alt(dlg.window(), "p");
    assert_eq!(apply.get(), 2);

    alt(dlg.window(), "a");
    assert_eq!(add.get(), 1);
    // &Remove / &Down need a selected patch.
    alt(dlg.window(), "r");
    assert_eq!(remove.get(), 0);
    dlg.set_current_row(0);
    alt(dlg.window(), "d");
    assert_eq!(down.get(), 1);
    alt(dlg.window(), "r");
    assert_eq!(remove.get(), 1);

    alt(dlg.window(), "3");
    assert!(!dlg.get_three_way());
    alt(dlg.window(), "i");
    assert!(!dlg.get_ignore_space());
    alt(dlg.window(), "s");
    assert!(dlg.get_sign_off());
    alt(dlg.window(), "k");
    assert!(!dlg.get_keep_cr());

    press(dlg.window(), K::Escape);
    assert_eq!(cancel.get(), 1);
    alt(dlg.window(), "c");
    assert_eq!(cancel.get(), 2);
    // Once finished, Cancel is disabled and Escape does nothing.
    dlg.set_finished(true);
    press(dlg.window(), K::Escape);
    assert_eq!(cancel.get(), 2);
    // While running, Cancel is "Abort" and the options are locked.
    dlg.set_running(true);
    press(dlg.window(), K::Escape);
    assert_eq!(cancel.get(), 3);
    alt(dlg.window(), "k");
    assert!(!dlg.get_keep_cr());
}

#[test]
fn apply_patch_list_is_a_tab_stop() {
    init();
    let dlg = ApplyPatchDialog::new().unwrap();
    let (selected, on_selected) = last_int();
    let toggled = Rc::new(Cell::new((-1, true)));
    {
        let t = toggled.clone();
        dlg.on_row_toggled(move |i, c| t.set((i, c)));
    }
    dlg.on_row_selected(on_selected);
    dlg.set_rows(patch_rows(3));
    dlg.show().unwrap();
    for _ in 0..20 {
        press(dlg.window(), K::Tab);
        press(dlg.window(), K::DownArrow);
        if selected.get() >= 0 {
            assert_eq!(selected.get(), 0);
            press(dlg.window(), K::DownArrow);
            assert_eq!(dlg.get_current_row(), 1);
            press(dlg.window(), K::End);
            assert_eq!(dlg.get_current_row(), 2);
            press(dlg.window(), " ");
            assert_eq!(toggled.get(), (2, false));
            return;
        }
    }
    panic!("no tab stop moved the patch selection with Down");
}

// ---------------------------------------------------------------- Submodule Add / Request Pull

#[test]
fn submodule_add_keys() {
    init();
    let dlg = SubmoduleAddDialog::new().unwrap();
    enable_access_keys!(dlg);
    let (ok, on_ok) = counter();
    let (cancel, on_cancel) = counter();
    dlg.on_ok(on_ok);
    dlg.on_cancel(on_cancel);
    dlg.show().unwrap();
    press(dlg.window(), K::Return);
    assert_eq!(ok.get(), 1);
    alt(dlg.window(), "f");
    assert!(dlg.get_force());
    // (A focused std CheckBox consumes Return itself, so Enter is tested first.)
    press(dlg.window(), K::Escape);
    assert_eq!(cancel.get(), 1);
}

#[test]
fn request_pull_keys() {
    init();
    let dlg = RequestPullDialog::new().unwrap();
    let (ok, on_ok) = counter();
    let (cancel, on_cancel) = counter();
    dlg.on_ok(on_ok);
    dlg.on_cancel(on_cancel);
    dlg.show().unwrap();
    press(dlg.window(), K::Return);
    assert_eq!(ok.get(), 1);
    press(dlg.window(), K::Escape);
    assert_eq!(cancel.get(), 1);
}

// ---------------------------------------------------------------- git-svn, Send Mail

#[test]
fn svn_dialog_keys() {
    init();
    let dlg = SvnDcommitDialog::new().unwrap();
    enable_access_keys!(dlg);
    let (ok, on_ok) = counter();
    let (cancel, on_cancel) = counter();
    dlg.on_ok(on_ok);
    dlg.on_cancel(on_cancel);
    dlg.show().unwrap();
    alt(dlg.window(), "g");
    assert!(dlg.get_rmdir());
    alt(dlg.window(), "s");
    assert!(!dlg.get_rmdir());
    press(dlg.window(), K::Return);
    assert_eq!(ok.get(), 1);
    press(dlg.window(), K::Escape);
    assert_eq!(cancel.get(), 1);

    let ign = SvnIgnoreDialog::new().unwrap();
    let (ok, on_ok) = counter();
    let (cancel, on_cancel) = counter();
    ign.on_ok(on_ok);
    ign.on_cancel(on_cancel);
    ign.show().unwrap();
    press(ign.window(), K::Return);
    assert_eq!(ok.get(), 1);
    press(ign.window(), K::Escape);
    assert_eq!(cancel.get(), 1);
}

#[test]
fn send_mail_enter_in_edits_sends() {
    init();
    let dlg = SendMailDialog::new().unwrap();
    let (send, on_send) = counter();
    let (cancel, on_cancel) = counter();
    dlg.on_send(on_send);
    dlg.on_cancel(on_cancel);
    dlg.show().unwrap();
    press(dlg.window(), K::Return);
    assert_eq!(send.get(), 1);
    // Tab reaches the To edit first.
    press(dlg.window(), K::Tab);
    type_text(dlg.window(), "a@b");
    assert_eq!(dlg.get_to(), "a@b");
    press(dlg.window(), K::Return);
    assert_eq!(send.get(), 2);
    press(dlg.window(), K::Escape);
    assert_eq!(cancel.get(), 1);
}

// ---------------------------------------------------------------- References commit is on

#[test]
fn commit_is_on_refs_keys() {
    init();
    let dlg = CommitIsOnRefsDialog::new().unwrap();
    let (entered, on_entered) = counter();
    let (refresh, on_refresh) = counter();
    let (copied, on_copy) = counter();
    let (cancel, on_cancel) = counter();
    let (filtered, on_filter) = counter();
    dlg.on_commit_entered(on_entered);
    dlg.on_refresh(on_refresh);
    dlg.on_copy_selected(on_copy);
    dlg.on_cancel(on_cancel);
    dlg.on_filter_changed(on_filter);
    dlg.set_refs(strings(&["refs/heads/main", "refs/heads/topic"]));
    dlg.show().unwrap();

    // The commit edit has the focus; Enter there reloads.
    press(dlg.window(), K::Return);
    assert_eq!(entered.get(), 1);
    press(dlg.window(), K::F5);
    assert_eq!(refresh.get(), 1);

    // Tab to the ref list: arrows select, Ctrl+C copies.
    let mut in_list = false;
    for _ in 0..5 {
        press(dlg.window(), K::Tab);
        press(dlg.window(), K::DownArrow);
        if dlg.get_current_row() >= 0 {
            in_list = true;
            break;
        }
    }
    assert!(in_list, "no tab stop moved the ref selection with Down");
    assert_eq!(dlg.get_current_row(), 0);
    press(dlg.window(), K::DownArrow);
    assert_eq!(dlg.get_current_row(), 1);
    ctrl(dlg.window(), "c");
    assert_eq!(copied.get(), 1);
    ctrl(dlg.window(), K::Insert);
    assert_eq!(copied.get(), 2);

    // The filter follows the list; Escape clears a non-empty filter first.
    press(dlg.window(), K::Tab);
    type_text(dlg.window(), "m");
    assert_eq!(dlg.get_filter(), "m");
    let before = filtered.get();
    press(dlg.window(), K::Escape);
    assert_eq!(dlg.get_filter(), "");
    assert_eq!(filtered.get(), before + 1);
    assert_eq!(cancel.get(), 0);
    press(dlg.window(), K::Escape);
    assert_eq!(cancel.get(), 1);
}

// ---------------------------------------------------------------- First Start Wizard

#[test]
fn first_start_wizard_keys() {
    init();
    let dlg = FirstStartDialog::new().unwrap();
    enable_access_keys!(dlg);
    let (next, on_next) = counter();
    let (back, on_back) = counter();
    let (finish, on_finish) = counter();
    let (cancel, on_cancel) = counter();
    dlg.on_next(on_next);
    dlg.on_back(on_back);
    dlg.on_finish(on_finish);
    dlg.on_cancel(on_cancel);
    dlg.show().unwrap();
    press(dlg.window(), K::Return);
    assert_eq!(next.get(), 1);
    // No < &Back on the first page.
    alt(dlg.window(), "b");
    assert_eq!(back.get(), 0);
    alt(dlg.window(), "n");
    assert_eq!(next.get(), 2);
    dlg.set_page(3);
    alt(dlg.window(), "d");
    assert!(dlg.get_dont_save_user());
    alt(dlg.window(), "b");
    assert_eq!(back.get(), 1);
    dlg.set_page(4);
    press(dlg.window(), K::Return);
    assert_eq!(finish.get(), 1);
    assert_eq!(next.get(), 2);
    press(dlg.window(), K::Escape);
    assert_eq!(cancel.get(), 1);
}

// ---------------------------------------------------------------- Settings

fn settings_dialog() -> SettingsDialog {
    let dlg = SettingsDialog::new().unwrap();
    let tree: Vec<TreeRow> = ["General", "Dialogs 1", "Git", "Remote"]
        .iter()
        .enumerate()
        .map(|(i, l)| TreeRow {
            label: (*l).into(),
            depth: (i == 3) as i32,
        })
        .collect();
    dlg.set_tree(ModelRc::new(VecModel::from(tree)));
    dlg
}

#[test]
fn settings_ok_cancel_apply() {
    init();
    let dlg = settings_dialog();
    enable_access_keys!(dlg);
    let (ok, on_ok) = counter();
    let (cancel, on_cancel) = counter();
    let (apply, on_apply) = counter();
    dlg.on_ok(on_ok);
    dlg.on_cancel(on_cancel);
    dlg.on_apply(on_apply);
    dlg.show().unwrap();
    press(dlg.window(), K::Return);
    assert_eq!(ok.get(), 1);
    press(dlg.window(), K::Escape);
    assert_eq!(cancel.get(), 1);
    // &Apply only while there are changes.
    alt(dlg.window(), "a");
    assert_eq!(apply.get(), 0);
    dlg.set_apply_enabled(true);
    alt(dlg.window(), "a");
    assert_eq!(apply.get(), 1);
}

#[test]
fn settings_page_navigation() {
    init();
    let dlg = settings_dialog();
    let (page, on_page) = last_int();
    dlg.on_page_selected(on_page);
    dlg.set_page(1);
    dlg.show().unwrap();
    ctrl(dlg.window(), K::Tab);
    assert_eq!(page.get(), 2);
    chord(dlg.window(), [K::Control, K::Shift], K::Tab);
    assert_eq!(page.get(), 0);
    ctrl(dlg.window(), K::PageDown);
    assert_eq!(page.get(), 2);
    ctrl(dlg.window(), K::PageUp);
    assert_eq!(page.get(), 0);
    // Cycling at both ends.
    dlg.set_page(3);
    ctrl(dlg.window(), K::Tab);
    assert_eq!(page.get(), 0);
    dlg.set_page(0);
    ctrl(dlg.window(), K::PageUp);
    assert_eq!(page.get(), 3);
    // The page tree has the focus: arrows move between pages.
    dlg.set_page(1);
    press(dlg.window(), K::DownArrow);
    assert_eq!(page.get(), 2);
    press(dlg.window(), K::Home);
    assert_eq!(page.get(), 0);
}

#[test]
fn settings_ctrl_tab_works_from_an_edit() {
    init();
    let dlg = settings_dialog();
    let (page, on_page) = last_int();
    let (ok, on_ok) = counter();
    let (edited, on_edited) = counter();
    dlg.on_page_selected(on_page);
    dlg.on_ok(on_ok);
    dlg.on_opt_text(move |_, _| on_edited());
    dlg.set_rows(ModelRc::new(VecModel::from(vec![OptRow {
        kind: 4,
        label: "Value".into(),
        enabled: true,
        ..Default::default()
    }])));
    dlg.show().unwrap();
    // Tab from the tree to the edit; Ctrl+Tab still switches pages from there,
    // and Enter in the edit is OK.
    for _ in 0..6 {
        press(dlg.window(), K::Tab);
        type_text(dlg.window(), "z");
        if edited.get() > 0 {
            break;
        }
    }
    assert!(edited.get() > 0, "Tab never reached the option edit");
    ctrl(dlg.window(), K::Tab);
    assert_eq!(page.get(), 1);
    press(dlg.window(), K::Return);
    assert_eq!(ok.get(), 1);
}
