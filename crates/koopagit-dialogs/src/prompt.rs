//! Non-blocking text prompt for use from inside another dialog's event loop
//! (Edit Notes, Edit description, ...). Slint cannot nest modal loops.

use slint::ComponentHandle;

use crate::{InputDialog, keep_alive};

/// Shows an input box. `on_ok` receives the text and returns an error message
/// to keep the box open, or `None` to close it.
pub fn text(
    title: &str,
    label: &str,
    value: &str,
    on_ok: impl Fn(&str) -> Option<String> + 'static,
) {
    let Ok(dlg) = InputDialog::new() else {
        return;
    };
    dlg.set_dialog_title(title.into());
    dlg.set_label(label.into());
    dlg.set_value(value.into());
    {
        let weak = dlg.as_weak();
        dlg.on_ok(move || {
            let Some(d) = weak.upgrade() else { return };
            match on_ok(d.get_value().as_str()) {
                Some(err) => d.set_notice_text(err.into()),
                None => {
                    let _ = d.hide();
                }
            }
        });
    }
    {
        let weak = dlg.as_weak();
        dlg.on_cancel(move || {
            if let Some(d) = weak.upgrade() {
                let _ = d.hide();
            }
        });
    }
    if dlg.show().is_ok() {
        keep_alive(dlg);
    }
}
