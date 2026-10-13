//! Stand-alone message boxes (`CMessageBox`), shown before or after a dialog.

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::MessageBox;

/// Splits a Win32 caption into its display text and lower-case `&` access
/// letter (`&&` is a literal `&`).
pub fn split_mnemonic(caption: &str) -> (String, Option<char>) {
    let mut text = String::with_capacity(caption.len());
    let mut key = None;
    let mut chars = caption.chars();
    while let Some(c) = chars.next() {
        if c != '&' {
            text.push(c);
            continue;
        }
        match chars.next() {
            Some('&') => text.push('&'),
            Some(n) => {
                if key.is_none() {
                    key = n.to_lowercase().next();
                }
                text.push(n);
            }
            None => {}
        }
    }
    (text, key)
}

/// The upstream caption of a message-box button. Callers may pass `&`
/// captions; plain ones get the access key TortoiseGit's string table gives
/// them (IDS_MSGBOX_*, IDS_*BUTTON). OK and Cancel have none.
fn upstream_caption(label: &str) -> String {
    const FIRST_LETTER: [&str; 13] = [
        "Yes",
        "No",
        "Abort",
        "Retry",
        "Ignore",
        "Continue",
        "Remove",
        "Stash",
        "Proceed",
        "Skip",
        "Merge",
        "Rebase",
        "Try Again",
    ];
    if !label.contains('&') && FIRST_LETTER.contains(&label) {
        format!("&{label}")
    } else {
        label.to_owned()
    }
}

/// The button Escape answers. CMessageBox maps Escape to the "negative" button
/// (Cancel, else Abort, else No); a lone OK button closes. -1 = none.
pub fn cancel_index(labels: &[String]) -> i32 {
    let find = |name: &str| labels.iter().position(|l| l.eq_ignore_ascii_case(name));
    find("Cancel")
        .or_else(|| find("Abort"))
        .or_else(|| find("No"))
        .or((labels.len() == 1).then_some(0))
        .map_or(-1, |i| i as i32)
}

/// Fills a message box: title, text, the buttons with their access keys, the
/// Escape button, and Ctrl+C / Ctrl+Insert copying the text.
pub fn configure(dlg: &MessageBox, title: &str, text: &str, buttons: &[&str]) {
    dlg.set_dialog_title(title.into());
    dlg.set_text(text.into());
    let (labels, keys): (Vec<String>, Vec<SharedString>) = buttons
        .iter()
        .map(|b| {
            let (label, key) = split_mnemonic(&upstream_caption(b));
            (label, key.map(String::from).unwrap_or_default().into())
        })
        .unzip();
    dlg.set_cancel_button(cancel_index(&labels));
    let labels: Vec<SharedString> = labels.into_iter().map(Into::into).collect();
    dlg.set_buttons(ModelRc::new(VecModel::from(labels)));
    dlg.set_button_keys(ModelRc::new(VecModel::from(keys)));
    let text = text.to_owned();
    dlg.on_copy_text(move || {
        crate::clipboard::copy(&text);
    });
}

/// Shows a message box and returns the index of the clicked button, or `None`
/// if the window was closed. With `check` the second value is the checkbox state.
pub fn ask_with_check(
    title: &str,
    text: &str,
    buttons: &[&str],
    check: Option<&str>,
) -> (Option<usize>, bool) {
    let Ok(dlg) = MessageBox::new() else {
        eprintln!("{title}: {text}");
        return (None, false);
    };
    configure(&dlg, title, text, buttons);
    dlg.set_check_text(check.unwrap_or_default().into());
    let choice = std::rc::Rc::new(std::cell::Cell::new(None));
    {
        let choice = choice.clone();
        let weak = dlg.as_weak();
        dlg.on_clicked(move |i| {
            choice.set(Some(i as usize));
            if let Some(d) = weak.upgrade() {
                let _ = d.hide();
            }
        });
    }
    crate::snapshot::arm(&dlg);
    let _ = dlg.run();
    (choice.get(), dlg.get_checked())
}

pub fn ask(title: &str, text: &str, buttons: &[&str]) -> Option<usize> {
    ask_with_check(title, text, buttons, None).0
}

pub fn info(title: &str, text: &str) {
    ask(title, text, &["OK"]);
}

pub fn error(title: &str, text: &str) {
    ask(title, text, &["OK"]);
}

/// Non-blocking message box for use inside another dialog's event loop:
/// `on_answer` receives the clicked button index (`None` if closed).
pub fn ask_async(
    title: &str,
    text: &str,
    buttons: &[&str],
    on_answer: impl FnOnce(Option<usize>) + 'static,
) {
    let Ok(dlg) = MessageBox::new() else {
        on_answer(None);
        return;
    };
    configure(&dlg, title, text, buttons);
    let answer = std::rc::Rc::new(std::cell::RefCell::new(Some(
        Box::new(on_answer) as Box<dyn FnOnce(Option<usize>)>
    )));
    {
        let answer = answer.clone();
        let weak = dlg.as_weak();
        dlg.on_clicked(move |i| {
            if let Some(d) = weak.upgrade() {
                let _ = d.hide();
            }
            if let Some(f) = answer.borrow_mut().take() {
                f(Some(i as usize));
            }
        });
    }
    {
        let answer = answer.clone();
        dlg.window().on_close_requested(move || {
            if let Some(f) = answer.borrow_mut().take() {
                f(None);
            }
            slint::CloseRequestResponse::HideWindow
        });
    }
    if dlg.show().is_ok() {
        crate::keep_alive(dlg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mnemonics_and_escape_button() {
        assert_eq!(
            split_mnemonic("Pus&h..."),
            ("Push...".to_owned(), Some('h'))
        );
        assert_eq!(
            split_mnemonic("Fetch && Re&base"),
            ("Fetch & Rebase".to_owned(), Some('b'))
        );
        assert_eq!(split_mnemonic("OK"), ("OK".to_owned(), None));
        assert_eq!(upstream_caption("Yes"), "&Yes");
        assert_eq!(upstream_caption("OK"), "OK");
        let labels = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(cancel_index(&labels(&["Yes", "No"])), 1);
        assert_eq!(cancel_index(&labels(&["Yes", "No", "Cancel"])), 2);
        assert_eq!(cancel_index(&labels(&["Merge", "Rebase", "Abort"])), 2);
        assert_eq!(cancel_index(&labels(&["OK"])), 0);
        assert_eq!(cancel_index(&labels(&["Commit", "Push"])), -1);
    }
}
