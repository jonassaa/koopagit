//! Multi-row selection for list views (Explorer/ListView semantics).

use std::collections::BTreeSet;

/// Modifier state of a click.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mods {
    /// Ctrl on Windows/Linux, Cmd on macOS: toggle one row.
    pub toggle: bool,
    /// Shift: extend from the anchor.
    pub range: bool,
}

/// Selected rows plus the anchor/current row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    rows: BTreeSet<usize>,
    anchor: Option<usize>,
    current: Option<usize>,
}

impl Selection {
    pub fn rows(&self) -> impl Iterator<Item = usize> + '_ {
        self.rows.iter().copied()
    }

    pub fn to_vec(&self) -> Vec<usize> {
        self.rows.iter().copied().collect()
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn contains(&self, row: usize) -> bool {
        self.rows.contains(&row)
    }

    pub fn current(&self) -> Option<usize> {
        self.current
    }

    /// Left click with modifiers.
    pub fn click(&mut self, row: usize, mods: Mods) {
        if mods.range {
            let anchor = self.anchor.unwrap_or(row);
            if !mods.toggle {
                self.rows.clear();
            }
            let (a, b) = if anchor <= row {
                (anchor, row)
            } else {
                (row, anchor)
            };
            self.rows.extend(a..=b);
        } else if mods.toggle {
            if !self.rows.remove(&row) {
                self.rows.insert(row);
            }
            self.anchor = Some(row);
        } else {
            self.select_only(row);
        }
        self.current = Some(row);
    }

    /// Right press: an unselected row becomes the only selection; a selected
    /// row keeps the selection (so the menu acts on all selected rows).
    pub fn right_click(&mut self, row: usize) {
        if !self.rows.contains(&row) {
            self.select_only(row);
        }
        self.current = Some(row);
    }

    pub fn select_only(&mut self, row: usize) {
        self.rows.clear();
        self.rows.insert(row);
        self.anchor = Some(row);
        self.current = Some(row);
    }

    /// Ctrl/Cmd+A.
    pub fn select_all(&mut self, len: usize) {
        self.rows = (0..len).collect();
        self.clamp(len);
    }

    /// Arrow keys: move the current row by `delta`; Shift extends.
    pub fn step(&mut self, delta: isize, len: usize, extend: bool) {
        if len == 0 {
            return;
        }
        // Like a list view: an arrow key with no focused row focuses the first one.
        let next = match self.current {
            None if delta.abs() == 1 => 0,
            cur => (cur.unwrap_or(0) as isize + delta).clamp(0, len as isize - 1) as usize,
        };
        self.click(
            next,
            Mods {
                toggle: false,
                range: extend,
            },
        );
        if !extend {
            self.anchor = Some(next);
        }
    }

    /// Drops rows beyond `len` (after a refresh shrank the list).
    pub fn clamp(&mut self, len: usize) {
        self.rows.retain(|r| *r < len);
        if self.current.is_some_and(|c| c >= len) {
            self.current = None;
        }
        if self.anchor.is_some_and(|a| a >= len) {
            self.anchor = None;
        }
    }

    /// Whether the selected rows form one contiguous run.
    pub fn is_contiguous(&self) -> bool {
        match (self.rows.first(), self.rows.last()) {
            (Some(a), Some(b)) => b - a + 1 == self.rows.len(),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: Mods = Mods {
        toggle: false,
        range: false,
    };
    const CTRL: Mods = Mods {
        toggle: true,
        range: false,
    };
    const SHIFT: Mods = Mods {
        toggle: false,
        range: true,
    };

    #[test]
    fn click_semantics() {
        let mut s = Selection::default();
        s.click(2, PLAIN);
        s.click(5, SHIFT);
        assert_eq!(s.to_vec(), [2, 3, 4, 5]);
        assert!(s.is_contiguous());
        s.click(3, CTRL);
        assert_eq!(s.to_vec(), [2, 4, 5]);
        assert!(!s.is_contiguous());
        s.right_click(4);
        assert_eq!(s.len(), 3, "right-click inside the selection keeps it");
        s.right_click(9);
        assert_eq!(s.to_vec(), [9]);
        s.click(7, SHIFT);
        assert_eq!(s.to_vec(), [7, 8, 9]);
        s.select_all(4);
        assert_eq!(s.len(), 4);
        s.click(0, PLAIN);
        s.step(1, 4, false);
        s.step(1, 4, true);
        assert_eq!(s.current(), Some(2));
        s.clamp(2);
        assert!(s.rows().all(|r| r < 2));
    }
}
