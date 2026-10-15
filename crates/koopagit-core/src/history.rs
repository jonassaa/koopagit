//! Pure logic behind the Log, RefLog and Browse References dialogs: the
//! revision graph lanes, log filters and limits, range parameters, reflog and
//! ref listings, and the files changed by a commit.

use crate::log::LogEntry;
use crate::status::{Action, StatusEntry};
use crate::{Repo, Result};

// ---------------------------------------------------------------------------
// Revision graph

/// A line drawn in one graph row, between lane positions. `from`/`to` are lane
/// indices; `upper` lines run from the row top to the node height, lower lines
/// from the node height to the row bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphLine {
    pub from: usize,
    pub to: usize,
    pub upper: bool,
}

/// The graph cell of one log row.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GraphRow {
    /// Lane holding this commit's node.
    pub node: usize,
    pub lines: Vec<GraphLine>,
    /// Number of lanes in use in this row (for the column width).
    pub width: usize,
}

/// Assigns lanes for commits listed child-before-parent (as the log shows
/// them), like TortoiseGit's `lanes.cpp`: a line of history keeps its lane
/// until it reaches the commit it waits for, so lanes never shift sideways.
/// Every lane waiting for a commit converges into its node (the leftmost such
/// lane); the first parent continues in the node's lane, other parents join a
/// lane already waiting for them or open a new one.
pub fn graph<'a>(commits: impl IntoIterator<Item = (&'a str, &'a [String])>) -> Vec<GraphRow> {
    let mut active: Vec<Option<String>> = Vec::new();
    let mut rows = Vec::new();
    for (id, parents) in commits {
        let col = match active.iter().position(|l| l.as_deref() == Some(id)) {
            Some(c) => c,
            None => match active.iter().position(Option::is_none) {
                Some(c) => c,
                None => {
                    active.push(None);
                    active.len() - 1
                }
            },
        };
        let mut lines = Vec::new();
        // Upper half: lanes waiting for this commit converge into the node,
        // every other lane passes straight through.
        for (i, lane) in active.iter_mut().enumerate() {
            let Some(l) = lane.as_deref() else { continue };
            let waits = l == id;
            lines.push(GraphLine {
                from: i,
                to: if waits { col } else { i },
                upper: true,
            });
            if waits {
                *lane = None;
            } else {
                lines.push(GraphLine {
                    from: i,
                    to: i,
                    upper: false,
                });
            }
        }
        // Lower half: lines from the node to each parent's lane.
        let mut parent_lanes: Vec<usize> = Vec::new();
        for (n, p) in parents.iter().enumerate() {
            let lane = if n == 0 {
                active[col] = Some(p.clone());
                col
            } else if let Some(waiting) = active.iter().position(|l| l.as_ref() == Some(p)) {
                waiting
            } else if let Some(free) = active.iter().position(Option::is_none) {
                active[free] = Some(p.clone());
                free
            } else {
                active.push(Some(p.clone()));
                active.len() - 1
            };
            if !parent_lanes.contains(&lane) {
                parent_lanes.push(lane);
                lines.push(GraphLine {
                    from: col,
                    to: lane,
                    upper: false,
                });
            }
        }
        let width = active.len().max(col + 1);
        while active.last().is_some_and(Option::is_none) {
            active.pop();
        }
        rows.push(GraphRow {
            node: col,
            lines,
            width,
        });
    }
    rows
}

// ---------------------------------------------------------------------------
// Log filter

/// Fields the Log dialog filter searches (`SelectedLogFilters`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilterFields {
    pub subject: bool,
    pub messages: bool,
    pub authors: bool,
    pub emails: bool,
    pub sha1: bool,
    pub refnames: bool,
    /// Changed paths (needs per-commit path lists, see [`LogFilter::matches_with`]).
    pub paths: bool,
    /// Git notes (needs per-commit notes).
    pub notes: bool,
}

impl Default for FilterFields {
    fn default() -> Self {
        FilterFields {
            subject: true,
            messages: true,
            authors: true,
            emails: false,
            sha1: true,
            refnames: true,
            paths: false,
            notes: false,
        }
    }
}

/// Text filter of the Log dialog: substring match, or a regular expression
/// with "Use regular expression" (`UseRegexFilter`).
#[derive(Debug, Clone, Default)]
pub struct LogFilter {
    pub text: String,
    pub fields: FilterFields,
    pub case_sensitive: bool,
    pub regex: bool,
}

impl LogFilter {
    /// Validates the filter; an invalid regular expression is an error message.
    pub fn compile(&self) -> std::result::Result<Option<regex::Regex>, String> {
        if !self.regex || self.text.is_empty() {
            return Ok(None);
        }
        regex::RegexBuilder::new(&self.text)
            .case_insensitive(!self.case_sensitive)
            .build()
            .map(Some)
            .map_err(|e| e.to_string())
    }

    pub fn matches(&self, e: &LogEntry) -> bool {
        self.matches_with(e, None, None)
    }

    /// Like [`LogFilter::matches`], with the commit's changed paths and notes
    /// for the Paths and Notes fields. An invalid regex matches nothing.
    pub fn matches_with(
        &self,
        e: &LogEntry,
        paths: Option<&[String]>,
        notes: Option<&str>,
    ) -> bool {
        if self.text.is_empty() {
            return true;
        }
        let Ok(re) = self.compile() else {
            return false;
        };
        let needle = if self.case_sensitive {
            self.text.clone()
        } else {
            self.text.to_lowercase()
        };
        let hit = |s: &str| match &re {
            Some(re) => re.is_match(s),
            None if self.case_sensitive => s.contains(&needle),
            None => s.to_lowercase().contains(&needle),
        };
        let f = self.fields;
        (f.subject && hit(&e.subject))
            || (f.messages && hit(&e.message))
            || (f.authors && (hit(&e.author.name) || hit(&e.committer.name)))
            || (f.emails && (hit(&e.author.email) || hit(&e.committer.email)))
            || (f.sha1 && hit(&e.id))
            || (f.refnames && e.refs.iter().any(|r| hit(&r.name)))
            || (f.paths && paths.is_some_and(|ps| ps.iter().any(|p| hit(p))))
            || (f.notes && notes.is_some_and(hit))
    }
}

// ---------------------------------------------------------------------------
// Limits and ranges

/// How many revisions the Log dialog loads (`NumberOfLogsScale` / `/limit`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLimit {
    None,
    Commits(usize),
    /// Only commits newer than this Unix time.
    Since(i64),
}

const DAY: i64 = 86_400;

/// Settings scale: 0 no limit, 1 last selected date, 2 commits, 3 years,
/// 4 months, 5 weeks. `now` is the current Unix time; dates count from today 00:00.
pub fn limit_from_scale(scale: u32, n: u32, now: i64) -> LogLimit {
    let today = now - now.rem_euclid(DAY);
    let n = i64::from(n.max(1));
    match scale {
        2 => LogLimit::Commits(n as usize),
        3 => LogLimit::Since(today - n * 365 * DAY),
        4 => LogLimit::Since(today - n * 30 * DAY),
        5 => LogLimit::Since(today - n * 7 * DAY),
        _ => LogLimit::None,
    }
}

/// Parses `/limit:"N SCALE"` (SCALE = commit/year/month/week; bare N = commits;
/// 0 = no limit).
pub fn parse_limit_param(value: &str, now: i64) -> LogLimit {
    let mut parts = value.split_whitespace();
    let n: u32 = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
    if n == 0 {
        return LogLimit::None;
    }
    let scale = match parts.next().map(str::to_lowercase).as_deref() {
        Some(s) if s.starts_with("year") => 3,
        Some(s) if s.starts_with("month") => 4,
        Some(s) if s.starts_with("week") => 5,
        _ => 2,
    };
    limit_from_scale(scale, n, now)
}

/// The working-tree pseudo revision (`GIT_REV_ZERO`).
pub const WORKING_TREE_REV: &str = "0000000000000000000000000000000000000000";

/// The range for `/range`, `/startrev`+`/endrev` (aliases `/revstart`,
/// `/revend`); the working-tree revision counts as empty.
pub fn range_param(range: Option<&str>, start: Option<&str>, end: Option<&str>) -> Option<String> {
    if let Some(r) = range.filter(|r| !r.is_empty()) {
        return Some(r.to_owned());
    }
    fn clean(r: Option<&str>) -> Option<&str> {
        r.filter(|r| !r.is_empty() && *r != WORKING_TREE_REV)
    }
    match (clean(start), clean(end)) {
        (Some(s), Some(e)) => Some(format!("{s}..{e}")),
        (Some(s), None) => Some(format!("{s}..")),
        (None, Some(e)) => Some(e.to_owned()),
        (None, None) => None,
    }
}

// ---------------------------------------------------------------------------
// Reflog

/// One reflog line (`CRefLogList`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReflogEntry {
    pub id: String,
    /// Selector such as `HEAD@{0}` or `refs/stash@{1}`.
    pub selector: String,
    /// The part of the reflog subject before `: ` (e.g. `commit`, `checkout`).
    pub action: String,
    pub message: String,
    pub time: i64,
    /// UTC offset of `time` in seconds.
    pub offset: i32,
}

const US: char = '\u{1f}';
const RS: char = '\u{1e}';

/// `git log -g` format consumed by [`parse_reflog`] (with `--date=raw`).
pub const REFLOG_FORMAT: &str = "--format=%H%x1f%gD%x1f%gs%x1f%cd%x1e";

/// Parses git's raw date `1700000000 +0200` into seconds and offset seconds.
pub fn parse_raw_date(s: &str) -> (i64, i32) {
    let mut parts = s.split_whitespace();
    let time = parts.next().and_then(|t| t.parse().ok()).unwrap_or(0);
    let offset = parts
        .next()
        .filter(|z| z.len() == 5)
        .and_then(|z| {
            let sign = if z.starts_with('-') { -1 } else { 1 };
            let h: i32 = z[1..3].parse().ok()?;
            let m: i32 = z[3..5].parse().ok()?;
            Some(sign * (h * 3600 + m * 60))
        })
        .unwrap_or(0);
    (time, offset)
}

pub fn parse_reflog(out: &str) -> Vec<ReflogEntry> {
    out.split(RS)
        .filter_map(|rec| {
            let rec = rec.trim_start_matches('\n');
            let mut f = rec.split(US);
            let id = f.next()?.to_owned();
            if id.is_empty() {
                return None;
            }
            let selector = f.next()?.to_owned();
            let subject = f.next().unwrap_or_default();
            let (time, offset) = parse_raw_date(f.next().unwrap_or_default());
            let (action, message) = match subject.split_once(": ") {
                Some((a, m)) => (a.to_owned(), m.to_owned()),
                None => (String::new(), subject.to_owned()),
            };
            Some(ReflogEntry {
                id,
                selector,
                action,
                message,
                time,
                offset,
            })
        })
        .collect()
}

impl Repo {
    /// Entries of the reflog of `refname` (`HEAD`, `refs/stash`, a branch ...).
    pub fn reflog(&self, refname: &str) -> Result<Vec<ReflogEntry>> {
        let out = self.git().output([
            "log",
            "-g",
            "--date=raw",
            REFLOG_FORMAT,
            "--end-of-options",
            refname,
            "--",
        ])?;
        // A ref without reflog makes git fail; that is an empty list.
        Ok(if out.success() {
            let mut entries = parse_reflog(&out.stdout);
            // `--date=raw` turns `%gD` into a date selector (`stash@{1700000000 +0200}`);
            // deleting needs the index form `stash@{n}`.
            for (i, e) in entries.iter_mut().enumerate() {
                if let Some(pos) = e.selector.find("@{") {
                    e.selector = format!("{}@{{{i}}}", &e.selector[..pos]);
                }
            }
            entries
        } else {
            Vec::new()
        })
    }
}

// ---------------------------------------------------------------------------
// References

/// Kind of a ref in Browse References.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefType {
    Branch,
    Remote,
    Tag,
    Other,
}

impl RefType {
    pub fn label(self) -> &'static str {
        match self {
            RefType::Branch => "Branch",
            RefType::Remote => "Remote branch",
            RefType::Tag => "Tag",
            RefType::Other => "Other",
        }
    }
}

/// A row of the Browse References list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefInfo {
    pub full_name: String,
    pub short_name: String,
    pub kind: RefType,
    pub id: String,
    pub subject: String,
    pub time: i64,
    /// UTC offset of `time` in seconds.
    pub offset: i32,
    pub author: String,
    pub upstream: String,
    /// The ref points directly at a commit (an annotated tag does not).
    pub is_commit: bool,
}

pub const FOR_EACH_REF_FORMAT: &str = "--format=%(refname)%1f%(objectname)%1f%(*objectname)%1f%(contents:subject)%1f%(creatordate:raw)%1f%(authorname)%(taggername)%1f%(upstream:short)%1e";

pub fn parse_for_each_ref(out: &str) -> Vec<RefInfo> {
    out.split(RS)
        .filter_map(|rec| {
            let rec = rec.trim_start_matches('\n');
            let f: Vec<&str> = rec.split(US).collect();
            if f.len() < 7 || f[0].is_empty() {
                return None;
            }
            let full = f[0].to_owned();
            let (kind, short) = if let Some(s) = full.strip_prefix("refs/heads/") {
                (RefType::Branch, s.to_owned())
            } else if let Some(s) = full.strip_prefix("refs/remotes/") {
                (RefType::Remote, s.to_owned())
            } else if let Some(s) = full.strip_prefix("refs/tags/") {
                (RefType::Tag, s.to_owned())
            } else {
                (RefType::Other, full.clone())
            };
            if kind == RefType::Remote && short.ends_with("/HEAD") {
                return None;
            }
            // Annotated tags: `*objectname` is the tagged commit.
            let id = if f[2].is_empty() { f[1] } else { f[2] };
            let (time, offset) = parse_raw_date(f[4]);
            Some(RefInfo {
                short_name: short,
                kind,
                id: id.to_owned(),
                subject: f[3].to_owned(),
                time,
                offset,
                author: f[5].to_owned(),
                upstream: f[6].trim().to_owned(),
                is_commit: f[2].is_empty(),
                full_name: full,
            })
        })
        .collect()
}

impl Repo {
    /// Every branch, remote-tracking branch and tag with its last commit.
    pub fn ref_infos(&self) -> Result<Vec<RefInfo>> {
        let out = self.git().run([
            "for-each-ref",
            FOR_EACH_REF_FORMAT,
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ])?;
        Ok(parse_for_each_ref(&out))
    }

    /// Files changed by `commit` relative to its first parent (or the empty
    /// tree for a root commit), with renames, as shown in the Log dialog.
    pub fn commit_changes(&self, commit: &str) -> Result<Vec<StatusEntry>> {
        let out = self.git().run([
            "diff-tree",
            "-r",
            "--root",
            "--name-status",
            "-M",
            "-z",
            "--no-commit-id",
            "-m",
            "--first-parent",
            commit,
        ])?;
        Ok(parse_name_status_z(&out))
    }

    /// `--numstat` of `commit` against its first parent.
    pub fn commit_numstat(&self, commit: &str) -> std::collections::HashMap<String, (u32, u32)> {
        self.git()
            .run([
                "diff-tree",
                "-r",
                "--root",
                "--numstat",
                "-M",
                "-z",
                "--no-commit-id",
                "-m",
                "--first-parent",
                commit,
            ])
            .map(|o| crate::commit::parse_numstat_z(&o))
            .unwrap_or_default()
    }
}

/// Parses `--name-status -z` output (`M\0path\0`, `R100\0old\0new\0`).
pub fn parse_name_status_z(out: &str) -> Vec<StatusEntry> {
    let mut fields = out.split('\0').filter(|f| !f.is_empty());
    let mut entries = Vec::new();
    while let Some(status) = fields.next() {
        let Some(code) = status.chars().next() else {
            continue;
        };
        let (path, old_path) = if matches!(code, 'R' | 'C') {
            let old = fields.next().unwrap_or_default().to_owned();
            (fields.next().unwrap_or_default().to_owned(), Some(old))
        } else {
            (fields.next().unwrap_or_default().to_owned(), None)
        };
        let action = match code {
            'A' => Action::Added,
            'D' => Action::Deleted,
            'R' => Action::Renamed,
            'C' => Action::Copied,
            'U' => Action::Unmerged,
            _ => Action::Modified,
        };
        if entries.iter().any(|e: &StatusEntry| e.path == path) {
            continue;
        }
        entries.push(StatusEntry {
            path,
            old_path,
            staged: Some(action),
            unstaged: None,
        });
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::{LogOptions, Signature};
    use crate::repo::tests::fixture;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn linear_history_stays_in_lane_zero() {
        let c = ids(&["b"]);
        let d = ids(&["a"]);
        let rows = graph([("c", c.as_slice()), ("b", d.as_slice()), ("a", &[][..])]);
        assert!(rows.iter().all(|r| r.node == 0 && r.width == 1));
        assert_eq!(
            rows[2].lines,
            [GraphLine {
                from: 0,
                to: 0,
                upper: true
            }]
        );
    }

    #[test]
    fn merge_opens_and_closes_a_lane() {
        // m = merge(c, s); c -> a; s -> a
        let m = ids(&["c", "s"]);
        let c = ids(&["a"]);
        let s = ids(&["a"]);
        let rows = graph([
            ("m", m.as_slice()),
            ("c", c.as_slice()),
            ("s", s.as_slice()),
            ("a", &[][..]),
        ]);
        assert_coherent(&rows);
        assert_eq!(rows[0].node, 0);
        assert!(rows[0].lines.contains(&GraphLine {
            from: 0,
            to: 1,
            upper: false
        }));
        assert_eq!(rows[1].node, 0);
        // s keeps its own lane down to "a" instead of jumping across.
        assert_eq!(rows[2].node, 1);
        assert!(rows[2].lines.contains(&GraphLine {
            from: 1,
            to: 1,
            upper: false
        }));
        // Both lanes converge into "a".
        assert_eq!(rows[3].node, 0);
        assert!(rows[3].lines.contains(&GraphLine {
            from: 1,
            to: 0,
            upper: true
        }));
        assert_eq!(rows[3].width, 2);
    }

    #[test]
    fn lanes_never_shift_sideways() {
        // Topo order m, f2, f1, c, a: the side branch reaches `a` first, yet
        // the first parent stays in the node's lane and both meet at `a`.
        let m = ids(&["c", "f2"]);
        let f2 = ids(&["f1"]);
        let f1 = ids(&["a"]);
        let c = ids(&["a"]);
        let rows = graph([
            ("m", m.as_slice()),
            ("f2", f2.as_slice()),
            ("f1", f1.as_slice()),
            ("c", c.as_slice()),
            ("a", &[][..]),
        ]);
        assert_coherent(&rows);
        assert_eq!(rows[2].node, 1);
        assert_eq!(rows[3].node, 0);
        assert_eq!(rows[4].node, 0);
        assert!(rows[4].lines.contains(&GraphLine {
            from: 1,
            to: 0,
            upper: true
        }));
    }

    #[test]
    fn all_branches_graph_is_coherent() {
        // Unmerged tips forking from main, a merge into a side branch, a merge
        // whose second parent another lane already waits for, and two roots.
        let list: &[(&str, &[&str])] = &[
            ("m5", &["m4"]),
            ("t", &["x2"]),
            ("m4", &["mf"]),
            ("mf", &["m3", "a3"]),
            ("a3", &["a2"]),
            ("a2", &["a1"]),
            ("a1", &["m1"]),
            ("m3", &["m2"]),
            ("c2", &["cm"]),
            ("cm", &["c1", "h1"]),
            ("h1", &["m2"]),
            ("c1", &["m2"]),
            ("b2", &["b1"]),
            ("j", &["b1", "m2"]),
            ("b1", &["m2"]),
            ("m2", &["m1"]),
            ("x2", &["x1"]),
            ("m1", &["init"]),
            ("init", &[]),
            ("x1", &[]),
        ];
        let parents: Vec<Vec<String>> = list.iter().map(|(_, p)| ids(p)).collect();
        let rows = graph(
            list.iter()
                .zip(&parents)
                .map(|((id, _), p)| (*id, p.as_slice())),
        );
        assert_coherent(&rows);
        // The unrelated tip keeps one lane all the way to its root.
        assert_eq!(rows[1].node, 1);
        assert_eq!(rows[16].node, 1);
        assert_eq!(rows[19].node, 1);
        // The lanes of m3, h1, c1 and b1 converge into m2; j's merge line
        // joined one of them rather than opening another.
        let into_m2 = rows[15].lines.iter().filter(|l| l.upper && l.to == 0);
        assert_eq!(into_m2.count(), 4);
    }

    /// Lines are continuous: every lane leaving a row's bottom enters the next
    /// row's top, and inside a row each line ends at the node or carries on.
    fn assert_coherent(rows: &[GraphRow]) {
        use std::collections::BTreeSet;
        let edges = |r: &GraphRow, upper: bool| -> BTreeSet<usize> {
            r.lines
                .iter()
                .filter(|l| l.upper == upper)
                .map(|l| if upper { l.from } else { l.to })
                .collect()
        };
        for (n, r) in rows.iter().enumerate() {
            let (top, bottom) = (edges(r, true), edges(r, false));
            for l in &r.lines {
                assert!(
                    l.from < r.width && l.to < r.width,
                    "row {n}: {l:?} too wide"
                );
                let inner = if l.upper { l.to } else { l.from };
                if inner != r.node {
                    assert_eq!(l.from, l.to, "row {n}: {l:?} shifts sideways");
                    let other = if l.upper { &bottom } else { &top };
                    assert!(other.contains(&l.from), "row {n}: {l:?} stops mid-row");
                }
            }
            if let Some(next) = rows.get(n + 1) {
                assert_eq!(bottom, edges(next, true), "rows {n} and {} differ", n + 1);
            }
        }
    }

    fn entry(subject: &str) -> LogEntry {
        let sig = Signature {
            name: "Ann".into(),
            email: "ann@x".into(),
            time: 0,
            offset: 0,
        };
        LogEntry {
            id: "abcdef1234".into(),
            parents: vec![],
            author: sig.clone(),
            committer: sig,
            subject: subject.into(),
            message: format!("{subject}\n\nbody words"),
            refs: vec![],
        }
    }

    #[test]
    fn topo_order_keeps_branches_together() {
        let mk = |id: &str, parents: &[&str]| LogEntry {
            id: id.into(),
            parents: ids(parents),
            ..entry(id)
        };
        // Date order interleaves the two branches: m, f2, c, f1, a.
        let interleaved = vec![
            mk("m", &["c", "f2"]),
            mk("f2", &["f1"]),
            mk("c", &["a"]),
            mk("f1", &["a"]),
            mk("a", &[]),
        ];
        let order: Vec<String> = crate::log::topo_order(interleaved)
            .into_iter()
            .map(|e| e.id)
            .collect();
        assert_eq!(order, ["m", "f2", "f1", "c", "a"]);
    }

    #[test]
    fn filter_fields_and_case() {
        let e = entry("Fix Crash");
        let mut f = LogFilter {
            text: "crash".into(),
            ..Default::default()
        };
        assert!(f.matches(&e));
        f.case_sensitive = true;
        assert!(!f.matches(&e));
        f.text = "ABCDEF".into();
        assert!(!f.matches(&e));
        f.case_sensitive = false;
        assert!(f.matches(&e));
        f.fields.sha1 = false;
        assert!(!f.matches(&e));
        f.text = "ann@".into();
        assert!(!f.matches(&e));
        f.fields.emails = true;
        assert!(f.matches(&e));
        f.text = "^fix c".into();
        assert!(!f.matches(&e));
        f.regex = true;
        assert!(f.matches(&e));
        f.text = "(".into();
        assert!(f.compile().is_err());
        assert!(!f.matches(&e));
        f.regex = false;
        f.text = "src/main".into();
        f.fields.paths = true;
        let paths = vec!["src/main.rs".to_owned()];
        assert!(f.matches_with(&e, Some(&paths), None));
        assert!(!f.matches(&e));
        f.text = "reviewed".into();
        f.fields.notes = true;
        assert!(f.matches_with(&e, None, Some("Reviewed-by: X")));
    }

    #[test]
    fn limits_and_ranges() {
        let now = 10 * DAY + 500;
        assert_eq!(parse_limit_param("0", now), LogLimit::None);
        assert_eq!(parse_limit_param("25", now), LogLimit::Commits(25));
        assert_eq!(
            parse_limit_param("2 week", now),
            LogLimit::Since(10 * DAY - 14 * DAY)
        );
        assert_eq!(limit_from_scale(0, 5, now), LogLimit::None);
        assert_eq!(
            range_param(Some("a..b"), Some("x"), None).as_deref(),
            Some("a..b")
        );
        assert_eq!(
            range_param(None, Some("v1"), Some("v2")).as_deref(),
            Some("v1..v2")
        );
        assert_eq!(
            range_param(None, Some(WORKING_TREE_REV), Some("HEAD")).as_deref(),
            Some("HEAD")
        );
        assert_eq!(range_param(None, None, None), None);
    }

    #[test]
    fn parsers() {
        let reflog = "aaa\u{1f}HEAD@{0}\u{1f}commit: second\u{1f}20\u{1e}\nbbb\u{1f}HEAD@{1}\u{1f}commit (initial): first\u{1f}10\u{1e}\n";
        let r = parse_reflog(reflog);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].action, "commit");
        assert_eq!(r[0].message, "second");
        assert_eq!(r[1].action, "commit (initial)");
        assert_eq!(r[1].time, 10);
        assert_eq!(parse_raw_date("1700000000 +0130"), (1_700_000_000, 5400));
        assert_eq!(parse_raw_date("5 -0200"), (5, -7200));

        let ns = parse_name_status_z("M\0a.txt\0R090\0old.txt\0new.txt\0A\0b\0");
        assert_eq!(ns.len(), 3);
        assert_eq!(ns[1].old_path.as_deref(), Some("old.txt"));
        assert_eq!(ns[1].action(), Action::Renamed);
        assert_eq!(ns[2].action(), Action::Added);
    }

    #[test]
    fn repository_queries() {
        let (dir, repo) = fixture();
        let git = repo.git();
        git.run(["tag", "-a", "v1", "-m", "release"]).unwrap();
        std::fs::rename(dir.path().join("a.txt"), dir.path().join("b.txt")).unwrap();
        git.run(["add", "-A"]).unwrap();
        git.run(["commit", "-qm", "rename"]).unwrap();
        git.run(["merge", "-q", "--no-ff", "-m", "noop", "HEAD"])
            .ok();

        let refs = repo.ref_infos().unwrap();
        let main = refs.iter().find(|r| r.short_name == "main").unwrap();
        assert_eq!(main.kind, RefType::Branch);
        assert_eq!(main.subject, "rename");
        let tag = refs.iter().find(|r| r.short_name == "v1").unwrap();
        assert_eq!(tag.kind, RefType::Tag);
        assert_eq!(tag.id, repo.log(&LogOptions::default()).unwrap()[1].id);

        let changes = repo.commit_changes("HEAD").unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, "b.txt");
        assert_eq!(changes[0].old_path.as_deref(), Some("a.txt"));
        let root = repo.commit_changes("HEAD~1").unwrap();
        assert_eq!(root[0].action(), Action::Added);

        let reflog = repo.reflog("HEAD").unwrap();
        assert_eq!(reflog[0].action, "commit");
        assert!(repo.reflog("refs/stash").unwrap().is_empty());

        let ranged = repo
            .log(&LogOptions {
                range: Some("HEAD~1..HEAD".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(ranged.len(), 1);
        assert_eq!(ranged[0].subject, "rename");
        let no_merges = repo
            .log(&LogOptions {
                no_merges: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(no_merges.len(), 2);
    }
}

#[cfg(test)]
mod reflog_selector_tests {
    use crate::repo::tests::fixture;

    #[test]
    fn stash_selectors_use_indices() {
        let (dir, repo) = fixture();
        for text in ["one\n", "two\n"] {
            std::fs::write(dir.path().join("a.txt"), text).unwrap();
            repo.git().run(["stash", "-q"]).unwrap();
        }
        let entries = repo.reflog("refs/stash").unwrap();
        let selectors: Vec<&str> = entries.iter().map(|e| e.selector.as_str()).collect();
        assert_eq!(selectors, ["refs/stash@{0}", "refs/stash@{1}"]);
        assert!(entries[0].time > 0);
    }
}
