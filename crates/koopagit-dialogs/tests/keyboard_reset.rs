//! Keyboard parity for the Reset dialog and the shared ChooseVersion block
//! (docs/spec/spec-keyboard.md §5, §6 "Reset").

mod common;

use common::*;
use koopagit_dialogs::*;
use slint::ComponentHandle;

#[test]
fn enter_is_ok_and_escape_is_cancel() {
    init();
    let dlg = ResetDialog::new().unwrap();
    let (ok, on_ok) = counter();
    let (cancel, on_cancel) = counter();
    dlg.on_ok(on_ok);
    dlg.on_cancel(on_cancel);
    dlg.show().unwrap();
    press(dlg.window(), K::Return);
    assert_eq!(ok.get(), 1);
    press(dlg.window(), K::Escape);
    assert_eq!(cancel.get(), 1);
    // Ctrl+Enter is not the default button.
    ctrl(dlg.window(), K::Return);
    assert_eq!(ok.get(), 1);
}

#[test]
fn access_keys_select_reset_type_and_version() {
    init();
    let dlg = ResetDialog::new().unwrap();
    enable_access_keys!(dlg);
    let (shown, on_show) = counter();
    dlg.on_show_modified(on_show);
    dlg.show().unwrap();
    alt(dlg.window(), "h");
    assert_eq!(dlg.get_reset_type(), 2);
    alt(dlg.window(), "s");
    assert_eq!(dlg.get_reset_type(), 0);
    alt(dlg.window(), "b");
    assert_eq!(dlg.get_version_mode(), 1);
    alt(dlg.window(), "w");
    assert_eq!(shown.get(), 1);
    alt(dlg.window(), "c");
    assert_eq!(dlg.get_version_mode(), 3);
}

#[test]
fn tab_reaches_radios_and_space_selects() {
    init();
    let dlg = ResetDialog::new().unwrap();
    dlg.set_reset_type(1);
    dlg.show().unwrap();
    // Alt-free path: Tab through the controls until a radio selects Soft on Space.
    for _ in 0..12 {
        press(dlg.window(), K::Tab);
        press(dlg.window(), " ");
        if dlg.get_reset_type() == 0 {
            return;
        }
    }
    panic!("no tab stop selected the Soft radio with Space");
}
