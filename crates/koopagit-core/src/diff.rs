//! Line-based diffing for the side-by-side viewer (TortoiseGitMerge).
//!
//! Myers' O((N+M)D) algorithm on lines, after stripping the common prefix and
//! suffix, followed by an alignment pass that pairs removed/added runs into
//! "changed" rows so both panes scroll together.

/// Whitespace handling (TortoiseGitMerge's "Ignore whitespace" options).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Whitespace {
    /// Compare lines exactly.
    #[default]
    Compare,
    /// Ignore changes in the amount of whitespace (like `git diff -b`).
    IgnoreChanges,
    /// Ignore all whitespace (like `git diff -w`).
    IgnoreAll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiffOptions {
    pub whitespace: Whitespace,
    pub ignore_case: bool,
    /// Treat `\r\n` and `\n` line endings as equal.
    pub ignore_eol: bool,
}

/// One step of an edit script: indices into the left (`a`) and right (`b`) lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edit {
    Equal(usize, usize),
    Delete(usize),
    Insert(usize),
}

/// How a row of the aligned view differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Equal,
    /// Only on the left.
    Removed,
    /// Only on the right.
    Added,
    /// Present on both sides with different content.
    Changed,
}

/// A row of the side-by-side view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlignedRow {
    pub left: Option<usize>,
    pub right: Option<usize>,
    pub kind: RowKind,
}

/// Splits text into lines, keeping no terminators; a trailing newline does not
/// produce an extra empty line.
pub fn split_lines(text: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = text.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    lines
}

fn normalize(line: &str, o: &DiffOptions) -> String {
    let line = if o.ignore_eol {
        line.trim_end_matches('\r')
    } else {
        line
    };
    let mut s: String = match o.whitespace {
        Whitespace::Compare => line.to_owned(),
        Whitespace::IgnoreAll => line.chars().filter(|c| !c.is_whitespace()).collect(),
        Whitespace::IgnoreChanges => {
            let mut out = String::with_capacity(line.len());
            let mut in_ws = false;
            for c in line.trim_end().chars() {
                if c.is_whitespace() {
                    if !in_ws {
                        out.push(' ');
                    }
                    in_ws = true;
                } else {
                    out.push(c);
                    in_ws = false;
                }
            }
            out
        }
    };
    if o.ignore_case {
        s = s.to_lowercase();
    }
    s
}

/// Computes a minimal edit script turning `a` into `b`.
pub fn diff_lines(a: &[&str], b: &[&str], o: &DiffOptions) -> Vec<Edit> {
    let na: Vec<String> = a.iter().map(|l| normalize(l, o)).collect();
    let nb: Vec<String> = b.iter().map(|l| normalize(l, o)).collect();

    let mut prefix = 0;
    while prefix < na.len() && prefix < nb.len() && na[prefix] == nb[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < na.len() - prefix
        && suffix < nb.len() - prefix
        && na[na.len() - 1 - suffix] == nb[nb.len() - 1 - suffix]
    {
        suffix += 1;
    }

    let mut edits: Vec<Edit> = (0..prefix).map(|i| Edit::Equal(i, i)).collect();
    let mid_a = &na[prefix..na.len() - suffix];
    let mid_b = &nb[prefix..nb.len() - suffix];
    for e in myers(mid_a, mid_b) {
        edits.push(match e {
            Edit::Equal(i, j) => Edit::Equal(i + prefix, j + prefix),
            Edit::Delete(i) => Edit::Delete(i + prefix),
            Edit::Insert(j) => Edit::Insert(j + prefix),
        });
    }
    for k in 0..suffix {
        edits.push(Edit::Equal(na.len() - suffix + k, nb.len() - suffix + k));
    }
    edits
}

fn myers(a: &[String], b: &[String]) -> Vec<Edit> {
    let (n, m) = (a.len() as isize, b.len() as isize);
    if n == 0 {
        return (0..b.len()).map(Edit::Insert).collect();
    }
    if m == 0 {
        return (0..a.len()).map(Edit::Delete).collect();
    }
    let max = (n + m) as usize;
    let offset = max as isize;
    let mut v = vec![0isize; 2 * max + 2];
    let mut trace: Vec<Vec<isize>> = Vec::new();
    'outer: for d in 0..=max as isize {
        trace.push(v.clone());
        let mut k = -d;
        while k <= d {
            let idx = (k + offset) as usize;
            let mut x = if k == -d || (k != d && v[idx - 1] < v[idx + 1]) {
                v[idx + 1]
            } else {
                v[idx - 1] + 1
            };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[idx] = x;
            if x >= n && y >= m {
                break 'outer;
            }
            k += 2;
        }
    }
    // Backtrack through the saved V arrays.
    let mut edits = Vec::new();
    let (mut x, mut y) = (n, m);
    for d in (0..trace.len() as isize).rev() {
        let v = &trace[d as usize];
        let k = x - y;
        let prev_k =
            if k == -d || (k != d && v[(k - 1 + offset) as usize] < v[(k + 1 + offset) as usize]) {
                k + 1
            } else {
                k - 1
            };
        let prev_x = v[(prev_k + offset) as usize];
        let prev_y = prev_x - prev_k;
        while x > prev_x && y > prev_y {
            x -= 1;
            y -= 1;
            edits.push(Edit::Equal(x as usize, y as usize));
        }
        if d > 0 {
            if x == prev_x {
                y -= 1;
                edits.push(Edit::Insert(y as usize));
            } else {
                x -= 1;
                edits.push(Edit::Delete(x as usize));
            }
        }
    }
    edits.reverse();
    edits
}

/// Aligns an edit script into side-by-side rows: within each block of
/// deletions/insertions, lines are paired as "changed" and the rest padded.
pub fn align(edits: &[Edit]) -> Vec<AlignedRow> {
    let mut rows = Vec::with_capacity(edits.len());
    let mut dels = Vec::new();
    let mut ins = Vec::new();
    let flush = |rows: &mut Vec<AlignedRow>, dels: &mut Vec<usize>, ins: &mut Vec<usize>| {
        let n = dels.len().max(ins.len());
        for k in 0..n {
            let (l, r) = (dels.get(k).copied(), ins.get(k).copied());
            let kind = match (l, r) {
                (Some(_), Some(_)) => RowKind::Changed,
                (Some(_), None) => RowKind::Removed,
                _ => RowKind::Added,
            };
            rows.push(AlignedRow {
                left: l,
                right: r,
                kind,
            });
        }
        dels.clear();
        ins.clear();
    };
    for e in edits {
        match *e {
            Edit::Equal(i, j) => {
                flush(&mut rows, &mut dels, &mut ins);
                rows.push(AlignedRow {
                    left: Some(i),
                    right: Some(j),
                    kind: RowKind::Equal,
                });
            }
            Edit::Delete(i) => dels.push(i),
            Edit::Insert(j) => ins.push(j),
        }
    }
    flush(&mut rows, &mut dels, &mut ins);
    rows
}

/// Start row of every block of differences (for next/previous difference).
pub fn difference_blocks(rows: &[AlignedRow]) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut prev_equal = true;
    for (i, r) in rows.iter().enumerate() {
        let eq = r.kind == RowKind::Equal;
        if !eq && prev_equal {
            starts.push(i);
        }
        prev_equal = eq;
    }
    starts
}

/// The next block start after `current` (`None` at the end).
pub fn next_block(blocks: &[usize], current: Option<usize>) -> Option<usize> {
    match current {
        None => blocks.first().copied(),
        Some(c) => blocks.iter().copied().find(|&b| b > c),
    }
}

/// The previous block start before `current`.
pub fn prev_block(blocks: &[usize], current: Option<usize>) -> Option<usize> {
    let c = current?;
    blocks.iter().copied().rev().find(|&b| b < c)
}

/// Renders an edit script as a unified diff with `context` lines.
pub fn unified(
    a: &[&str],
    b: &[&str],
    edits: &[Edit],
    name_a: &str,
    name_b: &str,
    context: usize,
) -> String {
    let mut out = format!("--- {name_a}\n+++ {name_b}\n");
    let n = edits.len();
    let changed: Vec<usize> = (0..n)
        .filter(|&i| !matches!(edits[i], Edit::Equal(..)))
        .collect();
    if changed.is_empty() {
        return out;
    }
    let mut groups: Vec<(usize, usize)> = Vec::new();
    for &c in &changed {
        let start = c.saturating_sub(context);
        let end = (c + context + 1).min(n);
        match groups.last_mut() {
            Some(g) if start <= g.1 => g.1 = end,
            _ => groups.push((start, end)),
        }
    }
    for (start, end) in groups {
        let (mut a0, mut b0) = (None, None);
        let (mut ac, mut bc) = (0, 0);
        let mut body = String::new();
        for e in &edits[start..end] {
            match *e {
                Edit::Equal(i, j) => {
                    a0.get_or_insert(i);
                    b0.get_or_insert(j);
                    ac += 1;
                    bc += 1;
                    body.push_str(&format!(" {}\n", a[i]));
                }
                Edit::Delete(i) => {
                    a0.get_or_insert(i);
                    ac += 1;
                    body.push_str(&format!("-{}\n", a[i]));
                }
                Edit::Insert(j) => {
                    b0.get_or_insert(j);
                    bc += 1;
                    body.push_str(&format!("+{}\n", b[j]));
                }
            }
        }
        let a_start = a0.map_or(0, |v| v + 1);
        let b_start = b0.map_or(0, |v| v + 1);
        out.push_str(&format!("@@ -{a_start},{ac} +{b_start},{bc} @@\n{body}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(a: &[&str], b: &[&str], edits: &[Edit]) -> Vec<String> {
        let mut out = Vec::new();
        let (mut ia, mut ib) = (0, 0);
        for e in edits {
            match *e {
                Edit::Equal(i, j) => {
                    assert_eq!((i, j), (ia, ib));
                    out.push(a[i].to_owned());
                    ia += 1;
                    ib += 1;
                }
                Edit::Delete(i) => {
                    assert_eq!(i, ia);
                    ia += 1;
                }
                Edit::Insert(j) => {
                    assert_eq!(j, ib);
                    out.push(b[j].to_owned());
                    ib += 1;
                }
            }
        }
        assert_eq!((ia, ib), (a.len(), b.len()));
        out
    }

    #[test]
    fn edit_script_reconstructs_target() {
        let cases: [(&str, &str); 5] = [
            ("a\nb\nc\n", "a\nx\nc\n"),
            ("", "one\ntwo\n"),
            ("one\ntwo\n", ""),
            ("a\nb\nc\nd\ne\n", "b\nc\nX\ne\nf\n"),
            ("same\n", "same\n"),
        ];
        for (x, y) in cases {
            let (a, b) = (split_lines(x), split_lines(y));
            let edits = diff_lines(&a, &b, &DiffOptions::default());
            assert_eq!(apply(&a, &b, &edits), b, "{x:?} -> {y:?}");
        }
    }

    #[test]
    fn minimal_and_aligned() {
        let a = split_lines("a\nb\nc\nd\n");
        let b = split_lines("a\nB\nc\nd\ne\n");
        let edits = diff_lines(&a, &b, &DiffOptions::default());
        let changes = edits
            .iter()
            .filter(|e| !matches!(e, Edit::Equal(..)))
            .count();
        assert_eq!(changes, 3);
        let rows = align(&edits);
        assert_eq!(
            rows.iter().map(|r| r.kind).collect::<Vec<_>>(),
            [
                RowKind::Equal,
                RowKind::Changed,
                RowKind::Equal,
                RowKind::Equal,
                RowKind::Added
            ]
        );
        let blocks = difference_blocks(&rows);
        assert_eq!(blocks, [1, 4]);
        assert_eq!(next_block(&blocks, None), Some(1));
        assert_eq!(next_block(&blocks, Some(1)), Some(4));
        assert_eq!(next_block(&blocks, Some(4)), None);
        assert_eq!(prev_block(&blocks, Some(4)), Some(1));
    }

    #[test]
    fn whitespace_options() {
        let a = split_lines("int  x = 1;\nfoo\n");
        let b = split_lines("int x = 1;   \nFOO\n");
        let strict = diff_lines(&a, &b, &DiffOptions::default());
        assert!(strict.iter().any(|e| !matches!(e, Edit::Equal(..))));
        let loose = DiffOptions {
            whitespace: Whitespace::IgnoreChanges,
            ignore_case: true,
            ignore_eol: true,
        };
        assert!(
            diff_lines(&a, &b, &loose)
                .iter()
                .all(|e| matches!(e, Edit::Equal(..)))
        );
    }

    #[test]
    fn unified_output() {
        let a = split_lines("1\n2\n3\n");
        let b = split_lines("1\n2x\n3\n");
        let e = diff_lines(&a, &b, &DiffOptions::default());
        assert_eq!(
            unified(&a, &b, &e, "a/f", "b/f", 3),
            "--- a/f\n+++ b/f\n@@ -1,3 +1,3 @@\n 1\n-2\n+2x\n 3\n"
        );
    }
}
