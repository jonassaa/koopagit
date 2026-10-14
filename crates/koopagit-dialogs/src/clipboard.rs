//! Copying text to the system clipboard. Slint has no public clipboard API,
//! so this uses `arboard`, falling back to each platform's command-line tool.
//! Callers fall back to showing the text when both fail.

use std::io::Write;
use std::process::{Command, Stdio};

#[cfg(target_os = "macos")]
fn candidates() -> &'static [(&'static str, &'static [&'static str])] {
    &[("pbcopy", &[])]
}

#[cfg(target_os = "windows")]
fn candidates() -> &'static [(&'static str, &'static [&'static str])] {
    &[("clip", &[])]
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn candidates() -> &'static [(&'static str, &'static [&'static str])] {
    &[
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard"]),
        ("xsel", &["--clipboard", "--input"]),
    ]
}

/// Puts `text` on the clipboard. Returns `false` if nothing worked.
pub fn copy(text: &str) -> bool {
    if arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text.to_owned()))
        .is_ok()
    {
        return true;
    }
    for (exe, args) in candidates() {
        let Ok(mut child) = Command::new(exe)
            .args(*args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            continue;
        };
        let wrote = child
            .stdin
            .take()
            .is_some_and(|mut s| s.write_all(text.as_bytes()).is_ok());
        if wrote && child.wait().is_ok_and(|s| s.success()) {
            return true;
        }
    }
    false
}

/// The clipboard's text, if any.
pub fn text() -> Option<String> {
    arboard::Clipboard::new()
        .and_then(|mut c| c.get_text())
        .ok()
}
