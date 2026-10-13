//! Pure helpers of the Log dialog: date range fields, the jump bar, the
//! ref-label modes, Combine preconditions and the statistics page.

use std::collections::{BTreeMap, HashMap};

use koopagit_core::log::{LogEntry, RefKind};

/// Parses `YYYY-MM-DD` to the Unix time of 00:00 UTC that day.
pub fn parse_ymd(s: &str) -> Option<i64> {
    let mut it = s.trim().splitn(3, '-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let d: i64 = it.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || y < 1970 {
        return None;
    }
    // Howard Hinnant's days-from-civil.
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146_097 + doe - 719_468) * 86_400)
}

/// `YYYY-MM-DD` of a Unix time (UTC).
pub fn format_ymd(t: i64) -> String {
    crate::format::iso_datetime(t, 0)[..10].to_owned()
}

/// The From/To date fields as `(since, until)` Unix times; `To` includes its day.
pub fn date_range(from: &str, to: &str) -> (Option<i64>, Option<i64>) {
    (parse_ymd(from), parse_ymd(to).map(|t| t + 86_399))
}

/// What the log shows, chosen with the ref label menu / "All Branches".
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RefMode {
    /// HEAD, or the `/range` given on the command line.
    #[default]
    Head,
    All,
    BasicRefs,
    LocalBranches,
    /// An explicit revision or range (`FETCH_HEAD`, history entries).
    Range(String),
}

impl RefMode {
    /// Text of the ref label (`<All Branches>` etc.).
    pub fn label(&self, branch: Option<&str>) -> String {
        match self {
            RefMode::Head => branch.unwrap_or("<No branch>").to_owned(),
            RefMode::All => "<All Branches>".to_owned(),
            RefMode::BasicRefs => "<Basic Refs>".to_owned(),
            RefMode::LocalBranches => "<Local Branches>".to_owned(),
            RefMode::Range(r) => r.clone(),
        }
    }

    /// `rev-list` arguments replacing HEAD (`Git.cpp` `LOG_INFO_*`).
    pub fn revs(&self) -> Vec<String> {
        match self {
            RefMode::BasicRefs => [
                "--branches",
                "--tags",
                "--remotes",
                "--glob=stas[h]",
                "--glob=bisect",
            ]
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
            RefMode::LocalBranches => vec!["--branches".to_owned()],
            _ => Vec::new(),
        }
    }
}

/// Jump bar types (`LogDlg.cpp:420-431`), in combo order.
pub const JUMP_TYPES: [&str; 10] = [
    "Author Email",
    "Committer Email",
    "Merge Point",
    "Parent 1",
    "Parent 2",
    "Tag",
    "Tag (FF)",
    "Branch",
    "Branch (FF)",
    "Selection History",
];

/// Finds the row to jump to from `current` (rows are `entries` in list order;
/// "up" means towards row 0). Parent jumps go to that parent wherever it is;
/// "(FF)" variants only follow the first-parent chain.
pub fn jump(
    entries: &[&LogEntry],
    current: usize,
    kind: usize,
    up: bool,
    history: &[usize],
) -> Option<usize> {
    let cur = entries.get(current)?;
    let index_of = |id: &str| entries.iter().position(|e| e.id == id);
    let has = |e: &LogEntry, k: RefKind| e.refs.iter().any(|r| r.kind == k);
    let is_branch = |e: &LogEntry| {
        e.refs.iter().any(|r| {
            matches!(
                r.kind,
                RefKind::CurrentBranch | RefKind::LocalBranch | RefKind::RemoteBranch
            )
        })
    };
    let scan = |pred: &dyn Fn(&LogEntry) -> bool| -> Option<usize> {
        if up {
            (0..current).rev().find(|&i| pred(entries[i]))
        } else {
            (current + 1..entries.len()).find(|&i| pred(entries[i]))
        }
    };
    let ff_scan = |pred: &dyn Fn(&LogEntry) -> bool| -> Option<usize> {
        if up {
            // Walk children along first parents towards newer commits.
            let mut id = cur.id.clone();
            loop {
                let child = entries
                    .iter()
                    .position(|e| e.parents.first() == Some(&id))?;
                if pred(entries[child]) {
                    return Some(child);
                }
                id = entries[child].id.clone();
            }
        } else {
            let mut e = *cur;
            loop {
                let next = index_of(e.parents.first()?)?;
                if pred(entries[next]) {
                    return Some(next);
                }
                e = entries[next];
            }
        }
    };
    match kind {
        0 => scan(&|e| e.author.email == cur.author.email),
        1 => scan(&|e| e.committer.email == cur.committer.email),
        2 => scan(&|e| e.parents.len() > 1),
        3 => index_of(cur.parents.first()?),
        4 => index_of(cur.parents.get(1)?),
        5 => scan(&|e| has(e, RefKind::Tag)),
        6 => ff_scan(&|e| has(e, RefKind::Tag)),
        7 => scan(&is_branch),
        8 => ff_scan(&is_branch),
        9 => {
            // Back through the selection history (up) or forward (down).
            let pos = history.iter().rposition(|&r| r == current)?;
            if up {
                pos.checked_sub(1).map(|p| history[p])
            } else {
                history.get(pos + 1).copied()
            }
        }
        _ => None,
    }
}

/// Combine-to-one-commit precondition for the rows koopagit can combine:
/// the selection is HEAD, HEAD~1, ..., HEAD~n along first parents.
pub fn combine_ok(selected_ids: &[&str], parent_of: &HashMap<&str, &str>, head: &str) -> bool {
    if selected_ids.len() < 2 || selected_ids.first() != Some(&head) {
        return false;
    }
    selected_ids
        .windows(2)
        .all(|w| parent_of.get(w[0]).copied() == Some(w[1]))
}

/// Ctrl+V in the Log (`OnPasteGitHash` / `LookLikeGitHash`): the leading run
/// of lower-case hex digits of the trimmed text, when it is `min_len` (at
/// least 3) to 40 digits long and not followed by `@`.
pub fn pasted_hash(text: &str, min_len: usize) -> Option<&str> {
    let text = text.trim();
    let n = text
        .bytes()
        .take_while(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
        .count();
    let ok = n >= min_len.clamp(3, 40) && n <= 40 && !text[n..].starts_with('@');
    ok.then(|| &text[..n])
}

/// `JumpToGitHash`: the first row after `current` (wrapping around, `current`
/// itself excluded) whose id starts with `prefix`.
pub fn find_hash(ids: &[&str], current: usize, prefix: &str) -> Option<usize> {
    let n = ids.len();
    (1..n)
        .map(|k| (current + k) % n)
        .find(|&i| !ids[i].is_empty() && ids[i].starts_with(prefix))
}

/// The Statistics page (`CStatGraphDlg`, "Statistics" tab, simplified).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statistics {
    pub weeks: usize,
    pub authors: usize,
    pub commits: usize,
    pub files_changed: usize,
    pub most_active_author: String,
    pub most_commits_per_week: usize,
    pub least_commits_per_week: usize,
    pub avg_commits_per_week: usize,
    /// `(author, commits)`, most commits first.
    pub per_author: Vec<(String, usize)>,
    /// `(week start YYYY-MM-DD, commits)`, oldest first.
    pub per_week: Vec<(String, usize)>,
}

pub fn statistics(entries: &[&LogEntry], files_changed: usize) -> Statistics {
    const WEEK: i64 = 7 * 86_400;
    let mut authors: HashMap<&str, usize> = HashMap::new();
    let mut weeks: BTreeMap<i64, usize> = BTreeMap::new();
    for e in entries {
        *authors.entry(e.author.name.as_str()).or_default() += 1;
        // Weeks start on Monday (1970-01-01 was a Thursday).
        let t = e.author.time + 3 * 86_400;
        let start = t - t.rem_euclid(WEEK) - 3 * 86_400;
        *weeks.entry(start).or_default() += 1;
    }
    let mut per_author: Vec<(String, usize)> = authors
        .into_iter()
        .map(|(a, n)| (a.to_owned(), n))
        .collect();
    per_author.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let counts: Vec<usize> = weeks.values().copied().collect();
    let commits = entries.len();
    Statistics {
        weeks: weeks.len(),
        authors: per_author.len(),
        commits,
        files_changed,
        most_active_author: per_author.first().map(|a| a.0.clone()).unwrap_or_default(),
        most_commits_per_week: counts.iter().copied().max().unwrap_or(0),
        least_commits_per_week: counts.iter().copied().min().unwrap_or(0),
        avg_commits_per_week: if weeks.is_empty() {
            0
        } else {
            commits / weeks.len()
        },
        per_author,
        per_week: weeks.into_iter().map(|(t, n)| (format_ymd(t), n)).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use koopagit_core::log::{RefLabel, Signature};

    fn e(id: &str, parents: &[&str], email: &str, time: i64, refs: Vec<RefLabel>) -> LogEntry {
        let sig = Signature {
            name: email.split('@').next().unwrap().to_owned(),
            email: email.into(),
            time,
            offset: 0,
        };
        LogEntry {
            id: id.into(),
            parents: parents.iter().map(|p| (*p).to_owned()).collect(),
            author: sig.clone(),
            committer: sig,
            subject: id.into(),
            message: id.into(),
            refs,
        }
    }

    #[test]
    fn dates() {
        assert_eq!(parse_ymd("1970-01-02"), Some(86_400));
        assert_eq!(parse_ymd("2023-11-14"), Some(1_699_920_000));
        assert_eq!(parse_ymd("2023-13-01"), None);
        assert_eq!(parse_ymd("x"), None);
        assert_eq!(format_ymd(1_699_920_000), "2023-11-14");
        assert_eq!(date_range("1970-01-02", ""), (Some(86_400), None));
        assert_eq!(date_range("", "1970-01-01").1, Some(86_399));
    }

    #[test]
    fn jump_bar() {
        let tag = vec![RefLabel {
            kind: RefKind::Tag,
            name: "v1".into(),
        }];
        let rows = [
            e("m", &["c", "f"], "a@x", 40, vec![]),
            e("f", &["b"], "b@x", 30, vec![]),
            e("c", &["b"], "a@x", 20, tag.clone()),
            e("b", &["a"], "b@x", 10, vec![]),
            e("a", &[], "a@x", 0, tag),
        ];
        let refs: Vec<&LogEntry> = rows.iter().collect();
        assert_eq!(
            jump(&refs, 0, 0, false, &[]),
            Some(2),
            "next by same author"
        );
        assert_eq!(jump(&refs, 2, 0, true, &[]), Some(0));
        assert_eq!(jump(&refs, 3, 2, true, &[]), Some(0), "merge point");
        assert_eq!(jump(&refs, 0, 3, false, &[]), Some(2), "parent 1");
        assert_eq!(jump(&refs, 0, 4, false, &[]), Some(1), "parent 2");
        assert_eq!(jump(&refs, 1, 5, false, &[]), Some(2), "next tag");
        assert_eq!(
            jump(&refs, 1, 6, false, &[]),
            Some(4),
            "tag along first parents"
        );
        assert_eq!(
            jump(&refs, 4, 6, true, &[]),
            None,
            "a's first child chain: b, then c or f"
        );
        assert_eq!(jump(&refs, 3, 9, true, &[0, 3]), Some(0));
        assert_eq!(jump(&refs, 0, 9, false, &[0, 3]), Some(3));
    }

    #[test]
    fn combine_and_modes() {
        let parents: HashMap<&str, &str> = [("h", "h1"), ("h1", "h2"), ("x", "h1")].into();
        assert!(combine_ok(&["h", "h1", "h2"], &parents, "h"));
        assert!(!combine_ok(&["h1", "h2"], &parents, "h"));
        assert!(!combine_ok(&["h", "h2"], &parents, "h"));
        assert!(!combine_ok(&["h"], &parents, "h"));
        assert_eq!(RefMode::All.label(Some("main")), "<All Branches>");
        assert_eq!(RefMode::Head.label(None), "<No branch>");
        assert_eq!(RefMode::LocalBranches.revs(), ["--branches"]);
        assert!(RefMode::BasicRefs.revs().contains(&"--remotes".to_owned()));
    }

    #[test]
    fn stats() {
        let day = 86_400;
        let rows = [
            e("a", &[], "ann@x", 4 * day, vec![]), // Mon 1970-01-05
            e("b", &[], "ann@x", 5 * day, vec![]),
            e("c", &[], "bob@x", 12 * day, vec![]), // next week
        ];
        let refs: Vec<&LogEntry> = rows.iter().collect();
        let s = statistics(&refs, 7);
        assert_eq!((s.weeks, s.authors, s.commits), (2, 2, 3));
        assert_eq!(s.most_active_author, "ann");
        assert_eq!((s.most_commits_per_week, s.least_commits_per_week), (2, 1));
        assert_eq!(s.per_week[0], ("1970-01-05".to_owned(), 2));
        assert_eq!(s.per_author[0], ("ann".to_owned(), 2));
    }

    #[test]
    fn pasted_hashes() {
        assert_eq!(pasted_hash("  1a2b3c4d\n", 7), Some("1a2b3c4d"));
        assert_eq!(pasted_hash("1a2b3c4 fix", 7), Some("1a2b3c4"));
        assert_eq!(pasted_hash("1a2b3c", 7), None);
        assert_eq!(pasted_hash("1A2B3C4D", 7), None);
        assert_eq!(pasted_hash("deadbeef@example", 7), None);
        assert_eq!(pasted_hash(&"a".repeat(41), 7), None);
        assert_eq!(pasted_hash("abc", 1), Some("abc"));
        let ids = ["", "abc1", "def2", "abc3"];
        assert_eq!(find_hash(&ids, 1, "abc"), Some(3));
        assert_eq!(find_hash(&ids, 3, "abc"), Some(1));
        assert_eq!(find_hash(&ids, 2, "def"), None);
        assert_eq!(find_hash(&[], 0, "abc"), None);
    }
}
