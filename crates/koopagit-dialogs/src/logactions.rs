//! Executes log-list context-menu actions (`CGitLogList::ContextMenuAction`,
//! spec-menus.md §2.3). Shared by the Log dialog and the Sync dialog's
//! In/Out Commits lists.
//!
//! Everything here runs inside another dialog's event loop, so confirmations,
//! progress windows and the notes editor are non-blocking windows.

use std::path::PathBuf;
use std::rc::Rc;

use koopagit_core::Repo;
use koopagit_core::git_cli::Git;
use koopagit_core::history::WORKING_TREE_REV;
use koopagit_core::launcher::{CloseOnEnd, Invocation};
use koopagit_core::log::LogEntry;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::log_menu::{
    ALL_PARENTS, CopyCommit, DELETE_ALL, EXTRA_AFTER_MERGE, LogAction, ONLY_MERGED, copy_text,
};
use crate::progress::{PostAction, ProgressJob};
use crate::{MessageBox, NotesDialog, keep_alive, spawn_launcher};

/// The empty tree, used as "previous revision" of a root commit.
pub const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// The working-tree pseudo-row has an empty id.
pub fn is_wc(e: &LogEntry) -> bool {
    e.id.is_empty()
}

/// The clipboard's text, for pasting a commit hash into a log list.
pub fn clipboard_text() -> Option<String> {
    arboard::Clipboard::new()
        .and_then(|mut c| c.get_text())
        .ok()
}

/// Parent `k` (1-based) of `e`, or the empty tree for a root commit.
pub fn parent(e: &LogEntry, k: usize) -> String {
    e.parents
        .get(k.saturating_sub(1))
        .cloned()
        .unwrap_or_else(|| EMPTY_TREE.to_owned())
}

/// Full ref names of a commit's labels.
pub fn full_refs(e: &LogEntry) -> Vec<String> {
    use koopagit_core::log::RefKind;
    e.refs
        .iter()
        .map(|r| match r.kind {
            RefKind::CurrentBranch | RefKind::LocalBranch => format!("refs/heads/{}", r.name),
            RefKind::RemoteBranch => format!("refs/remotes/{}", r.name),
            RefKind::Tag => format!("refs/tags/{}", r.name),
            RefKind::Stash => "refs/stash".to_owned(),
            RefKind::Other => r.name.clone(),
        })
        .collect()
}

/// `StripRefName`: drops `refs/heads/`, otherwise `refs/`.
fn strip_ref(r: &str) -> String {
    r.strip_prefix("refs/heads/")
        .or_else(|| r.strip_prefix("refs/"))
        .unwrap_or(r)
        .to_owned()
}

/// Where a log-list action runs.
#[derive(Clone)]
pub struct LogActions {
    pub workdir: PathBuf,
    /// The single file the log is for (Blame entries).
    pub file: Option<PathBuf>,
    /// The path the log was opened for (`/path`), for range logs.
    pub log_path: PathBuf,
    /// Whether dates are shown relative (clipboard "Date:" matches the list).
    pub relative_dates: bool,
    /// Reloads the list after git changed something.
    pub reload: Rc<dyn Fn()>,
    /// Focuses the filter box ("Search log messages...").
    pub find: Rc<dyn Fn()>,
    /// Shows text the clipboard could not take.
    pub show_text: Rc<dyn Fn(String)>,
    /// The list is the reflog (clipboard formats differ).
    pub reflog: bool,
}

impl LogActions {
    fn launch(&self, command: &str) -> Invocation {
        Invocation::new(command).with_paths([self.workdir.clone()])
    }

    fn git(&self) -> Git {
        Git::new(&self.workdir)
    }

    /// Runs git commands in a non-blocking progress window, then reloads.
    pub fn git_job(&self, title: &str, commands: Vec<Vec<String>>, post: Vec<PostAction>) {
        let job = ProgressJob {
            title: crate::window_title(&self.workdir, title),
            git: self.git(),
            commands,
            post_actions: post,
            close_on_end: CloseOnEnd::Manual,
        };
        let reload = self.reload.clone();
        match crate::progress::start(job, move |_| reload()) {
            Ok(p) => keep_alive(p),
            Err(e) => eprintln!("koopagit: {e}"),
        }
    }

    /// Executes `action` (with sub-entry `k`) for the selected rows (top to bottom).
    pub fn run(&self, action: LogAction, k: u32, sel: &[LogEntry]) {
        let Some(e) = sel.first() else { return };
        let id = |e: &LogEntry| {
            if is_wc(e) {
                WORKING_TREE_REV.to_owned()
            } else {
                e.id.clone()
            }
        };
        let pk = (k as usize).max(1);
        match action {
            LogAction::Commit => spawn_launcher(&self.launch("commit")),
            LogAction::MergeAbort => spawn_launcher(&self.launch("merge").with_switch("abort")),
            LogAction::Compare => spawn_launcher(
                &self
                    .launch("showcompare")
                    .with("revision1", e.id.clone())
                    .with("revision2", WORKING_TREE_REV),
            ),
            LogAction::CompareWithPrevious => spawn_launcher(
                &self
                    .launch("showcompare")
                    .with("revision1", parent(e, pk))
                    .with("revision2", id(e)),
            ),
            LogAction::GnuDiff1 => match k {
                ALL_PARENTS | ONLY_MERGED | EXTRA_AFTER_MERGE => self.combined_diff(e, k),
                _ => spawn_launcher(
                    &self
                        .launch("showcompare")
                        .with("revision1", parent(e, pk))
                        .with("revision2", id(e))
                        .with_switch("unified"),
                ),
            },
            LogAction::GnuDiff2 if sel.len() >= 2 => spawn_launcher(
                &self
                    .launch("showcompare")
                    .with("revision1", id(&sel[sel.len() - 1]))
                    .with("revision2", id(&sel[0]))
                    .with_switch("unified"),
            ),
            LogAction::CompareTwo | LogAction::CompareTwoCommitChanges if sel.len() >= 2 => {
                spawn_launcher(
                    &self
                        .launch("showcompare")
                        .with("revision1", id(&sel[sel.len() - 1]))
                        .with("revision2", id(&sel[0])),
                )
            }
            LogAction::Blame | LogAction::BlamePrevious => {
                if let Some(f) = &self.file {
                    let rev = if action == LogAction::Blame {
                        e.id.clone()
                    } else {
                        parent(e, pk)
                    };
                    spawn_launcher(
                        &Invocation::new("blame")
                            .with_paths([f.clone()])
                            .with("endrev", rev),
                    );
                }
            }
            LogAction::StashSave => spawn_launcher(&self.launch("stashsave")),
            LogAction::StashPop => spawn_launcher(&self.launch("stashpop")),
            LogAction::StashList => {
                spawn_launcher(&self.launch("reflog").with("ref", "refs/stash"))
            }
            LogAction::ReflogStashApply => {
                if let Some(r) = reflog_selector(e) {
                    self.git_job(
                        "Stash Apply",
                        vec![koopagit_core::commands::stash_apply(&r)],
                        vec![],
                    );
                }
            }
            LogAction::BisectGood | LogAction::BisectBad => {
                let verb = if action == LogAction::BisectGood {
                    "good"
                } else {
                    "bad"
                };
                let mut cmd = vec!["bisect".to_owned(), verb.to_owned()];
                if !is_wc(e) {
                    cmd.push(e.id.clone());
                }
                self.git_job(&format!("Bisect {verb}"), vec![cmd], vec![]);
            }
            LogAction::BisectSkip => {
                let mut cmd = vec!["bisect".to_owned(), "skip".to_owned()];
                cmd.extend(sel.iter().filter(|e| !is_wc(e)).map(|e| e.id.clone()));
                self.git_job("Bisect skip", vec![cmd], vec![]);
            }
            LogAction::BisectReset => self.git_job(
                "Bisect reset",
                vec![vec!["bisect".into(), "reset".into()]],
                vec![],
            ),
            LogAction::BisectStart if sel.len() == 2 => {
                // Good = the older (last) selected, bad = the newer (first).
                let (bad, good) = (sel[0].id.clone(), sel[1].id.clone());
                self.git_job(
                    "Bisect start",
                    vec![vec![
                        "bisect".into(),
                        "start".into(),
                        bad,
                        good,
                        "--".into(),
                    ]],
                    vec![],
                );
            }
            LogAction::Pull | LogAction::PullHead => spawn_launcher(&self.launch("pull")),
            LogAction::Fetch => spawn_launcher(&self.launch("fetch")),
            LogAction::SubmoduleUpdate => spawn_launcher(
                &self
                    .launch("subupdate")
                    .with("bkpath", self.workdir.to_string_lossy()),
            ),
            LogAction::Cleanup => spawn_launcher(&self.launch("cleanup")),
            LogAction::Log => spawn_launcher(
                &Invocation::new("log")
                    .with_paths([self.workdir.clone()])
                    .with("endrev", e.id.clone())
                    .with("rev", e.id.clone()),
            ),
            LogAction::RepoBrowse => {
                spawn_launcher(&self.launch("repobrowser").with("rev", e.id.clone()))
            }
            LogAction::MergeRev => {
                spawn_launcher(&self.launch("merge").with("rev", preferred_ref(e, false)))
            }
            LogAction::Reset => {
                let reload = self.reload.clone();
                if let Err(err) = crate::reset::show(self.workdir.clone(), &e.id, move |_| reload())
                {
                    eprintln!("koopagit: {err}");
                }
            }
            LogAction::SwitchBranch => {
                let refs = full_refs(e);
                if let Some(r) = refs.get(k as usize) {
                    let branch = strip_ref(r);
                    self.git_job(
                        "Switch/Checkout",
                        vec![vec![
                            "checkout".into(),
                            "--end-of-options".into(),
                            branch,
                            "--".into(),
                        ]],
                        vec![],
                    );
                }
            }
            LogAction::SwitchToRev => {
                spawn_launcher(&self.launch("switch").with("rev", preferred_ref(e, true)))
            }
            LogAction::CreateBranch => {
                spawn_launcher(&self.launch("branch").with("rev", preferred_ref(e, true)))
            }
            LogAction::CreateTag => {
                spawn_launcher(&self.launch("tag").with("rev", preferred_ref(e, true)))
            }
            LogAction::RebaseToVersion => {
                let upstream = full_refs(e)
                    .iter()
                    .find(|r| r.starts_with("refs/heads/"))
                    .map(|r| strip_ref(r))
                    .or_else(|| full_refs(e).first().map(|r| strip_ref(r)))
                    .unwrap_or_else(|| e.id.clone());
                spawn_launcher(&self.launch("rebase").with("upstream", upstream));
            }
            LogAction::Export => {
                let rev = e
                    .refs
                    .iter()
                    .find(|r| r.kind == koopagit_core::log::RefKind::Tag)
                    .map(|r| r.name.clone())
                    .unwrap_or_else(|| e.id.clone());
                spawn_launcher(&self.launch("export").with("rev", rev));
            }
            LogAction::RevertRev => self.revert(sel, k),
            LogAction::EditNote => self.edit_notes(e),
            LogAction::ReflogDel => self.delete_reflog(sel),
            LogAction::LogViewRange
            | LogAction::LogViewRangeReverse
            | LogAction::LogViewRangeReachable
                if sel.len() == 2 =>
            {
                let (first, last) = (&sel[0].id, &sel[1].id);
                let range = match action {
                    LogAction::LogViewRange => format!("{last}..{first}"),
                    LogAction::LogViewRangeReverse => format!("{first}..{last}"),
                    _ => format!("{last}...{first}"),
                };
                spawn_launcher(
                    &Invocation::new("log")
                        .with_paths([self.log_path.clone()])
                        .with("range", range),
                );
            }
            LogAction::CombineCommit => self.combine(sel),
            LogAction::CherryPick => {
                // Oldest first, as the cherry-pick dialog applies them in order.
                let revs: Vec<String> = sel
                    .iter()
                    .rev()
                    .filter(|e| !is_wc(e))
                    .map(|e| e.id.clone())
                    .collect();
                spawn_launcher(
                    &self
                        .launch("rebase")
                        .with_switch("cherrypick")
                        .with("revs", revs.join(",")),
                );
            }
            LogAction::CreatePatch => {
                let newest = &sel[0].id;
                let oldest = &sel[sel.len() - 1].id;
                spawn_launcher(
                    &self
                        .launch("formatpatch")
                        .with("range", format!("{oldest}~1..{newest}")),
                );
            }
            LogAction::Push => {
                let refs = full_refs(e);
                let local = refs
                    .iter()
                    .find(|r| r.starts_with("refs/heads/"))
                    .or_else(|| refs.iter().find(|r| r.starts_with("refs/tags/")))
                    .map(|r| r.trim_end_matches("^{}").to_owned());
                let mut inv = self.launch("push");
                if let Some(l) = local {
                    inv = inv.with("branch", l);
                }
                spawn_launcher(&inv);
            }
            LogAction::SvnDCommit => spawn_launcher(&self.launch("svndcommit")),
            LogAction::Delete => self.delete_refs(e, k),
            // `CCommitIsOnRefsDlg` (GitLogListAction.cpp:1023).
            LogAction::ShowBranches => {
                spawn_launcher(&self.launch("commitisonrefs").with("rev", e.id.clone()))
            }
            LogAction::FindEntry => (self.find)(),
            LogAction::CopyFull
            | LogAction::CopyNoPaths
            | LogAction::CopySha
            | LogAction::CopyAuthorsFull
            | LogAction::CopyAuthorNames
            | LogAction::CopyAuthorEmails
            | LogAction::CopySubjects
            | LogAction::CopyMessages
            | LogAction::CopyRefNames => {
                let commits: Vec<CopyCommit> = sel
                    .iter()
                    .map(|e| self.copy_commit(e, action == LogAction::CopyFull))
                    .collect();
                let text = copy_text(action, &commits, self.reflog);
                if !crate::clipboard::copy(&text) {
                    (self.show_text)(text);
                }
            }
            _ => {}
        }
    }

    fn copy_commit(&self, e: &LogEntry, with_paths: bool) -> CopyCommit {
        let repo = Repo::discover_including_bare(&self.workdir).ok();
        let body = e
            .message
            .split_once('\n')
            .map(|(_, b)| b.trim_start_matches('\n').to_owned())
            .unwrap_or_default();
        let paths = if with_paths && !is_wc(e) {
            repo.as_ref()
                .and_then(|r| r.commit_changes(&e.id).ok())
                .unwrap_or_default()
                .iter()
                .map(|f| match &f.old_path {
                    Some(old) => format!("{}: {} (from {old})", f.action().name(), f.path),
                    None => format!("{}: {}", f.action().name(), f.path),
                })
                .collect()
        } else {
            Vec::new()
        };
        let git = self.git();
        let notes = git
            .run(["notes", "show", &e.id])
            .map(|n| n.trim_end().to_owned())
            .unwrap_or_default();
        let tag_info = if e
            .refs
            .iter()
            .any(|r| r.kind == koopagit_core::log::RefKind::Tag)
        {
            git.run([
                "for-each-ref",
                "--points-at",
                &e.id,
                "--format=%(refname:short)%0a%(contents)",
                "refs/tags",
            ])
            .map(|t| t.trim_end().to_owned())
            .unwrap_or_default()
        } else {
            String::new()
        };
        let date = if self.relative_dates {
            crate::format::relative_time(e.author.time, crate::format::now())
        } else {
            crate::format::short_datetime(e.author.time, e.author.offset)
        };
        CopyCommit {
            id: e.id.clone(),
            author_name: e.author.name.clone(),
            author_email: e.author.email.clone(),
            date,
            subject: e.subject.clone(),
            body,
            notes,
            tag_info,
            paths,
            refs: full_refs(e),
            reflog_action: String::new(),
        }
    }

    /// Unified diff of a merge against all parents (`-m`), the combined diff
    /// (`--cc`), or the changes the merge itself introduced.
    fn combined_diff(&self, e: &LogEntry, k: u32) {
        let args: Vec<&str> = match k {
            ALL_PARENTS => vec!["diff-tree", "-p", "-m", "--no-commit-id", "-r"],
            ONLY_MERGED => vec!["diff-tree", "-p", "--cc", "--no-commit-id"],
            _ => vec!["diff-tree", "-p", "--cc", "--no-commit-id"],
        };
        let mut full: Vec<String> = args.iter().map(|s| (*s).to_owned()).collect();
        full.push(e.id.clone());
        let text = self.git().run(full).unwrap_or_default();
        if k == EXTRA_AFTER_MERGE && text.trim().is_empty() {
            confirm(
                "KoopaGit",
                "There are no changes after the merge.",
                &["OK"],
                |_| {},
            );
            return;
        }
        show_patch_text(&e.id, &text);
    }

    /// "Revert change(s) by this/these commit(s)": `git revert --no-edit
    /// --no-commit [-m k] <hash>` per commit, then offer Commit.
    fn revert(&self, sel: &[LogEntry], k: u32) {
        let commits: Vec<LogEntry> = sel.iter().filter(|e| !is_wc(e)).cloned().collect();
        if commits.is_empty() {
            return;
        }
        let text = if commits.len() == 1 {
            format!(
                "Do you really want to revert the changes of commit {}?",
                &commits[0].id[..commits[0].id.len().min(8)]
            )
        } else {
            format!(
                "Do you really want to revert the changes of the {} selected commits?",
                commits.len()
            )
        };
        let this = self.clone();
        confirm("KoopaGit", &text, &["No", "Yes"], move |choice| {
            if choice != 1 {
                return;
            }
            let commands = commits
                .iter()
                .map(|e| {
                    let mut c = vec!["revert".into(), "--no-edit".into(), "--no-commit".into()];
                    if e.parents.len() > 1 {
                        c.extend(["-m".into(), k.max(1).to_string()]);
                    }
                    c.push(e.id.clone());
                    c
                })
                .collect();
            this.git_job(
                "Revert",
                commands,
                vec![PostAction::on_success("&Commit...", this.launch("commit"))],
            );
        });
    }

    /// "Combine to one commit" for HEAD..HEAD~n: `git reset --soft <oldest>^`
    /// then commit with the concatenated messages (oldest first).
    fn combine(&self, sel: &[LogEntry]) {
        let Some(oldest) = sel.last() else { return };
        let dirty = Repo::discover(&self.workdir)
            .ok()
            .and_then(|r| r.status(Default::default(), &[]).ok())
            .is_some_and(|s| !s.is_empty());
        if dirty {
            confirm(
                "KoopaGit",
                "The working tree must be clean to combine commits. Commit or stash your changes first.",
                &["OK"],
                |_| {},
            );
            return;
        }
        let message: String = sel
            .iter()
            .rev()
            .map(|e| e.message.trim_end().to_owned())
            .collect::<Vec<_>>()
            .join("\n\n");
        let base = format!("{}^", oldest.id);
        let n = sel.len();
        let this = self.clone();
        confirm(
            "KoopaGit",
            &format!(
                "Combine the {n} selected commits into one commit?\nThe new commit uses their concatenated messages; amend it afterwards to edit the message."
            ),
            &["Combine", "Abort"],
            move |choice| {
                if choice != 0 {
                    return;
                }
                let file = match crate::temp_message_file(&message) {
                    Ok(f) => f,
                    Err(e) => {
                        eprintln!("koopagit: {e}");
                        return;
                    }
                };
                this.git_job(
                    "Combine to one commit",
                    vec![
                        vec![
                            "reset".into(),
                            "--soft".into(),
                            "--end-of-options".into(),
                            base.clone(),
                            "--".into(),
                        ],
                        vec![
                            "commit".into(),
                            "-F".into(),
                            file.to_string_lossy().into_owned(),
                        ],
                    ],
                    vec![PostAction::on_success("&Commit...", this.launch("commit"))],
                );
            },
        );
    }

    fn edit_notes(&self, e: &LogEntry) {
        if is_wc(e) {
            return;
        }
        let current = self.git().run(["notes", "show", &e.id]).unwrap_or_default();
        let Ok(dlg) = NotesDialog::new() else { return };
        dlg.set_dialog_title(format!("Edit Notes - {}", &e.id[..e.id.len().min(8)]).into());
        dlg.set_text(current.trim_end().into());
        let this = self.clone();
        let id = e.id.clone();
        let weak = dlg.as_weak();
        dlg.on_ok(move || {
            let Some(d) = weak.upgrade() else { return };
            let text = d.get_text().to_string();
            let cmd = if text.trim().is_empty() {
                vec![
                    "notes".into(),
                    "remove".into(),
                    "--ignore-missing".into(),
                    id.clone(),
                ]
            } else {
                match crate::temp_message_file(&text) {
                    Ok(f) => vec![
                        "notes".into(),
                        "add".into(),
                        "-f".into(),
                        "-F".into(),
                        f.to_string_lossy().into_owned(),
                        id.clone(),
                    ],
                    Err(_) => return,
                }
            };
            match this.git().output(&cmd) {
                Ok(o) if o.success() => {
                    let _ = d.hide();
                    (this.reload)();
                }
                Ok(o) => d.set_error(o.stderr.trim().into()),
                Err(e) => d.set_error(e.to_string().into()),
            }
        });
        let weak = dlg.as_weak();
        dlg.on_cancel(move || {
            if let Some(d) = weak.upgrade() {
                let _ = d.hide();
            }
        });
        crate::snapshot::arm(&dlg);
        let _ = dlg.show();
        keep_alive(dlg);
    }

    /// "Delete Ref..." in the reflog: confirm, then drop stash entries or
    /// delete reflog entries, last to first.
    fn delete_reflog(&self, sel: &[LogEntry]) {
        let refs: Vec<String> = sel.iter().filter_map(reflog_selector).collect();
        if refs.is_empty() {
            return;
        }
        let text = if refs.len() == 1 {
            format!(
                "\"{}\" will be permanently deleted. It can NOT be recovered!\n\nDo you really want to continue?",
                refs[0]
            )
        } else {
            format!(
                "Do you really want to permanently delete the {} selected refs? It can NOT be recovered!",
                refs.len()
            )
        };
        let this = self.clone();
        confirm("KoopaGit", &text, &["Delete", "Abort"], move |c| {
            if c != 0 {
                return;
            }
            let commands = refs
                .iter()
                .rev()
                .map(|r| {
                    if r.starts_with("stash") {
                        koopagit_core::commands::stash_drop(r)
                    } else {
                        vec!["reflog".into(), "delete".into(), "--".into(), r.clone()]
                    }
                })
                .collect();
            this.git_job("Delete Ref", commands, vec![]);
        });
    }

    fn delete_refs(&self, e: &LogEntry, k: u32) {
        let current = Repo::discover_including_bare(&self.workdir)
            .ok()
            .and_then(|r| r.current_branch().ok().flatten())
            .map(|b| format!("refs/heads/{b}"));
        let refs = full_refs(e);
        let targets: Vec<String> = if k == DELETE_ALL {
            refs.into_iter()
                .filter(|r| Some(r) != current.as_ref())
                .collect()
        } else {
            refs.get(k as usize).cloned().into_iter().collect()
        };
        if targets.is_empty() {
            return;
        }
        let text = if targets.len() == 1 {
            format!("Do you really want to delete \"{}\"?", targets[0])
        } else {
            format!(
                "Do you really want to delete these {} refs?\n{}",
                targets.len(),
                targets.join("\n")
            )
        };
        let this = self.clone();
        confirm("KoopaGit", &text, &["Delete", "Abort"], move |c| {
            if c != 0 {
                return;
            }
            let commands = targets
                .iter()
                .filter_map(|r| delete_ref_command(r))
                .collect();
            this.git_job("Delete", commands, vec![]);
        });
    }
}

/// Shows a patch in the unified-diff viewer (`koopagit-merge /diff:<file>`),
/// in its own process so the calling dialog keeps running.
fn show_patch_text(id: &str, text: &str) {
    let dir = std::env::temp_dir().join("koopagit");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join(format!(
        "{}-{}.diff",
        &id[..id.len().min(8)],
        std::process::id()
    ));
    if std::fs::write(&file, text).is_err() {
        return;
    }
    let exe = crate::diffcmd::sibling_exe("koopagit-merge");
    if let Err(e) = std::process::Command::new(exe)
        .arg(format!("/diff:{}", file.to_string_lossy()))
        .spawn()
    {
        eprintln!("koopagit: {e}");
    }
}

/// The git command that deletes a ref (`CAppUtils::DeleteRef`).
pub fn delete_ref_command(full: &str) -> Option<Vec<String>> {
    let s = |v: &str| v.to_owned();
    if let Some(b) = full.strip_prefix("refs/heads/") {
        Some(vec![s("branch"), s("-D"), s("--"), s(b)])
    } else if let Some(t) = full.strip_prefix("refs/tags/") {
        Some(vec![s("tag"), s("-d"), s("--"), s(t)])
    } else if let Some(rb) = full.strip_prefix("refs/remotes/") {
        let (remote, branch) = rb.split_once('/')?;
        Some(vec![
            s("push"),
            s(remote),
            s("--delete"),
            format!("refs/heads/{branch}"),
        ])
    } else if full == "refs/stash" {
        Some(vec![s("stash"), s("drop")])
    } else {
        Some(vec![s("update-ref"), s("-d"), s(full)])
    }
}

/// The ref a "switch/branch/tag at this version" dialog is preset to: the
/// first remote ref (for tracking) when `prefer_remote`, else the first ref,
/// else the hash.
fn preferred_ref(e: &LogEntry, prefer_remote: bool) -> String {
    let refs = full_refs(e);
    if prefer_remote {
        if let Some(r) = refs.iter().find(|r| r.starts_with("refs/remotes/")) {
            return r.trim_start_matches("refs/").to_owned();
        }
        return e.id.clone();
    }
    refs.first()
        .map(|r| strip_ref(r))
        .unwrap_or_else(|| e.id.clone())
}

/// Reflog rows carry their selector (`stash@{0}`) in the first ref label of
/// kind `Other` prefixed with `reflog:`.
fn reflog_selector(e: &LogEntry) -> Option<String> {
    e.refs
        .iter()
        .find_map(|r| r.name.strip_prefix("reflog:").map(str::to_owned))
}

/// A non-blocking message box; `on_choice` gets the clicked button index.
pub fn confirm(title: &str, text: &str, buttons: &[&str], on_choice: impl FnOnce(usize) + 'static) {
    let Ok(dlg) = MessageBox::new() else { return };
    dlg.set_dialog_title(title.into());
    dlg.set_text(text.into());
    let labels: Vec<SharedString> = buttons.iter().map(|b| (*b).into()).collect();
    dlg.set_buttons(ModelRc::new(VecModel::from(labels)));
    let cb = std::cell::RefCell::new(Some(on_choice));
    let weak = dlg.as_weak();
    dlg.on_clicked(move |i| {
        if let Some(d) = weak.upgrade() {
            let _ = d.hide();
        }
        if let Some(f) = cb.borrow_mut().take() {
            f(i as usize);
        }
    });
    let _ = dlg.show();
    keep_alive(dlg);
}

#[cfg(test)]
mod tests {
    use super::*;
    use koopagit_core::log::{RefKind, RefLabel, Signature};

    fn entry(refs: Vec<RefLabel>) -> LogEntry {
        let sig = Signature {
            name: "A".into(),
            email: "a@x".into(),
            time: 0,
            offset: 0,
        };
        LogEntry {
            id: "abc".into(),
            parents: vec!["p1".into()],
            author: sig.clone(),
            committer: sig,
            subject: "s".into(),
            message: "s\n".into(),
            refs,
        }
    }

    #[test]
    fn refs_and_delete_commands() {
        let e = entry(vec![
            RefLabel {
                kind: RefKind::LocalBranch,
                name: "topic".into(),
            },
            RefLabel {
                kind: RefKind::RemoteBranch,
                name: "origin/topic".into(),
            },
            RefLabel {
                kind: RefKind::Tag,
                name: "v1".into(),
            },
        ]);
        assert_eq!(
            full_refs(&e),
            [
                "refs/heads/topic",
                "refs/remotes/origin/topic",
                "refs/tags/v1"
            ]
        );
        assert_eq!(preferred_ref(&e, true), "remotes/origin/topic");
        assert_eq!(preferred_ref(&e, false), "topic");
        assert_eq!(
            delete_ref_command("refs/remotes/origin/topic")
                .unwrap()
                .join(" "),
            "push origin --delete refs/heads/topic"
        );
        assert_eq!(
            delete_ref_command("refs/tags/v1").unwrap().join(" "),
            "tag -d -- v1"
        );
        assert_eq!(parent(&e, 1), "p1");
        assert_eq!(parent(&e, 2), EMPTY_TREE);
        assert!(!is_wc(&e));
    }
}
