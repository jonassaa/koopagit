//! Editable two-pane document for TortoiseGitMerge's block commands
//! ("Use this text block", "Use both text blocks", "Mark this block", ...).
//!
//! The left pane is the base/original, the right pane is the modified file
//! ("mine"), which is the default target of every "use" command. A *hunk* is
//! one contiguous block of differences, as in the viewer.

use std::collections::BTreeSet;

use crate::diff::{self, DiffOptions, Edit};

/// A block of differences: `left[l..l+ll]` versus `right[r..r+rl]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hunk {
    pub left: usize,
    pub left_len: usize,
    pub right: usize,
    pub right_len: usize,
}

/// A block command (spec-menus.md §8.3/§8.4, two-pane variants).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockOp {
    /// Right block := left block ("Use this text block" on the left,
    /// "Use other text block" on the right).
    UseLeftBlock,
    /// Right block := left block + right block.
    UseBothLeftFirst,
    /// Right block := right block + left block.
    UseBothRightFirst,
    /// Left block := right block + left block ("Prepend right block").
    PrependRightToLeft,
    /// Left block := right block ("Use right block" / "Use this block on left").
    ReplaceLeftByRight,
    /// Left block := left block + right block ("Append right block").
    AppendRightToLeft,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffDoc {
    pub left: Vec<String>,
    pub right: Vec<String>,
    /// Marked right-pane lines ("Mark this block").
    pub marks: BTreeSet<usize>,
    pub left_modified: bool,
    pub right_modified: bool,
}

/// Splits file text into lines without terminators.
pub fn lines_of(text: &str) -> Vec<String> {
    diff::split_lines(text)
        .into_iter()
        .map(|l| l.trim_end_matches('\r').to_owned())
        .collect()
}

impl DiffDoc {
    pub fn new(left: &str, right: &str) -> Self {
        DiffDoc {
            left: lines_of(left),
            right: lines_of(right),
            ..Default::default()
        }
    }

    /// The hunks for the current text.
    pub fn hunks(&self, opts: &DiffOptions) -> Vec<Hunk> {
        let a: Vec<&str> = self.left.iter().map(String::as_str).collect();
        let b: Vec<&str> = self.right.iter().map(String::as_str).collect();
        hunks_of(&diff::diff_lines(&a, &b, opts), a.len(), b.len())
    }

    /// The hunk containing right line `r` or left line `l` (whichever is given).
    pub fn hunk_at(
        &self,
        opts: &DiffOptions,
        left: Option<usize>,
        right: Option<usize>,
    ) -> Option<Hunk> {
        self.hunks(opts).into_iter().find(|h| {
            let in_left = left.is_some_and(|l| l >= h.left && l < h.left + h.left_len.max(1));
            let in_right = right.is_some_and(|r| r >= h.right && r < h.right + h.right_len.max(1));
            in_left || in_right
        })
    }

    pub fn apply(&mut self, hunk: Hunk, op: BlockOp) {
        let l: Vec<String> = self.left[hunk.left..hunk.left + hunk.left_len].to_vec();
        let r: Vec<String> = self.right[hunk.right..hunk.right + hunk.right_len].to_vec();
        match op {
            BlockOp::UseLeftBlock => self.replace_right(hunk, l),
            BlockOp::UseBothLeftFirst => self.replace_right(hunk, [l, r].concat()),
            BlockOp::UseBothRightFirst => self.replace_right(hunk, [r, l].concat()),
            BlockOp::PrependRightToLeft => self.replace_left(hunk, [r, l].concat()),
            BlockOp::ReplaceLeftByRight => self.replace_left(hunk, r),
            BlockOp::AppendRightToLeft => self.replace_left(hunk, [l, r].concat()),
        }
    }

    /// "Use this whole file" from the left: right := left.
    pub fn use_left_file(&mut self) {
        self.right = self.left.clone();
        self.marks.clear();
        self.right_modified = true;
    }

    /// "Use whole other file" from the right onto the left: left := right.
    pub fn use_right_file(&mut self) {
        self.left = self.right.clone();
        self.left_modified = true;
    }

    /// Marks or unmarks the right lines of a hunk.
    pub fn mark(&mut self, hunk: Hunk, on: bool) {
        for i in hunk.right..hunk.right + hunk.right_len {
            if on {
                self.marks.insert(i);
            } else {
                self.marks.remove(&i);
            }
        }
    }

    pub fn is_marked(&self, hunk: Hunk) -> bool {
        hunk.right_len > 0
            && (hunk.right..hunk.right + hunk.right_len).all(|i| self.marks.contains(&i))
    }

    /// "Leave only marked blocks": every hunk without marks is reverted to the left text.
    pub fn leave_only_marked(&mut self, opts: &DiffOptions) {
        // Work from the bottom up so earlier hunk positions stay valid.
        let mut hunks = self.hunks(opts);
        hunks.reverse();
        for h in hunks {
            let marked = (h.right..h.right + h.right_len).any(|i| self.marks.contains(&i));
            if !marked {
                self.apply(h, BlockOp::UseLeftBlock);
            }
        }
    }

    pub fn right_text(&self, eol: &str) -> String {
        join(&self.right, eol)
    }

    pub fn left_text(&self, eol: &str) -> String {
        join(&self.left, eol)
    }

    fn replace_right(&mut self, h: Hunk, new: Vec<String>) {
        let n = new.len();
        self.right.splice(h.right..h.right + h.right_len, new);
        // Drop marks in the replaced range and shift the ones below it.
        let end = h.right + h.right_len;
        self.marks = self
            .marks
            .iter()
            .filter(|&&m| m < h.right || m >= end)
            .map(|&m| if m >= end { m + n - h.right_len } else { m })
            .collect();
        self.right_modified = true;
    }

    fn replace_left(&mut self, h: Hunk, new: Vec<String>) {
        self.left.splice(h.left..h.left + h.left_len, new);
        self.left_modified = true;
    }
}

fn join(lines: &[String], eol: &str) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let mut s = lines.join(eol);
    s.push_str(eol);
    s
}

/// Groups an edit script into hunks.
pub fn hunks_of(edits: &[Edit], left_len: usize, right_len: usize) -> Vec<Hunk> {
    let mut out = Vec::new();
    let (mut li, mut ri) = (0usize, 0usize);
    let mut cur: Option<Hunk> = None;
    for e in edits {
        match *e {
            Edit::Equal(i, j) => {
                if let Some(h) = cur.take() {
                    out.push(h);
                }
                li = i + 1;
                ri = j + 1;
            }
            Edit::Delete(i) => {
                let h = cur.get_or_insert(Hunk {
                    left: i,
                    left_len: 0,
                    right: ri,
                    right_len: 0,
                });
                h.left_len += 1;
                li = i + 1;
            }
            Edit::Insert(j) => {
                let h = cur.get_or_insert(Hunk {
                    left: li,
                    left_len: 0,
                    right: j,
                    right_len: 0,
                });
                h.right_len += 1;
                ri = j + 1;
            }
        }
    }
    if let Some(h) = cur {
        out.push(h);
    }
    let _ = (left_len, right_len);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> DiffDoc {
        DiffDoc::new("a\nb\nc\nd\ne\n", "a\nB\nc\nd\nx\ny\ne\n")
    }

    #[test]
    fn hunks_and_block_ops() {
        let o = DiffOptions::default();
        let d = doc();
        let h = d.hunks(&o);
        assert_eq!(
            h,
            [
                Hunk {
                    left: 1,
                    left_len: 1,
                    right: 1,
                    right_len: 1
                },
                Hunk {
                    left: 4,
                    left_len: 0,
                    right: 4,
                    right_len: 2
                },
            ]
        );
        let mut x = d.clone();
        x.apply(h[0], BlockOp::UseLeftBlock);
        assert_eq!(x.right_text("\n"), "a\nb\nc\nd\nx\ny\ne\n");
        let mut x = d.clone();
        x.apply(h[0], BlockOp::UseBothLeftFirst);
        assert_eq!(x.right[1..3], ["b", "B"]);
        let mut x = d.clone();
        x.apply(h[0], BlockOp::UseBothRightFirst);
        assert_eq!(x.right[1..3], ["B", "b"]);
        let mut x = d.clone();
        x.apply(h[1], BlockOp::ReplaceLeftByRight);
        assert_eq!(x.left_text("\n"), "a\nb\nc\nd\nx\ny\ne\n");
        assert!(x.left_modified && !x.right_modified);
        let mut x = d.clone();
        x.apply(h[0], BlockOp::PrependRightToLeft);
        assert_eq!(x.left[1..3], ["B", "b"]);
        let mut x = d.clone();
        x.apply(h[0], BlockOp::AppendRightToLeft);
        assert_eq!(x.left[1..3], ["b", "B"]);
        assert_eq!(d.hunk_at(&o, None, Some(5)), Some(h[1]));
        assert_eq!(d.hunk_at(&o, Some(0), None), None);
        let mut x = d.clone();
        x.use_left_file();
        assert!(x.hunks(&o).is_empty());
    }

    #[test]
    fn marks_survive_and_leave_only_marked() {
        let o = DiffOptions::default();
        let mut d = doc();
        let h = d.hunks(&o);
        d.mark(h[1], true);
        assert!(d.is_marked(h[1]) && !d.is_marked(h[0]));
        d.leave_only_marked(&o);
        assert_eq!(d.right_text("\n"), "a\nb\nc\nd\nx\ny\ne\n");
        // Marks shift when an earlier block grows.
        let mut d = doc();
        let h = d.hunks(&o);
        d.mark(h[1], true);
        d.apply(h[0], BlockOp::UseBothLeftFirst);
        assert!(
            d.marks.contains(&5) && d.marks.contains(&6),
            "{:?}",
            d.marks
        );
        d.mark(h[1], false);
    }
}
