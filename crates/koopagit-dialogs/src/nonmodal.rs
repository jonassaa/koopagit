//! Keeping non-blocking windows (opened from inside another dialog's event
//! loop) alive until the process ends.

use std::any::Any;
use std::cell::RefCell;

thread_local! {
    static WINDOWS: RefCell<Vec<Box<dyn Any>>> = const { RefCell::new(Vec::new()) };
}

/// Holds a component handle so its window stays open after the caller returns.
pub fn keep_alive(handle: impl Any) {
    WINDOWS.with(|w| w.borrow_mut().push(Box::new(handle)));
}
