//! `git blame --porcelain` for TortoiseGitBlame.

use std::collections::HashMap;

use crate::{Repo, Result};

/// Commit metadata shared by blamed lines.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlameCommit {
    pub id: String,
    pub author: String,
    pub author_mail: String,
    pub author_time: i64,
    /// Offset in seconds, parsed from `author-tz` (`+0130`).
    pub author_offset: i32,
    pub summary: String,
    /// Path of the file in that commit (differs after renames).
    pub filename: String,
    pub boundary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlameLine {
    pub commit: String,
    pub orig_line: usize,
    pub final_line: usize,
    pub content: String,
}

#[derive(Debug, Clone, Default)]
pub struct Blame {
    pub lines: Vec<BlameLine>,
    pub commits: HashMap<String, BlameCommit>,
}

fn parse_tz(tz: &str) -> i32 {
    let (sign, digits) = match tz.as_bytes().first() {
        Some(b'-') => (-1, &tz[1..]),
        Some(b'+') => (1, &tz[1..]),
        _ => (1, tz),
    };
    let v: i32 = digits.parse().unwrap_or(0);
    sign * ((v / 100) * 3600 + (v % 100) * 60)
}

/// Parses `git blame --porcelain` output.
pub fn parse_porcelain(out: &str) -> Blame {
    let mut blame = Blame::default();
    let mut current: Option<(String, usize, usize)> = None;
    for line in out.lines() {
        if let Some(content) = line.strip_prefix('\t') {
            if let Some((id, orig, fin)) = current.take() {
                blame.lines.push(BlameLine {
                    commit: id,
                    orig_line: orig,
                    final_line: fin,
                    content: content.to_owned(),
                });
            }
            continue;
        }
        let mut parts = line.splitn(2, ' ');
        let key = parts.next().unwrap_or_default();
        let value = parts.next().unwrap_or_default();
        if key.len() == 40 && key.bytes().all(|b| b.is_ascii_hexdigit()) {
            let mut nums = value.split(' ').filter_map(|n| n.parse::<usize>().ok());
            let orig = nums.next().unwrap_or(0);
            let fin = nums.next().unwrap_or(0);
            blame
                .commits
                .entry(key.to_owned())
                .or_insert_with(|| BlameCommit {
                    id: key.to_owned(),
                    ..Default::default()
                });
            current = Some((key.to_owned(), orig, fin));
            continue;
        }
        let Some((id, _, _)) = &current else { continue };
        let Some(c) = blame.commits.get_mut(id) else {
            continue;
        };
        match key {
            "author" => c.author = value.to_owned(),
            "author-mail" => c.author_mail = value.trim_matches(['<', '>']).to_owned(),
            "author-time" => c.author_time = value.parse().unwrap_or(0),
            "author-tz" => c.author_offset = parse_tz(value),
            "summary" => c.summary = value.to_owned(),
            "filename" => c.filename = value.to_owned(),
            "boundary" => c.boundary = true,
            _ => {}
        }
    }
    blame
}

/// Blame options matching the TortoiseGitBlame settings page.
#[derive(Debug, Clone, Default)]
pub struct BlameOptions {
    /// Revision to blame (`None` = working tree).
    pub rev: Option<String>,
    pub ignore_whitespace: bool,
    /// "Detect moved or copied lines": `-M` within a file.
    pub detect_moved: bool,
    /// `-C` between files.
    pub detect_copied: bool,
    pub first_parent: bool,
}

impl Repo {
    /// Runs `git blame --porcelain` on a repository-relative path.
    pub fn blame(&self, rel_path: &str, o: &BlameOptions) -> Result<Blame> {
        let mut args = vec!["blame".to_owned(), "--porcelain".to_owned()];
        if o.ignore_whitespace {
            args.push("-w".to_owned());
        }
        if o.detect_moved {
            args.push("-M".to_owned());
        }
        if o.detect_copied {
            args.push("-C".to_owned());
        }
        if o.first_parent {
            args.push("--first-parent".to_owned());
        }
        if let Some(r) = &o.rev {
            args.push(r.clone());
        }
        args.push("--".to_owned());
        args.push(rel_path.to_owned());
        Ok(parse_porcelain(&self.git().run(args)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::tests::fixture;

    #[test]
    fn parses_porcelain() {
        let sha = "a".repeat(40);
        let out = format!(
            "{sha} 1 1 2\nauthor Ann\nauthor-mail <ann@x>\nauthor-time 100\nauthor-tz -0130\nsummary first\nboundary\nfilename f.txt\n\tline one\n{sha} 2 2\n\tline two\n"
        );
        let b = parse_porcelain(&out);
        assert_eq!(b.lines.len(), 2);
        assert_eq!(b.lines[1].content, "line two");
        assert_eq!(b.lines[1].final_line, 2);
        let c = &b.commits[&sha];
        assert_eq!(
            (c.author.as_str(), c.author_mail.as_str()),
            ("Ann", "ann@x")
        );
        assert_eq!(c.author_offset, -5400);
        assert!(c.boundary);
        assert_eq!(c.summary, "first");
    }

    #[test]
    fn blames_real_file() {
        let (dir, repo) = fixture();
        std::fs::write(dir.path().join("a.txt"), "a\nb\n").unwrap();
        repo.git().run(["commit", "-qam", "second"]).unwrap();
        let b = repo.blame("a.txt", &BlameOptions::default()).unwrap();
        assert_eq!(b.lines.len(), 2);
        let summaries: Vec<&str> = b
            .lines
            .iter()
            .map(|l| b.commits[&l.commit].summary.as_str())
            .collect();
        assert_eq!(summaries, ["initial", "second"]);
    }
}

/// One parent a blamed commit can be compared with ("Blame previous revision").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviousFile {
    pub parent: String,
    /// The file's path in that parent (the old name after a rename).
    pub path: String,
}

impl Repo {
    /// The parents of `commit` in which `path` was modified or renamed
    /// (`TortoiseGitBlameView` "parentHashWithFile"); empty if the file was
    /// added by `commit`.
    pub fn previous_files(&self, commit: &str, path: &str) -> Result<Vec<PreviousFile>> {
        let parents = self
            .git()
            .run(["rev-list", "--parents", "-n", "1", commit])?;
        let mut out = Vec::new();
        for parent in parents.split_whitespace().skip(1) {
            let diff = self.git().run([
                "diff-tree",
                "-r",
                "-M",
                "--name-status",
                "-z",
                parent,
                commit,
                "--",
            ])?;
            let mut fields = diff.split('\0').filter(|f| !f.is_empty());
            while let Some(status) = fields.next() {
                let renamed = status.starts_with('R') || status.starts_with('C');
                let (old, new) = if renamed {
                    (
                        fields.next().unwrap_or_default(),
                        fields.next().unwrap_or_default(),
                    )
                } else {
                    let p = fields.next().unwrap_or_default();
                    (p, p)
                };
                if new == path && (status.starts_with('M') || status.starts_with('R')) {
                    out.push(PreviousFile {
                        parent: parent.to_owned(),
                        path: old.to_owned(),
                    });
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod previous_tests {
    use crate::repo::tests::fixture;

    #[test]
    fn previous_file_follows_renames() {
        let (dir, repo) = fixture();
        let g = repo.git();
        let added = g.run(["rev-parse", "HEAD"]).unwrap();
        assert!(
            repo.previous_files(added.trim(), "a.txt")
                .unwrap()
                .is_empty()
        );
        std::fs::write(dir.path().join("a.txt"), "a\nmore\n").unwrap();
        g.run(["commit", "-qam", "modify"]).unwrap();
        let p = repo.previous_files("HEAD", "a.txt").unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].path, "a.txt");
        g.run(["mv", "a.txt", "b.txt"]).unwrap();
        g.run(["commit", "-qm", "rename"]).unwrap();
        let p = repo.previous_files("HEAD", "b.txt").unwrap();
        assert_eq!(p[0].path, "a.txt");
    }
}
