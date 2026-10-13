//! TortoiseGit's rebase / cherry-pick engine (`CRebaseDlg::DoRebase`).
//!
//! TortoiseGit does not use `git rebase`: it detaches HEAD at the target
//! (rebase mode), replays each kept commit with `git cherry-pick`, folds
//! squashes into the previous commit with `commit --amend`, stops for Edit
//! steps and conflicts, and finally moves the branch to the result. Cherry-pick
//! mode replays the selected commits on top of the current HEAD instead.
//!
//! [`Sequencer`] holds the state between stops; the Rebase dialog drives it.

use std::collections::HashMap;
use std::path::PathBuf;

pub use crate::fileops::RebaseAction;
use crate::{Error, Repo, Result};

/// Marker TortoiseGit writes while its rebase runs (`tgitrebase.active`).
pub const ACTIVE_MARKER: &str = "tgitrebase.active";

/// Progress of one row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepState {
    Pending,
    Current,
    Done,
}

/// One commit to replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub id: String,
    pub subject: String,
    pub author: String,
    pub parents: Vec<String>,
    pub action: RebaseAction,
    pub state: StepState,
}

/// What to replay and where.
#[derive(Debug, Clone, Default)]
pub struct Plan {
    /// Local branch moved to the result at the end (rebase mode). `None` in
    /// cherry-pick mode, where HEAD advances directly.
    pub branch: Option<String>,
    /// Commit the replay starts from (upstream, or "Onto"). Ignored in cherry-pick mode.
    pub onto: String,
    /// Oldest first.
    pub steps: Vec<Step>,
    pub cherry_pick: bool,
    /// `add "cherry picked from"`: pass `-x`.
    pub add_cherry_picked_from: bool,
    /// Re-create merge commits (`Preserve merges`).
    pub preserve_merges: bool,
}

/// Why the sequencer stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stop {
    /// An Edit step was applied (uncommitted) and waits for the user.
    Edit { index: usize, message: String },
    /// Applying the step produced conflicts; resolve them, then continue.
    Conflict { index: usize, output: String },
    /// Applying failed for another reason (e.g. the commit became empty).
    Failed { index: usize, output: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// Every step was replayed; call [`Sequencer::finish`].
    Finished,
    Stopped(Stop),
}

/// The commit still owed for the step the sequencer stopped at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Owed {
    /// Commit with the original message and author.
    Commit,
    /// Amend HEAD with HEAD's message followed by this commit's message.
    Squash,
    /// Commit with the (possibly edited) message from the dialog.
    Edit,
}

#[derive(Debug, Clone)]
pub struct Sequencer {
    pub plan: Plan,
    orig_head: String,
    orig_branch_tip: Option<String>,
    /// Resolved `plan.onto` (rebase mode).
    onto_id: String,
    next: usize,
    owed: Option<Owed>,
    /// Old commit id -> replayed commit id (for preserve merges).
    rewritten: HashMap<String, String>,
    /// Whether any commit has been made since the start (squash needs one).
    committed: bool,
    pub log: Vec<String>,
}

fn s(v: &str) -> String {
    v.to_owned()
}

/// Commits of `upstream..branch` for the rebase list, oldest first.
/// Merges are included only with `preserve_merges`.
pub fn rebase_steps(
    repo: &Repo,
    upstream: &str,
    branch: &str,
    preserve_merges: bool,
) -> Result<Vec<Step>> {
    let range = format!("{upstream}..{branch}");
    let mut args = vec![s("log"), s("--reverse"), s("--topo-order")];
    if !preserve_merges {
        args.push(s("--no-merges"));
    }
    args.extend([
        s("--format=%H%x1f%P%x1f%an%x1f%s%x1e"),
        s("--end-of-options"),
        range,
        s("--"),
    ]);
    Ok(parse_steps(&repo.git().run(args)?))
}

/// Steps for cherry-picking the given commits in this order.
pub fn cherry_pick_steps(repo: &Repo, revs: &[String]) -> Result<Vec<Step>> {
    let mut steps = Vec::new();
    for r in revs {
        let out = repo.git().run([
            "log",
            "-1",
            "--format=%H%x1f%P%x1f%an%x1f%s%x1e",
            "--end-of-options",
            r,
            "--",
        ])?;
        steps.extend(parse_steps(&out));
    }
    Ok(steps)
}

fn parse_steps(out: &str) -> Vec<Step> {
    out.split('\u{1e}')
        .filter_map(|rec| {
            let rec = rec.trim_start_matches('\n');
            let f: Vec<&str> = rec.split('\u{1f}').collect();
            if f.len() < 4 || f[0].is_empty() {
                return None;
            }
            Some(Step {
                id: f[0].to_owned(),
                parents: f[1].split_whitespace().map(str::to_owned).collect(),
                author: f[2].to_owned(),
                subject: f[3].to_owned(),
                action: RebaseAction::Pick,
                state: StepState::Pending,
            })
        })
        .collect()
}

impl Sequencer {
    /// Starts the replay: records `ORIG_HEAD` and, in rebase mode, detaches HEAD
    /// at `plan.onto` (`git checkout -f <onto> --`).
    pub fn start(repo: &Repo, plan: Plan) -> Result<Self> {
        let git = repo.git();
        let orig_head = repo.rev_parse("HEAD")?;
        let orig_branch_tip = match &plan.branch {
            Some(b) => Some(repo.rev_parse(b)?),
            None => None,
        };
        git.run(["update-ref", "ORIG_HEAD", &orig_head])?;
        let mut log = Vec::new();
        let mut onto_id = String::new();
        if !plan.cherry_pick {
            let onto = repo.rev_parse(&plan.onto)?;
            git.run(["checkout", "-f", &onto, "--"])?;
            log.push(format!("git checkout -f {onto} --"));
            onto_id = onto;
        }
        let _ = std::fs::write(marker(repo), &orig_head);
        Ok(Sequencer {
            plan,
            orig_head,
            orig_branch_tip,
            onto_id,
            next: 0,
            owed: None,
            rewritten: HashMap::new(),
            committed: false,
            log,
        })
    }

    /// Index of the step being worked on (the stop), if any.
    pub fn current(&self) -> Option<usize> {
        self.owed.map(|_| self.next)
    }

    /// Replays steps until all are done or one stops.
    pub fn run(&mut self, repo: &Repo) -> Result<Progress> {
        while self.next < self.plan.steps.len() {
            if let Some(stop) = self.apply(repo, self.next)? {
                return Ok(Progress::Stopped(stop));
            }
        }
        Ok(Progress::Finished)
    }

    /// Finishes the stopped step (after an Edit or resolved conflicts), then
    /// continues. `message` replaces the commit message for Edit steps.
    pub fn continue_with(&mut self, repo: &Repo, message: Option<&str>) -> Result<Progress> {
        let Some(owed) = self.owed else {
            return self.run(repo);
        };
        let unmerged = repo.git().run(["diff", "--name-only", "--diff-filter=U"])?;
        if !unmerged.trim().is_empty() {
            return Ok(Progress::Stopped(Stop::Conflict {
                index: self.next,
                output: format!("Resolve the conflicts first:\n{unmerged}"),
            }));
        }
        quit_cherry_pick(repo);
        let index = self.next;
        self.settle(repo, index, owed, message)?;
        self.run(repo)
    }

    /// Skips the stopped step, discarding its changes (`git reset --hard`).
    pub fn skip_current(&mut self, repo: &Repo) -> Result<Progress> {
        if self.owed.is_some() {
            quit_cherry_pick(repo);
            repo.git().run(["reset", "--hard"])?;
            self.log.push(s("git reset --hard"));
            self.owed = None;
            self.mark_done(self.next);
        }
        self.run(repo)
    }

    /// Moves the branch to the result (rebase mode) and removes the marker.
    pub fn finish(&mut self, repo: &Repo) -> Result<String> {
        let head = repo.rev_parse("HEAD")?;
        if let Some(b) = &self.plan.branch {
            repo.git().run(["checkout", "-f", "-B", b, &head, "--"])?;
            repo.git().run(["reset", "--hard", &head, "--"])?;
            self.log.push(format!("git checkout -f -B {b} {head} --"));
        }
        let _ = std::fs::remove_file(marker(repo));
        Ok(head)
    }

    /// Restores the state before the rebase / cherry-pick.
    pub fn abort(&mut self, repo: &Repo) -> Result<()> {
        let git = repo.git();
        quit_cherry_pick(repo);
        git.run(["reset", "--hard"])?;
        match (&self.plan.branch, &self.orig_branch_tip) {
            (Some(b), Some(tip)) => {
                git.run(["checkout", "-f", "-B", b, tip, "--"])?;
            }
            _ => {
                git.run(["reset", "--hard", &self.orig_head, "--"])?;
            }
        }
        let _ = std::fs::remove_file(marker(repo));
        Ok(())
    }

    fn mark_done(&mut self, index: usize) {
        if let Some(step) = self.plan.steps.get_mut(index) {
            step.state = StepState::Done;
        }
        self.next = index + 1;
    }

    fn git_logged(&mut self, repo: &Repo, args: &[String]) -> Result<crate::git_cli::GitOutput> {
        self.log.push(format!("git {}", args.join(" ")));
        let out = repo.git().output(args)?;
        if !out.stdout.trim().is_empty() {
            self.log.push(out.stdout.trim_end().to_owned());
        }
        if !out.stderr.trim().is_empty() {
            self.log.push(out.stderr.trim_end().to_owned());
        }
        Ok(out)
    }

    /// Applies step `index`. Returns a stop, or `None` when the step is done.
    fn apply(&mut self, repo: &Repo, index: usize) -> Result<Option<Stop>> {
        let step = self.plan.steps[index].clone();
        self.plan.steps[index].state = StepState::Current;
        let mut action = step.action;
        // Nothing to squash into yet: replay as a pick.
        if action == RebaseAction::Squash && !self.committed && !self.plan.cherry_pick {
            action = RebaseAction::Pick;
        }
        if action == RebaseAction::Skip {
            self.mark_done(index);
            return Ok(None);
        }
        let is_merge = step.parents.len() > 1;
        if self.plan.preserve_merges && !self.plan.cherry_pick {
            // Replay on top of the rewritten first parent (or the new base).
            let in_plan = |id: &str| self.plan.steps.iter().any(|s| s.id == id);
            let target = match step.parents.first() {
                Some(p) if self.rewritten.contains_key(p) => self.rewritten[p].clone(),
                Some(p) if in_plan(p) => p.clone(),
                _ => self.onto_id.clone(),
            };
            if repo.rev_parse("HEAD")? != target {
                self.git_logged(repo, &[s("checkout"), s("-f"), target, s("--")])?;
            }
        }
        let no_commit = matches!(action, RebaseAction::Squash | RebaseAction::Edit);
        let out = if is_merge && self.plan.preserve_merges && !self.plan.cherry_pick {
            // Re-create the merge on top of the replayed first parent.
            let mut args = vec![s("merge"), s("--no-ff"), s("--no-commit"), s("--")];
            for p in &step.parents[1..] {
                args.push(self.rewritten.get(p).cloned().unwrap_or_else(|| p.clone()));
            }
            self.git_logged(repo, &args)?
        } else {
            let mut args = vec![s("cherry-pick")];
            if self.plan.add_cherry_picked_from && !no_commit {
                args.push(s("-x"));
            } else if !self.plan.cherry_pick && !no_commit {
                args.push(s("--ff"));
            }
            args.push(s("--allow-empty"));
            if is_merge {
                args.extend([s("-m"), s("1")]);
            }
            if no_commit {
                args.push(s("--no-commit"));
            }
            args.push(step.id.clone());
            self.git_logged(repo, &args)?
        };
        let owed = match action {
            RebaseAction::Squash => Owed::Squash,
            RebaseAction::Edit => Owed::Edit,
            _ => Owed::Commit,
        };
        // A re-created merge (`merge --no-commit`) still needs its commit.
        let recreated_merge = is_merge && self.plan.preserve_merges && !self.plan.cherry_pick;
        let picked_and_committed = !(no_commit || recreated_merge);
        if !out.success() {
            self.owed = Some(owed);
            let conflicts = !repo
                .git()
                .run(["diff", "--name-only", "--diff-filter=U"])?
                .trim()
                .is_empty();
            let output = format!("{}{}", out.stdout, out.stderr);
            return Ok(Some(if conflicts {
                Stop::Conflict { index, output }
            } else {
                Stop::Failed { index, output }
            }));
        }
        if picked_and_committed {
            self.record(repo, index)?;
            return Ok(None);
        }
        if action == RebaseAction::Edit {
            self.owed = Some(Owed::Edit);
            return Ok(Some(Stop::Edit {
                index,
                message: commit_message(repo, &step.id)?,
            }));
        }
        self.settle(repo, index, owed, None)?;
        Ok(None)
    }

    /// Makes the commit owed for step `index` and marks it done.
    fn settle(
        &mut self,
        repo: &Repo,
        index: usize,
        owed: Owed,
        message: Option<&str>,
    ) -> Result<()> {
        let step = self.plan.steps[index].clone();
        let original = commit_message(repo, &step.id)?;
        let (author, date) = author_of(repo, &step.id)?;
        let mut msg = match (owed, message) {
            (Owed::Edit, Some(m)) if !m.trim().is_empty() => m.to_owned(),
            _ => original.clone(),
        };
        if self.plan.add_cherry_picked_from && owed != Owed::Squash {
            msg = format!(
                "{}\n\n(cherry picked from commit {})\n",
                msg.trim_end(),
                step.id
            );
        }
        let args = if owed == Owed::Squash && self.committed {
            let head_msg = commit_message(repo, "HEAD")?;
            let combined = format!("{}\n\n{}", head_msg.trim_end(), original.trim_end());
            let file = message_file(repo, &combined)?;
            vec![
                s("commit"),
                s("--amend"),
                s("--allow-empty"),
                s("--no-verify"),
                s("-F"),
                file,
            ]
        } else {
            let file = message_file(repo, &msg)?;
            vec![
                s("commit"),
                s("--allow-empty"),
                s("--no-verify"),
                format!("--author={author}"),
                format!("--date={date}"),
                s("-F"),
                file,
            ]
        };
        let out = self.git_logged(repo, &args)?;
        if !out.success() {
            return Err(Error::GitFailed {
                args: args.join(" "),
                code: out.code,
                stderr: out.stderr,
            });
        }
        self.owed = None;
        self.record(repo, index)
    }

    fn record(&mut self, repo: &Repo, index: usize) -> Result<()> {
        let head = repo.rev_parse("HEAD")?;
        let id = self.plan.steps[index].id.clone();
        self.rewritten.insert(id, head);
        self.committed = true;
        self.mark_done(index);
        Ok(())
    }
}

fn marker(repo: &Repo) -> PathBuf {
    repo.git_dir().join(ACTIVE_MARKER)
}

fn quit_cherry_pick(repo: &Repo) {
    if repo.git_dir().join("CHERRY_PICK_HEAD").exists() {
        let _ = repo.git().output(["cherry-pick", "--quit"]);
    }
    if repo.git_dir().join("MERGE_HEAD").exists() {
        let _ = std::fs::remove_file(repo.git_dir().join("MERGE_HEAD"));
        let _ = std::fs::remove_file(repo.git_dir().join("MERGE_MSG"));
        let _ = std::fs::remove_file(repo.git_dir().join("MERGE_MODE"));
    }
}

/// Full message of a commit.
pub fn commit_message(repo: &Repo, rev: &str) -> Result<String> {
    repo.git()
        .run(["log", "-1", "--format=%B", "--end-of-options", rev, "--"])
}

fn author_of(repo: &Repo, rev: &str) -> Result<(String, String)> {
    let out = repo.git().run([
        "log",
        "-1",
        "--format=%an <%ae>%x1f%ad",
        "--date=raw",
        "--end-of-options",
        rev,
        "--",
    ])?;
    let (a, d) = out.trim().split_once('\u{1f}').unwrap_or((out.trim(), ""));
    Ok((a.to_owned(), d.to_owned()))
}

fn message_file(repo: &Repo, text: &str) -> Result<String> {
    let path = repo.git_dir().join("tgitrebase-msg.txt");
    std::fs::write(&path, text)?;
    Ok(path.to_string_lossy().into_owned())
}

/// Whether a TortoiseGit-style rebase is in progress.
pub fn is_active(repo: &Repo) -> bool {
    marker(repo).exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_cli::Git;

    /// main: a-b ; topic (from a): t1 t2 t3, each touching its own file.
    fn setup() -> (tempfile::TempDir, Repo) {
        let dir = tempfile::tempdir().unwrap();
        let git = Git::new(dir.path());
        for c in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.name", "T"],
            vec!["config", "user.email", "t@e"],
            vec!["config", "commit.gpgsign", "false"],
        ] {
            git.run(c).unwrap();
        }
        let commit = |file: &str, text: &str, msg: &str| {
            std::fs::write(dir.path().join(file), text).unwrap();
            git.run(["add", file]).unwrap();
            git.run(["commit", "-qm", msg]).unwrap();
        };
        commit("a.txt", "a\n", "a");
        git.run(["checkout", "-qb", "topic"]).unwrap();
        commit("t1.txt", "1\n", "t1");
        commit("t2.txt", "2\n", "t2");
        commit("t3.txt", "3\n", "t3");
        git.run(["checkout", "-q", "main"]).unwrap();
        commit("b.txt", "b\n", "b");
        git.run(["checkout", "-q", "topic"]).unwrap();
        let repo = Repo::discover(dir.path()).unwrap();
        (dir, repo)
    }

    fn subjects(repo: &Repo, range: &str) -> Vec<String> {
        repo.git()
            .run(["log", "--reverse", "--format=%s", range])
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn plan(repo: &Repo, actions: &[RebaseAction]) -> Plan {
        let mut steps = rebase_steps(repo, "main", "topic", false).unwrap();
        for (s, a) in steps.iter_mut().zip(actions) {
            s.action = *a;
        }
        Plan {
            branch: Some("topic".into()),
            onto: "main".into(),
            steps,
            ..Default::default()
        }
    }

    #[test]
    fn pick_squash_skip() {
        let (_d, repo) = setup();
        use RebaseAction::*;
        let mut seq = Sequencer::start(&repo, plan(&repo, &[Pick, Squash, Skip])).unwrap();
        assert_eq!(seq.run(&repo).unwrap(), Progress::Finished);
        seq.finish(&repo).unwrap();
        assert_eq!(repo.current_branch().unwrap().as_deref(), Some("topic"));
        assert_eq!(subjects(&repo, "main..topic"), ["t1"]);
        let msg = commit_message(&repo, "topic").unwrap();
        assert!(msg.starts_with("t1\n\nt2"), "{msg:?}");
        assert!(repo.workdir().join("t2.txt").exists());
        assert!(!repo.workdir().join("t3.txt").exists());
        assert!(!is_active(&repo));
    }

    #[test]
    fn edit_stops_and_continues_with_new_message() {
        let (_d, repo) = setup();
        use RebaseAction::*;
        let mut seq = Sequencer::start(&repo, plan(&repo, &[Pick, Edit, Pick])).unwrap();
        assert!(is_active(&repo));
        match seq.run(&repo).unwrap() {
            Progress::Stopped(Stop::Edit { index, message }) => {
                assert_eq!(index, 1);
                assert_eq!(message.trim(), "t2");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(seq.current(), Some(1));
        std::fs::write(repo.workdir().join("t2.txt"), "edited\n").unwrap();
        repo.git().run(["add", "t2.txt"]).unwrap();
        assert_eq!(
            seq.continue_with(&repo, Some("t2 edited\n")).unwrap(),
            Progress::Finished
        );
        seq.finish(&repo).unwrap();
        assert_eq!(subjects(&repo, "main..topic"), ["t1", "t2 edited", "t3"]);
        assert_eq!(
            std::fs::read_to_string(repo.workdir().join("t2.txt")).unwrap(),
            "edited\n"
        );
        // The author of the edited commit is kept.
        let author = repo
            .git()
            .run(["log", "-1", "--format=%an", "topic~1"])
            .unwrap();
        assert_eq!(author.trim(), "T");
    }

    #[test]
    fn conflict_then_continue_and_abort() {
        let (dir, repo) = setup();
        let git = repo.git();
        // Make t2 conflict with main.
        git.run(["checkout", "-q", "main"]).unwrap();
        std::fs::write(dir.path().join("t2.txt"), "main\n").unwrap();
        git.run(["add", "t2.txt"]).unwrap();
        git.run(["commit", "-qm", "main t2"]).unwrap();
        git.run(["checkout", "-q", "topic"]).unwrap();
        let tip = repo.rev_parse("topic").unwrap();
        use RebaseAction::*;

        let mut seq = Sequencer::start(&repo, plan(&repo, &[Pick, Pick, Pick])).unwrap();
        assert!(matches!(
            seq.run(&repo).unwrap(),
            Progress::Stopped(Stop::Conflict { index: 1, .. })
        ));
        // Continue without resolving is refused.
        assert!(matches!(
            seq.continue_with(&repo, None).unwrap(),
            Progress::Stopped(Stop::Conflict { .. })
        ));
        std::fs::write(dir.path().join("t2.txt"), "resolved\n").unwrap();
        git.run(["add", "t2.txt"]).unwrap();
        assert_eq!(seq.continue_with(&repo, None).unwrap(), Progress::Finished);
        seq.finish(&repo).unwrap();
        assert_eq!(subjects(&repo, "main..topic"), ["t1", "t2", "t3"]);

        // Abort restores the branch tip.
        git.run(["reset", "-q", "--hard", &tip]).unwrap();
        let mut seq = Sequencer::start(&repo, plan(&repo, &[Pick, Pick, Pick])).unwrap();
        assert!(matches!(seq.run(&repo).unwrap(), Progress::Stopped(_)));
        seq.abort(&repo).unwrap();
        assert_eq!(repo.rev_parse("topic").unwrap(), tip);
        assert_eq!(repo.current_branch().unwrap().as_deref(), Some("topic"));
        assert!(!is_active(&repo));
    }

    #[test]
    fn cherry_pick_mode_adds_trailer() {
        let (_d, repo) = setup();
        let git = repo.git();
        git.run(["checkout", "-q", "main"]).unwrap();
        let t3 = repo.rev_parse("topic").unwrap();
        let t1 = repo.rev_parse("topic~2").unwrap();
        let steps = cherry_pick_steps(&repo, &[t1.clone(), t3.clone()]).unwrap();
        assert_eq!(steps.len(), 2);
        let mut seq = Sequencer::start(
            &repo,
            Plan {
                steps,
                cherry_pick: true,
                add_cherry_picked_from: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(seq.run(&repo).unwrap(), Progress::Finished);
        seq.finish(&repo).unwrap();
        assert_eq!(repo.current_branch().unwrap().as_deref(), Some("main"));
        assert_eq!(subjects(&repo, "main~2..main"), ["t1", "t3"]);
        let msg = commit_message(&repo, "main").unwrap();
        assert!(
            msg.contains(&format!("(cherry picked from commit {t3})")),
            "{msg}"
        );
    }

    #[test]
    fn preserve_merges_recreates_merge() {
        let (dir, repo) = setup();
        let git = repo.git();
        git.run(["checkout", "-qb", "side", "topic~1"]).unwrap();
        std::fs::write(dir.path().join("s.txt"), "s\n").unwrap();
        git.run(["add", "s.txt"]).unwrap();
        git.run(["commit", "-qm", "s"]).unwrap();
        git.run(["checkout", "-q", "topic"]).unwrap();
        git.run(["merge", "-q", "--no-ff", "side", "-m", "merge side"])
            .unwrap();
        let steps = rebase_steps(&repo, "main", "topic", true).unwrap();
        assert!(steps.iter().any(|s| s.parents.len() == 2));
        let mut seq = Sequencer::start(
            &repo,
            Plan {
                branch: Some("topic".into()),
                onto: "main".into(),
                steps,
                preserve_merges: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(seq.run(&repo).unwrap(), Progress::Finished);
        seq.finish(&repo).unwrap();
        let parents = git.run(["log", "-1", "--format=%P", "topic"]).unwrap();
        assert_eq!(parents.split_whitespace().count(), 2);
        assert!(repo.is_ancestor("main", "topic"));
        assert!(dir.path().join("s.txt").exists());
    }
}
