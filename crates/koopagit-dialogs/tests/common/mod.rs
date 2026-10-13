//! Headless key-event helpers shared by the `keyboard_*` tests.
#![allow(dead_code)]

use std::cell::Cell;
use std::rc::Rc;

use slint::platform::{Key, WindowEvent};

#[allow(unused_imports)]
pub use slint::platform::Key as K;

/// Must run before the first window of a test thread is created.
pub fn init() {
    i_slint_backend_testing::init_no_event_loop();
}

pub fn press(win: &slint::Window, text: impl Into<slint::SharedString>) {
    let text = text.into();
    win.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    win.dispatch_event(WindowEvent::KeyReleased { text });
}

/// Presses `key` while holding every key in `modifiers` (e.g. `[Key::Control, Key::Shift]`).
pub fn chord<const N: usize>(
    win: &slint::Window,
    modifiers: [Key; N],
    key: impl Into<slint::SharedString>,
) {
    for m in modifiers {
        win.dispatch_event(WindowEvent::KeyPressed { text: m.into() });
    }
    press(win, key);
    for m in modifiers.into_iter().rev() {
        win.dispatch_event(WindowEvent::KeyReleased { text: m.into() });
    }
}

pub fn ctrl(win: &slint::Window, key: impl Into<slint::SharedString>) {
    chord(win, [Key::Control], key);
}

pub fn alt(win: &slint::Window, key: impl Into<slint::SharedString>) {
    chord(win, [Key::Alt], key);
}

pub fn shift(win: &slint::Window, key: impl Into<slint::SharedString>) {
    chord(win, [Key::Shift], key);
}

/// Types plain text into whatever has focus.
pub fn type_text(win: &slint::Window, text: &str) {
    for c in text.chars() {
        press(win, c.to_string());
    }
}

/// Presses and releases `button` at a window position.
pub fn click(win: &slint::Window, x: f32, y: f32, button: slint::platform::PointerEventButton) {
    let position = slint::LogicalPosition::new(x, y);
    win.dispatch_event(WindowEvent::PointerMoved { position });
    win.dispatch_event(WindowEvent::PointerPressed { position, button });
    win.dispatch_event(WindowEvent::PointerReleased { position, button });
}

/// A call counter and a callback that bumps it.
pub fn counter() -> (Rc<Cell<u32>>, impl Fn() + 'static) {
    let n = Rc::new(Cell::new(0));
    let c = n.clone();
    (n, move || c.set(c.get() + 1))
}

/// Access keys are off on macOS by default; tests turn them on everywhere.
#[macro_export]
macro_rules! enable_access_keys {
    ($dlg:expr) => {
        $dlg.global::<koopagit_dialogs::KeyboardPrefs>()
            .set_access_keys(true)
    };
}
