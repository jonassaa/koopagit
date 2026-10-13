//! Commit dialog operations (`CCommitDlg`): index preparation without staging
//! support, the `git commit` command line and Signed-off-by handling.

use std::collections::HashMap;

use crate::status::{Action, StatusEntry};
use crate::{Repo, Result};

/// A row of the commit dialog's file list.
#[derive(Debug, Clone)]
pub struct CommitRow {
    pub entry: StatusEntry,
    pub checked: bool,
    pub is_dir: bool,
}

/// Git command lines to run around `git commit`
/// (`PrepareIndexForCommitWithoutStagingSupport`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndexPlan {
    /// Run before the commit, in TortoiseGit's task order: add,
    /// update-index --force-remove, update-index, rm, rm --cached, reset.
    pub before: Vec<Vec<String>>,
    /// Run after a successful commit to restore unselected added/renamed files
    /// (`ReaddUnselectedAddedFilesAfterCommit`).
    pub after: Vec<Vec<String>>,
}

fn s(v: &str) -> String {
    v.to_owned()
}

fn task(prefix: &[&str], paths: &[String]) -> Option<Vec<String>> {
    if paths.is_empty() {
        return None;
    }
    let mut v: Vec<String> = prefix.iter().map(|a| s(a)).collect();
    v.push(s("--"));
    v.extend(paths.iter().cloned());
    Some(v)
}

/// Builds the index preparation for the checked rows. `has_head` is false in a
/// fresh repository; `amend_base` uses `HEAD~1` for resets (amend without
/// "Show diff to last commit").
pub fn plan_index(rows: &[CommitRow], has_head: bool, amend_base: bool) -> IndexPlan {
    let mut add = Vec::new();
    let mut force_remove = Vec::new();
    let mut update = Vec::new();
    let mut rm = Vec::new();
    let mut rm_cached = Vec::new();
    let mut reset = Vec::new();
    let mut readd = Vec::new();
    let mut redelete = Vec::new();
    for row in rows {
        let e = &row.entry;
        let action = e.action();
        if row.checked {
            match action {
                Action::Unversioned | Action::Ignored => add.push(e.path.clone()),
                _ if row.is_dir && action != Action::Deleted => add.push(e.path.clone()),
                Action::Deleted | Action::Missing => force_remove.push(e.path.clone()),
                _ => update.push(e.path.clone()),
            }
            if let Some(old) = &e.old_path {
                rm.push(old.clone());
            }
        } else {
            match action {
                Action::Unversioned | Action::Ignored => {}
                Action::Added | Action::Renamed | Action::Copied => {
                    rm_cached.push(e.path.clone());
                    readd.push(e.path.clone());
                    if let Some(old) = &e.old_path {
                        if has_head {
                            reset.push(old.clone());
                        }
                        redelete.push(old.clone());
                    }
                }
                _ if !has_head => rm_cached.push(e.path.clone()),
                _ => {
                    reset.push(e.path.clone());
                    if e.staged == Some(Action::Deleted) && e.unstaged.is_some() {
                        redelete.push(e.path.clone());
                    }
                }
            }
        }
    }
    let base = if amend_base { "HEAD~1" } else { "HEAD" };
    let before = [
        task(&["add", "-f"], &add),
        task(&["update-index", "--force-remove"], &force_remove),
        task(&["update-index"], &update),
        task(&["rm", "--ignore-unmatch"], &rm),
        task(&["rm", "-f", "--cached"], &rm_cached),
        task(&["reset", "-q", base], &reset),
    ]
    .into_iter()
    .flatten()
    .collect();
    let after = [
        task(&["add", "--ignore-errors", "-f"], &readd),
        task(&["rm", "--cached", "--ignore-unmatch", "-q"], &redelete),
    ]
    .into_iter()
    .flatten()
    .collect();
    IndexPlan { before, after }
}

/// Author date option of the dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorDate {
    /// "Set author date": `YYYY-MM-DDTHH:MM:SS`.
    At(String),
    /// Amend + "Reset": `--date=now`.
    Now,
}

/// The commit dialog's options.
#[derive(Debug, Clone, Default)]
pub struct CommitOptions {
    pub amend: bool,
    /// "Set author": `Name <email>`.
    pub author: Option<String>,
    pub date: Option<AuthorDate>,
    /// "Message only": commit with `--allow-empty`.
    pub message_only: bool,
}

/// `git commit [--author=..] [--date=..] [--amend] [--allow-empty] -F <file>`.
pub fn commit_args(o: &CommitOptions, message_file: &str) -> Vec<String> {
    let mut a = vec![s("commit")];
    if let Some(author) = &o.author {
        a.push(format!("--author={author}"));
    }
    match &o.date {
        Some(AuthorDate::At(d)) => a.push(format!("--date={d}")),
        Some(AuthorDate::Now) => a.push(s("--date=now")),
        None => {}
    }
    if o.amend {
        a.push(s("--amend"));
    }
    if o.message_only {
        a.push(s("--allow-empty"));
    }
    a.extend([s("-F"), s(message_file)]);
    a
}

/// "Add Signed-off-by": appends the trailer unless already present.
pub fn add_sign_off(message: &str, name: &str, email: &str) -> String {
    let clean = |v: &str| v.replace(['\r', '\n'], "");
    let line = format!("Signed-off-by: {} <{}>", clean(name), clean(email));
    let trimmed = message.trim_end();
    if trimmed.lines().any(|l| l.trim_end() == line) {
        return message.to_owned();
    }
    let mut out = trimmed.to_owned();
    if !out.is_empty() {
        let last_is_trailer = out.lines().last().is_some_and(|l| l.contains("-by: "));
        out.push('\n');
        if !last_is_trailer {
            out.push('\n');
        }
    }
    out.push_str(&line);
    out.push('\n');
    out
}

/// Whether the message has a Signed-off-by line for this identity.
pub fn has_sign_off(message: &str, name: &str, email: &str) -> bool {
    let line = format!("Signed-off-by: {name} <{email}>");
    message.lines().any(|l| l.trim_end() == line)
}

/// `StripCommentedLines`: drops lines starting with `#`.
pub fn strip_comments(message: &str) -> String {
    message
        .lines()
        .filter(|l| !l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether the OK button is enabled (`CCommitDlg::UpdateOKButton`).
pub fn can_commit(
    message: &str,
    min_size: usize,
    message_only: bool,
    any_checked: bool,
    amend_diff_last: bool,
    merge_active: bool,
) -> bool {
    let m = message.trim();
    !m.is_empty()
        && m.chars().count() >= min_size
        && (message_only || any_checked || amend_diff_last || merge_active)
}

impl Repo {
    /// `user.name` / `user.email` as git resolves them (`git var`-like via config).
    pub fn user_identity(&self) -> (String, String) {
        let git = self.git();
        let get = |k: &str| {
            git.run(["config", "--get", k])
                .map(|v| v.trim().to_owned())
                .unwrap_or_default()
        };
        (get("user.name"), get("user.email"))
    }

    /// Whether a merge is in progress (`MERGE_HEAD` exists).
    pub fn merge_active(&self) -> bool {
        self.git_dir().join("MERGE_HEAD").exists()
    }

    /// The initial commit message: `commit.template`, then `SQUASH_MSG`, then
    /// `MERGE_MSG` (each later one wins).
    pub fn initial_commit_message(&self) -> String {
        let mut msg = String::new();
        if let Ok(t) = self
            .git()
            .run(["config", "--path", "--get", "commit.template"])
        {
            if let Ok(text) = std::fs::read_to_string(t.trim()) {
                msg = text;
            }
        }
        for f in ["SQUASH_MSG", "MERGE_MSG"] {
            if let Ok(text) = std::fs::read_to_string(self.git_dir().join(f)) {
                msg = text;
            }
        }
        msg
    }

    /// Subject and body of HEAD (amend loads this into the editor).
    pub fn head_message(&self) -> Result<String> {
        Ok(self
            .git()
            .run(["log", "-1", "--format=%B", "HEAD"])?
            .trim_end()
            .to_owned()
            + "\n")
    }

    /// Lines added/removed per path relative to `base` (`--numstat`), for the
    /// "Lines added" / "Lines removed" columns.
    pub fn numstat(&self, base: &str) -> HashMap<String, (u32, u32)> {
        let Ok(out) = self.git().run(["diff", "--numstat", "-z", base, "--"]) else {
            return HashMap::new();
        };
        parse_numstat_z(&out)
    }
}

pub(crate) fn parse_numstat_z(out: &str) -> HashMap<String, (u32, u32)> {
    let mut map = HashMap::new();
    let mut fields = out.split('\0');
    while let Some(rec) = fields.next() {
        let mut parts = rec.splitn(3, '\t');
        let (Some(a), Some(d), Some(path)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let path = if path.is_empty() {
            // Rename: "add\tdel\t\0old\0new".
            let _old = fields.next();
            fields.next().unwrap_or_default().to_owned()
        } else {
            path.to_owned()
        };
        map.insert(path, (a.parse().unwrap_or(0), d.parse().unwrap_or(0)));
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::tests::fixture;
    use crate::status::StatusOptions;

    fn row(
        path: &str,
        staged: Option<Action>,
        unstaged: Option<Action>,
        checked: bool,
    ) -> CommitRow {
        CommitRow {
            entry: StatusEntry {
                path: path.into(),
                old_path: None,
                staged,
                unstaged,
            },
            checked,
            is_dir: false,
        }
    }

    #[test]
    fn plan_orders_tasks_like_tortoisegit() {
        let rows = [
            row("new.txt", None, Some(Action::Unversioned), true),
            row("gone.txt", None, Some(Action::Missing), true),
            row("mod.txt", None, Some(Action::Modified), true),
            row("skip.txt", None, Some(Action::Modified), false),
            row("added.txt", Some(Action::Added), None, false),
            row("untracked.txt", None, Some(Action::Unversioned), false),
        ];
        let plan = plan_index(&rows, true, false);
        let lines: Vec<String> = plan.before.iter().map(|c| c.join(" ")).collect();
        assert_eq!(
            lines,
            [
                "add -f -- new.txt",
                "update-index --force-remove -- gone.txt",
                "update-index -- mod.txt",
                "rm -f --cached -- added.txt",
                "reset -q HEAD -- skip.txt",
            ]
        );
        assert_eq!(
            plan.after[0].join(" "),
            "add --ignore-errors -f -- added.txt"
        );
    }

    #[test]
    fn commit_line_and_sign_off() {
        let o = CommitOptions {
            amend: true,
            author: Some("A <a@b>".into()),
            date: Some(AuthorDate::Now),
            message_only: true,
        };
        assert_eq!(
            commit_args(&o, "/tmp/m").join(" "),
            "commit --author=A <a@b> --date=now --amend --allow-empty -F /tmp/m"
        );
        let m = add_sign_off("Fix bug\n\n", "Ann", "a@x");
        assert_eq!(m, "Fix bug\n\nSigned-off-by: Ann <a@x>\n");
        assert_eq!(add_sign_off(&m, "Ann", "a@x"), m);
        let two = add_sign_off(&m, "Bob", "b@x");
        assert!(two.ends_with("Signed-off-by: Ann <a@x>\nSigned-off-by: Bob <b@x>\n"));
        assert!(has_sign_off(&two, "Bob", "b@x"));
        assert!(can_commit("msg", 0, false, true, false, false));
        assert!(!can_commit("  ", 0, true, true, false, false));
        assert!(!can_commit("ab", 3, false, true, false, false));
        assert_eq!(strip_comments("a\n# c\nb"), "a\nb");
    }

    #[test]
    fn end_to_end_commit_of_selected_files() {
        let (dir, repo) = fixture();
        std::fs::write(dir.path().join("a.txt"), "changed\n").unwrap();
        std::fs::write(dir.path().join("b.txt"), "b\n").unwrap();
        std::fs::write(dir.path().join("c.txt"), "c\n").unwrap();
        let entries = repo.status(StatusOptions::with_unversioned(), &[]).unwrap();
        let rows: Vec<CommitRow> = entries
            .into_iter()
            .map(|e| CommitRow {
                checked: e.path != "c.txt",
                entry: e,
                is_dir: false,
            })
            .collect();
        let plan = plan_index(&rows, true, false);
        let git = repo.git();
        for c in &plan.before {
            git.run(c).unwrap();
        }
        let msg = dir.path().join("msg");
        std::fs::write(&msg, "second\n").unwrap();
        git.run(commit_args(
            &CommitOptions::default(),
            &msg.to_string_lossy(),
        ))
        .unwrap();
        let files = git
            .run(["show", "--name-only", "--format=", "HEAD"])
            .unwrap();
        assert_eq!(files.lines().collect::<Vec<_>>(), ["a.txt", "b.txt"]);
        let left = repo.status(StatusOptions::with_unversioned(), &[]).unwrap();
        assert_eq!(left.len(), 2, "{left:?}"); // c.txt and the msg file stay unversioned
        assert_eq!(repo.numstat("HEAD~1").get("a.txt"), Some(&(1, 1)));
    }
}
