//! Glue between a list view, [`Selection`] and a [`MenuCache`]: click and
//! keyboard handling, and the rows a context-menu action applies to.

use std::cell::RefCell;

use crate::menus::MenuCache;
use crate::selection::{Mods, Selection};

#[derive(Default)]
pub struct ListSel {
    pub sel: RefCell<Selection>,
    pub cache: MenuCache,
}

impl ListSel {
    pub fn click(&self, row: usize, toggle: bool, range: bool) {
        self.sel.borrow_mut().click(row, Mods { toggle, range });
        self.cache.invalidate();
    }

    pub fn right(&self, row: usize) {
        self.sel.borrow_mut().right_click(row);
        self.cache.invalidate();
    }

    /// Arrow keys (`delta` ±1) or Ctrl/Cmd+A (`delta` 0).
    pub fn key(&self, delta: i32, extend: bool, len: usize) {
        let mut s = self.sel.borrow_mut();
        if delta == 0 {
            s.select_all(len);
        } else {
            s.step(delta as isize, len, extend);
        }
        drop(s);
        self.cache.invalidate();
    }

    /// After the list was reloaded with `len` rows.
    pub fn reset(&self, len: usize, select_first: bool) {
        let mut s = self.sel.borrow_mut();
        *s = Selection::default();
        if select_first && len > 0 {
            s.select_only(0);
        }
        drop(s);
        self.cache.invalidate();
    }

    /// The rows a menu opened on `row` acts on: the selection if it contains
    /// `row`, else just `row` (sorted, top first).
    pub fn effective(&self, row: usize) -> Vec<usize> {
        let s = self.sel.borrow();
        if s.contains(row) {
            s.to_vec()
        } else {
            vec![row]
        }
    }

    pub fn is_selected(&self, row: usize) -> bool {
        self.sel.borrow().contains(row)
    }

    /// Whether `rows` form one contiguous run.
    pub fn contiguous(rows: &[usize]) -> bool {
        match (rows.first(), rows.last()) {
            (Some(a), Some(b)) => b - a + 1 == rows.len(),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_rows() {
        let l = ListSel::default();
        l.click(1, false, false);
        l.click(3, false, true);
        assert_eq!(l.effective(2), [1, 2, 3]);
        assert_eq!(l.effective(7), [7]);
        assert!(ListSel::contiguous(&[1, 2, 3]));
        assert!(!ListSel::contiguous(&[1, 3]));
        l.key(0, false, 5);
        assert_eq!(l.effective(0).len(), 5);
        l.reset(5, true);
        assert_eq!(l.effective(0), [0]);
        assert!(l.is_selected(0));
    }
}
