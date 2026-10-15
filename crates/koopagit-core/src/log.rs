//! Commit history for the Log dialog (TortoiseGit's `CGitLogListBase`).

use std::collections::{HashMap, HashSet};

use gix::bstr::ByteSlice;

use crate::{Error, Repo, Result};

/// A person and timestamp from a commit header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub name: String,
    pub email: String,
    /// Seconds since the Unix epoch (UTC).
    pub time: i64,
    /// Offset from UTC in seconds.
    pub offset: i32,
}

/// One row of the Log dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub id: String,
    pub parents: Vec<String>,
    pub author: Signature,
    pub committer: Signature,
    /// First line of the message (the "Message" column).
    pub subject: String,
    /// The full message.
    pub message: String,
    /// Ref labels pointing at this commit, e.g. `main`, `origin/main`, `v1.0`.
    pub refs: Vec<RefLabel>,
}

impl LogEntry {
    /// The abbreviated hash shown in the "Hash" column.
    pub fn short_id(&self) -> &str {
        &self.id[..self.id.len().min(8)]
    }
}

/// The kind of a ref label drawn next to a commit in the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RefKind {
    /// The checked-out branch (drawn highlighted).
    CurrentBranch,
    LocalBranch,
    RemoteBranch,
    Tag,
    Stash,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RefLabel {
    pub kind: RefKind,
    pub name: String,
}

/// Mirrors the Log dialog's "All Branches" / path filter / count controls.
#[derive(Debug, Clone, Default)]
pub struct LogOptions {
    /// Start from every branch and tag instead of HEAD ("All Branches").
    pub all_branches: bool,
    /// Follow only the first parent ("Show only first parent").
    pub first_parent: bool,
    /// Only commits touching these repository-relative paths.
    pub paths: Vec<String>,
    /// Stop after this many commits ("Show only N" / default `NumberOfLogs`).
    pub limit: Option<usize>,
    /// Walk Behavior "No merges" (`--no-merges`).
    pub no_merges: bool,
    /// Explicit revision range (`/range`, `/startrev..endrev`); ignored with
    /// `all_branches`, like upstream.
    pub range: Option<String>,
    /// Only commits newer than this Unix time (`--max-age`, date limits).
    pub since: Option<i64>,
    /// Only commits older than this Unix time (`--min-age`, the "To" date).
    pub until: Option<i64>,
    /// Revision arguments replacing HEAD/range, e.g. `--branches --tags
    /// --remotes --glob=stash` ("All basic refs") or `--branches` ("All local
    /// branches"). Ignored with `all_branches`.
    pub revs: Vec<String>,
}

impl Repo {
    /// Commit history for the Log dialog. The plain case walks with gix; ranges,
    /// path limits and "No merges" ask `git rev-list --topo-order` for the ids
    /// (it implements git's history simplification) and read details with gix.
    pub fn log(&self, options: &LogOptions) -> Result<Vec<LogEntry>> {
        if options.paths.is_empty()
            && !options.no_merges
            && options.range.is_none()
            && options.revs.is_empty()
            && options.until.is_none()
        {
            // Walk everything, order topologically (TortoiseGit's default
            // `--topo-order`), then apply the limits.
            let walk = LogOptions {
                limit: None,
                ..options.clone()
            };
            let mut entries = topo_order(self.log_walk(&walk)?);
            if let Some(since) = options.since {
                entries.retain(|e| e.committer.time >= since);
            }
            if let Some(n) = options.limit {
                entries.truncate(n);
            }
            return Ok(entries);
        }
        let mut args = vec!["rev-list".to_owned(), "--topo-order".to_owned()];
        if options.all_branches {
            args.push("--all".to_owned());
        }
        if options.first_parent {
            args.push("--first-parent".to_owned());
        }
        if options.no_merges {
            args.push("--no-merges".to_owned());
        }
        if let Some(since) = options.since {
            args.push(format!("--max-age={since}"));
        }
        if let Some(until) = options.until {
            args.push(format!("--min-age={until}"));
        }
        if let Some(n) = options.limit {
            args.push(format!("-n{n}"));
        }
        if !options.all_branches && !options.revs.is_empty() {
            args.extend(options.revs.iter().cloned());
        } else if !options.all_branches {
            if self.head_id()?.is_none() && options.range.is_none() {
                return Ok(Vec::new());
            }
            args.push("--end-of-options".to_owned());
            args.push(options.range.clone().unwrap_or_else(|| "HEAD".to_owned()));
        }
        args.push("--".to_owned());
        args.extend(options.paths.iter().cloned());
        let out = self.git().run(args)?;
        let labels = self.ref_labels()?;
        let repo = self.gix();
        let mut entries = Vec::new();
        for id in out.lines().filter(|l| !l.is_empty()) {
            let oid = gix::ObjectId::from_hex(id.as_bytes()).map_err(Error::gix)?;
            let commit = repo.find_commit(oid).map_err(Error::gix)?;
            let decoded = commit.decode().map_err(Error::gix)?;
            entries.push(LogEntry {
                parents: decoded.parents().map(|p| p.to_string()).collect(),
                author: signature(decoded.author().map_err(Error::gix)?)?,
                committer: signature(decoded.committer().map_err(Error::gix)?)?,
                subject: decoded.message_summary().to_str_lossy().into_owned(),
                message: decoded.message.to_str_lossy().into_owned(),
                refs: labels.get(id).cloned().unwrap_or_default(),
                id: id.to_owned(),
            });
        }
        Ok(entries)
    }
}

impl Repo {
    /// Ref labels keyed by the full id of the commit they point at.
    pub fn ref_labels(&self) -> Result<HashMap<String, Vec<RefLabel>>> {
        let current = self.current_branch()?;
        let mut map: HashMap<String, Vec<RefLabel>> = HashMap::new();
        let platform = self.gix().references().map_err(Error::gix)?;
        for r in platform.all().map_err(Error::gix)?.flatten() {
            let full = r.name().as_bstr().to_string();
            let mut r = r;
            let Ok(id) = r.peel_to_id() else { continue };
            let (kind, name) = if let Some(n) = full.strip_prefix("refs/heads/") {
                let kind = if current.as_deref() == Some(n) {
                    RefKind::CurrentBranch
                } else {
                    RefKind::LocalBranch
                };
                (kind, n.to_owned())
            } else if let Some(n) = full.strip_prefix("refs/remotes/") {
                if n.ends_with("/HEAD") {
                    continue;
                }
                (RefKind::RemoteBranch, n.to_owned())
            } else if let Some(n) = full.strip_prefix("refs/tags/") {
                (RefKind::Tag, n.to_owned())
            } else if full == "refs/stash" {
                (RefKind::Stash, "stash".to_owned())
            } else {
                (RefKind::Other, full.clone())
            };
            map.entry(id.to_string())
                .or_default()
                .push(RefLabel { kind, name });
        }
        Ok(map)
    }

    /// Walks history newest-first (by commit time), like `git log`.
    fn log_walk(&self, options: &LogOptions) -> Result<Vec<LogEntry>> {
        let repo = self.gix();
        let mut tips: Vec<gix::ObjectId> = Vec::new();
        if options.all_branches {
            let platform = repo.references().map_err(Error::gix)?;
            for prefix in ["refs/heads/", "refs/remotes/", "refs/tags/"] {
                for r in platform.prefixed(prefix).map_err(Error::gix)?.flatten() {
                    let mut r = r;
                    if let Ok(id) = r.peel_to_id()
                        && repo.find_commit(id).is_ok()
                    {
                        tips.push(id.detach());
                    }
                }
            }
        }
        if let Some(head) = self.head_id()? {
            tips.push(gix::ObjectId::from_hex(head.as_bytes()).map_err(Error::gix)?);
        }
        tips.sort();
        tips.dedup();
        if tips.is_empty() {
            return Ok(Vec::new());
        }

        // Path-limited history needs tree diffs with git's history simplification;
        // ask git for the matching ids and fill in details from gix.
        let path_filter: Option<HashSet<String>> = if options.paths.is_empty() {
            None
        } else {
            let mut args = vec!["log".to_owned(), "--format=%H".to_owned()];
            if options.all_branches {
                args.push("--all".to_owned());
            }
            if options.first_parent {
                args.push("--first-parent".to_owned());
            }
            args.push("--".to_owned());
            args.extend(options.paths.iter().cloned());
            let out = self.git().run(args)?;
            Some(out.lines().map(str::to_owned).collect())
        };

        let labels = self.ref_labels()?;
        let mut walk = repo
            .rev_walk(tips)
            .sorting(gix::revision::walk::Sorting::ByCommitTime(
                gix::traverse::commit::simple::CommitTimeOrder::NewestFirst,
            ));
        if options.first_parent {
            walk = walk.first_parent_only();
        }
        let mut out = Vec::new();
        for info in walk.all().map_err(Error::gix)? {
            let info = info.map_err(Error::gix)?;
            let id = info.id.to_string();
            if let Some(filter) = &path_filter
                && !filter.contains(&id)
            {
                continue;
            }
            let commit = info.object().map_err(Error::gix)?;
            let decoded = commit.decode().map_err(Error::gix)?;
            let message = decoded.message.to_str_lossy().into_owned();
            let subject = decoded.message_summary().to_str_lossy().into_owned();
            out.push(LogEntry {
                parents: info.parent_ids.iter().map(|p| p.to_string()).collect(),
                author: signature(decoded.author().map_err(Error::gix)?)?,
                committer: signature(decoded.committer().map_err(Error::gix)?)?,
                subject,
                message,
                refs: labels.get(&id).cloned().unwrap_or_default(),
                id,
            });
            if options.limit.is_some_and(|l| out.len() >= l) {
                break;
            }
        }
        Ok(out)
    }
}

/// Reorders newest-first entries like `git log --topo-order`: no parent before
/// all of its children, and each line of history kept together (a LIFO of
/// ready commits, so a merge's side branch follows the merge).
pub fn topo_order(entries: Vec<LogEntry>) -> Vec<LogEntry> {
    let index: HashMap<&str, usize> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| (e.id.as_str(), i))
        .collect();
    let mut children = vec![0usize; entries.len()];
    for e in &entries {
        for p in &e.parents {
            if let Some(&i) = index.get(p.as_str()) {
                children[i] += 1;
            }
        }
    }
    // Tips in reverse date order so the newest ends up on top of the stack.
    let mut stack: Vec<usize> = (0..entries.len())
        .rev()
        .filter(|&i| children[i] == 0)
        .collect();
    let mut order = Vec::with_capacity(entries.len());
    while let Some(i) = stack.pop() {
        order.push(i);
        for p in &entries[i].parents {
            if let Some(&pi) = index.get(p.as_str()) {
                children[pi] -= 1;
                if children[pi] == 0 {
                    stack.push(pi);
                }
            }
        }
    }
    let mut slots: Vec<Option<LogEntry>> = entries.into_iter().map(Some).collect();
    order.into_iter().filter_map(|i| slots[i].take()).collect()
}

fn signature(sig: gix::actor::SignatureRef<'_>) -> Result<Signature> {
    let time = sig.time().map_err(Error::gix)?;
    Ok(Signature {
        name: sig.name.to_str_lossy().into_owned(),
        email: sig.email.to_str_lossy().into_owned(),
        time: time.seconds,
        offset: time.offset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::tests::fixture;

    #[test]
    fn walks_history_with_refs_and_filters() {
        let (dir, repo) = fixture();
        let git = repo.git();
        std::fs::write(dir.path().join("b.txt"), "b\n").unwrap();
        git.run(["add", "b.txt"]).unwrap();
        git.run(["commit", "-qm", "add b\n\nbody text"]).unwrap();
        git.run(["tag", "v1"]).unwrap();
        git.run(["checkout", "-qb", "side", "HEAD~1"]).unwrap();
        std::fs::write(dir.path().join("a.txt"), "side\n").unwrap();
        git.run(["commit", "-qam", "side change"]).unwrap();
        git.run(["checkout", "-q", "main"]).unwrap();

        let repo = crate::Repo::discover(dir.path()).unwrap();
        let log = repo.log(&LogOptions::default()).unwrap();
        assert_eq!(
            log.iter().map(|e| e.subject.as_str()).collect::<Vec<_>>(),
            ["add b", "initial"]
        );
        assert_eq!(log[0].message.trim_end(), "add b\n\nbody text");
        assert_eq!(log[0].author.name, "Test");
        assert_eq!(log[0].short_id().len(), 8);
        assert!(log[0].refs.contains(&RefLabel {
            kind: RefKind::CurrentBranch,
            name: "main".into()
        }));
        assert!(log[0].refs.contains(&RefLabel {
            kind: RefKind::Tag,
            name: "v1".into()
        }));

        let all = repo
            .log(&LogOptions {
                all_branches: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(all.len(), 3);

        let only_b = repo
            .log(&LogOptions {
                all_branches: true,
                paths: vec!["b.txt".into()],
                ..Default::default()
            })
            .unwrap();
        assert_eq!(only_b.len(), 1);
        assert_eq!(only_b[0].subject, "add b");

        let limited = repo
            .log(&LogOptions {
                all_branches: true,
                limit: Some(1),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(limited.len(), 1);
    }
}
