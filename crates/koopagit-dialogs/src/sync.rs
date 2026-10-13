//! `/command:sync` — `CSyncDlg` ("Git Synchronization").
//!
//! Implemented: Local/Remote branch, Remote URL, Force, the Pull split button
//! (Pull, Fetch, Fetch & Rebase, Fetch all refs, Remote Update, Clean up stale
//! remote branches), the Push split button (Push, Push tags), the git output
//! log and the In/Out commit lists. Not implemented: Push notes, Compare tags,
//! Ref List, Submodule/Stash split buttons, Apply/Email Patch (see
//! docs/divergences.md).

use std::cell::RefCell;
use std::io::Read;
use std::process::Stdio;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use koopagit_core::Repo;
use koopagit_core::extras::DiffOptions;
use koopagit_core::launcher::Invocation;
use koopagit_core::log::{LogEntry, LogOptions};
use koopagit_core::remote::{SyncOptions, SyncPull, sync_pull, sync_push};
use koopagit_core::settings::Settings;
use koopagit_core::status::StatusEntry;
use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::filelist::{self, Item};
use crate::filelist_actions::{FileListEnv, FileListHost, FileListMenu, LateRefresh};
use crate::filelist_menu::PopMask;
use crate::log_menu::{self, LogMask};
use crate::logactions::LogActions;
use crate::menus::MenuCache;
use crate::progress::OutputBuffer;
use crate::remote_util::{merged, remote_branches_of, save, strings, wd_key};
use crate::selection::{Mods, Selection};
use crate::{Context, DialogResult, FileListBridge, SyncCommit, SyncDialog, spawn_launcher};

type Answer = Box<dyn Fn(i32)>;
type OnDone = (Box<dyn Fn(bool)>, Rc<dyn Fn()>);

thread_local! {
    /// Completion handler of the running command (one at a time; the buttons
    /// are disabled while `running`).
    static ON_DONE: RefCell<Option<OnDone>> = const { RefCell::new(None) };
}

struct State {
    settings: Settings,
    output: Arc<Mutex<OutputBuffer>>,
    pending: Option<Answer>,
}

/// Push menu-button entries (IDS_PROC_SYNC_PUSH, IDS_PROC_SYNC_PUSH_TAGS).
const PUSH_ENTRIES: [&str; 2] = ["Pus&h", "Push ta&gs"];

/// Pull menu-button captions with their `&` access keys (SyncDlg.cpp:1174-1192).
fn pull_caption(kind: SyncPull) -> &'static str {
    match kind {
        SyncPull::Pull => "&Pull",
        SyncPull::Fetch => "Fetc&h",
        SyncPull::FetchRebase => "Fetch && Re&base",
        SyncPull::FetchAllRefs => "Fetch all refs",
        SyncPull::RemoteUpdate => "Remote Update",
        SyncPull::CleanupStale => "Clean up stale remote branches",
    }
}

/// Display labels and lower-case access letters ("" for none).
fn split_captions(captions: &[&str]) -> (Vec<String>, Vec<String>) {
    captions
        .iter()
        .map(|c| {
            let (label, key) = crate::msgbox::split_mnemonic(c);
            (label, key.map(String::from).unwrap_or_default())
        })
        .unzip()
}

fn to_rows(commits: &[LogEntry], sel: &Selection) -> ModelRc<SyncCommit> {
    let rows: Vec<SyncCommit> = commits
        .iter()
        .enumerate()
        .map(|(i, c)| SyncCommit {
            id: c.id.chars().take(8).collect::<String>().into(),
            subject: c.subject.clone().into(),
            author: c.author.name.clone().into(),
            date: crate::format::short_datetime(c.author.time, c.author.offset).into(),
            selected: sel.contains(i),
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

/// One of the In/Out Commits lists (a `CGitLogList` with the default mask).
#[derive(Default)]
struct CommitListState {
    entries: Vec<LogEntry>,
    sel: Selection,
    cache: MenuCache,
}

/// Commits in `exclude..include`, with parents and ref labels for the menu.
fn commits_in(repo: &Repo, exclude: &str, include: &str) -> Vec<LogEntry> {
    repo.log(&LogOptions {
        range: Some(format!("{exclude}..{include}")),
        ..Default::default()
    })
    .unwrap_or_default()
}

/// One In/Out ChangeList tab: its rows and file-list menu.
struct ChangeList {
    items: Rc<RefCell<Vec<Item>>>,
    model: Rc<VecModel<crate::FileRow>>,
    menu: Rc<FileListMenu>,
    /// `m_Rev1` / `m_Rev2` of the list.
    revs: RefCell<Option<(String, String)>>,
}

impl ChangeList {
    fn new(list_id: i32, workdir: &std::path::Path, refresh: LateRefresh) -> Self {
        let items: Rc<RefCell<Vec<Item>>> = Rc::default();
        let model = Rc::new(VecModel::<crate::FileRow>::default());
        let mut env = FileListEnv::working_tree(
            list_id,
            workdir,
            PopMask::SYNC_CHANGELIST,
            &crate::window_title(workdir, "Git Synchronization"),
        );
        // `Init(..., bHasWC = false)` (SyncDlg.cpp:1053-1055, 1102-1104).
        env.has_wc = false;
        let menu = {
            let items = items.clone();
            FileListMenu::new(
                env,
                FileListHost {
                    items: Box::new(move || items.borrow().clone()),
                    model: model.clone(),
                    refresh: Box::new(move || {
                        let f = refresh.borrow().clone();
                        if let Some(f) = f {
                            f();
                        }
                    }),
                },
            )
        };
        ChangeList {
            items,
            model,
            menu,
            revs: RefCell::new(None),
        }
    }

    /// `AddDiffFileList(list, rev1, rev2)`: the files changed from `rev2` to
    /// `rev1`; the menu's "Compare revisions" uses the same pair.
    fn fill(&self, repo: &Repo, rev1: Option<&str>, rev2: Option<&str>) {
        let (items, revs) = match (rev1, rev2) {
            (Some(r1), Some(r2)) => {
                let files = repo
                    .changed_files(r2, r1, &DiffOptions::default(), false)
                    .unwrap_or_default();
                (files, Some((r1.to_owned(), r2.to_owned())))
            }
            _ => (Vec::new(), None),
        };
        let stats: std::collections::HashMap<String, (u32, u32)> = items
            .iter()
            .filter(|f| f.has_stat)
            .map(|f| {
                (
                    f.path.clone(),
                    (f.added.unwrap_or(0), f.removed.unwrap_or(0)),
                )
            })
            .collect();
        let list: Vec<Item> = items
            .into_iter()
            .map(|f| Item {
                entry: StatusEntry {
                    path: f.path,
                    old_path: f.old_path,
                    staged: Some(f.action),
                    unstaged: None,
                },
                checked: false,
                is_dir: f.is_submodule,
            })
            .collect();
        self.model.set_vec(
            list.iter()
                .map(|i| filelist::to_row(i, &stats))
                .collect::<Vec<_>>(),
        );
        *self.items.borrow_mut() = list;
        *self.revs.borrow_mut() = revs.clone();
        self.menu.set_revisions(revs);
        self.menu.after_rebuild();
    }
}

fn merge_base(repo: &Repo, a: &str, b: &str) -> Option<String> {
    repo.git()
        .run(["merge-base", a, b])
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

pub fn run(ctx: &mut Context) -> DialogResult<i32> {
    let repo = &ctx.repo;
    let dlg = SyncDialog::new()?;
    dlg.set_dialog_title(crate::window_title(repo.workdir(), "Git Synchronization").into());

    let locals = repo.local_branches()?;
    let current = repo.current_branch()?;
    dlg.set_local_branches(strings(&locals));
    dlg.set_local_index(
        current
            .as_ref()
            .and_then(|c| locals.iter().position(|l| l == c))
            .unwrap_or(0) as i32,
    );
    let remotes = repo.remotes();
    let urls = merged(
        ctx.settings
            .history(&wd_key(r"History\SyncURL", repo), "url"),
        remotes.iter().cloned(),
    );
    dlg.set_url_items(strings(&urls));
    let tracked = current.as_deref().and_then(|b| repo.tracking_branch(b));
    let url = tracked
        .as_ref()
        .map(|t| t.0.clone())
        .or_else(|| urls.first().cloned())
        .unwrap_or_default();
    dlg.set_url(url.clone().into());
    let rb = tracked
        .map(|t| t.1)
        .or_else(|| current.clone())
        .unwrap_or_default();
    dlg.set_remote_branch(rb.into());
    dlg.set_remote_branch_items(strings(&remote_branches_of(repo, &url)));

    let labels: Vec<&str> = SyncPull::ALL.iter().map(|p| pull_caption(*p)).collect();
    let (pull, pull_keys) = split_captions(&labels);
    dlg.set_pull_entries(strings(&pull));
    dlg.set_pull_keys(strings(&pull_keys));
    let (push, push_keys) = split_captions(&PUSH_ENTRIES);
    dlg.set_push_entries(strings(&push));
    dlg.set_push_keys(strings(&push_keys));
    let sync_key = wd_key(r"TortoiseProc\Sync", repo);
    dlg.set_pull_choice(
        (ctx.settings.dword(&format!(r"{sync_key}\Pull"), 0) as usize).min(labels.len() - 1) as i32,
    );
    dlg.set_push_choice((ctx.settings.dword(&format!(r"{sync_key}\Push"), 0) as i32).min(1));

    let state = Rc::new(RefCell::new(State {
        settings: ctx.settings.clone(),
        output: Arc::new(Mutex::new(OutputBuffer::default())),
        pending: None,
    }));
    let workdir = repo.workdir().to_owned();
    let lists: Rc<[RefCell<CommitListState>; 2]> = Rc::new(Default::default());
    let repo_state = Rc::new(RefCell::new((repo.state(), repo.head_id().ok().flatten())));

    // In/Out ChangeList tabs with the file-list menu (SyncDlg mask).
    let changes_refresh: LateRefresh = Rc::default();
    let changes = Rc::new([
        ChangeList::new(1, &workdir, changes_refresh.clone()),
        ChangeList::new(2, &workdir, changes_refresh.clone()),
    ]);
    dlg.set_in_files(ModelRc::from(changes[0].model.clone()));
    dlg.set_out_files(ModelRc::from(changes[1].model.clone()));
    FileListMenu::install(
        &dlg.global::<FileListBridge>(),
        &[changes[0].menu.clone(), changes[1].menu.clone()],
    );
    {
        let changes = changes.clone();
        let workdir = workdir.clone();
        dlg.on_file_activated(move |which, row| {
            let which = which.clamp(0, 1) as usize;
            let item = changes[which].items.borrow().get(row as usize).cloned();
            let revs = changes[which].revs.borrow().clone();
            if let (Some(item), Some((r1, r2))) = (item, revs) {
                spawn_launcher(
                    &Invocation::new("diff")
                        .with_paths([workdir.join(&item.entry.path)])
                        .with("startrev", r2)
                        .with("endrev", r1),
                );
            }
        });
    }

    let refresh = {
        let weak = dlg.as_weak();
        let workdir = workdir.clone();
        let lists = lists.clone();
        let repo_state = repo_state.clone();
        let changes = changes.clone();
        Rc::new(move || {
            let Some(d) = weak.upgrade() else { return };
            let Ok(repo) = Repo::discover(&workdir) else {
                return;
            };
            let opts = options(&d, &repo);
            let remote_ref = format!("remotes/{}/{}", opts.url, opts.remote_branch);
            let ok = !opts.remote_branch.is_empty() && opts.tracking_ref_exists;
            let (incoming, outgoing) = if ok && !opts.local_branch.is_empty() {
                (
                    commits_in(&repo, &opts.local_branch, &remote_ref),
                    commits_in(&repo, &remote_ref, &opts.local_branch),
                )
            } else {
                (Vec::new(), Vec::new())
            };
            d.set_out_summary(
                if ok {
                    format!(
                        "{} commits ahead \"{}/{}\"",
                        outgoing.len(),
                        opts.url,
                        opts.remote_branch
                    )
                } else {
                    String::new()
                }
                .into(),
            );
            *repo_state.borrow_mut() = (repo.state(), repo.head_id().ok().flatten());
            for (list, commits) in lists.iter().zip([incoming, outgoing]) {
                let mut l = list.borrow_mut();
                l.entries = commits;
                l.sel = Selection::default();
                l.cache.invalidate();
            }
            d.set_in_commits(to_rows(&lists[0].borrow().entries, &Selection::default()));
            d.set_out_commits(to_rows(&lists[1].borrow().entries, &Selection::default()));
            // In: remote vs. the merge base; Out: local vs. the remote (fast
            // forward) or the merge base (SyncDlg.cpp:537, 1436-1442).
            let (in_revs, out_revs) = if ok && !opts.local_branch.is_empty() {
                let base = merge_base(&repo, &opts.local_branch, &remote_ref);
                (
                    (Some(remote_ref.clone()), base.clone()),
                    (Some(opts.local_branch.clone()), base),
                )
            } else {
                ((None, None), (None, None))
            };
            changes[0].fill(&repo, in_revs.0.as_deref(), in_revs.1.as_deref());
            changes[1].fill(&repo, out_revs.0.as_deref(), out_revs.1.as_deref());
            d.set_remote_branch_items(strings(&remote_branches_of(&repo, &opts.url)));
        })
    };
    *changes_refresh.borrow_mut() = Some(refresh.clone());
    refresh();
    // Like upstream, open on the outgoing commits when there are any.
    if dlg.get_out_commits().row_count() > 0 {
        dlg.set_current_tab(3);
    }
    {
        let refresh = refresh.clone();
        dlg.on_refs_changed(move || refresh());
    }
    for (cb, command) in [("manage", "settings"), ("log", "log"), ("commit", "commit")] {
        let workdir = workdir.clone();
        let f = move || {
            let mut inv = Invocation::new(command).with_paths([workdir.clone()]);
            if cb == "manage" {
                inv = inv.with("page", "gitremote");
            }
            spawn_launcher(&inv);
        };
        match cb {
            "manage" => dlg.on_manage(f),
            "log" => dlg.on_show_log(f),
            _ => dlg.on_commit(f),
        }
    }
    // In/Out Commits: selection and the log-list context menu.
    let show_list = {
        let weak = dlg.as_weak();
        let lists = lists.clone();
        Rc::new(move |which: usize| {
            let Some(d) = weak.upgrade() else { return };
            let l = lists[which].borrow();
            let rows = to_rows(&l.entries, &l.sel);
            if which == 0 {
                d.set_in_commits(rows);
            } else {
                d.set_out_commits(rows);
            }
        })
    };
    {
        let lists = lists.clone();
        let show_list = show_list.clone();
        dlg.on_commit_pressed(move |which, row, toggle, range| {
            let which = which.clamp(0, 1) as usize;
            {
                let mut l = lists[which].borrow_mut();
                l.sel.click(row as usize, Mods { toggle, range });
                l.cache.invalidate();
            }
            show_list(which);
        });
    }
    {
        let lists = lists.clone();
        let show_list = show_list.clone();
        let repo_state = repo_state.clone();
        dlg.on_commit_menu(move |which, row| {
            let which = which.clamp(0, 1) as usize;
            let changed = {
                let mut l = lists[which].borrow_mut();
                if l.sel.contains(row as usize) {
                    false
                } else {
                    l.sel.right_click(row as usize);
                    l.cache.invalidate();
                    true
                }
            };
            if changed {
                let show = show_list.clone();
                slint::Timer::single_shot(std::time::Duration::ZERO, move || show(which));
            }
            let l = lists[which].borrow();
            let rs = repo_state.borrow();
            l.cache.get(row, || {
                let rows: Vec<&LogEntry> = l.entries.iter().collect();
                log_menu::build(&crate::log::menu_input_for(
                    &rows,
                    &l.sel,
                    rs.1.as_deref(),
                    &rs.0,
                    LogMask::sync(),
                ))
            })
        });
    }
    {
        let lists = lists.clone();
        let actions = Rc::new(LogActions {
            workdir: workdir.clone(),
            file: None,
            log_path: workdir.clone(),
            relative_dates: false,
            reload: {
                let refresh = refresh.clone();
                Rc::new(move || refresh())
            },
            find: Rc::new(|| {}),
            show_text: Rc::new(|text: String| {
                crate::logactions::confirm("Copy", &text, &["OK"], |_| {});
            }),
            reflog: false,
        });
        dlg.on_commit_action(move |which, _row, id| {
            let Some((action, k)) = log_menu::decode(id) else {
                return;
            };
            let which = which.clamp(0, 1) as usize;
            let sel: Vec<LogEntry> = {
                let l = lists[which].borrow();
                l.sel
                    .rows()
                    .filter_map(|r| l.entries.get(r).cloned())
                    .collect()
            };
            actions.run(action, k, &sel);
        });
    }
    {
        let weak = dlg.as_weak();
        dlg.on_close_dialog(move || {
            if let Some(d) = weak.upgrade() {
                let _ = d.hide();
            }
        });
    }
    {
        let state = state.clone();
        let weak = dlg.as_weak();
        dlg.on_notice_clicked(move |i| {
            let answer = state.borrow_mut().pending.take();
            if let Some(d) = weak.upgrade() {
                d.set_notice_text("".into());
                d.set_notice_buttons(strings::<&str>(&[]));
            }
            if let Some(f) = answer {
                f(i);
            }
        });
    }
    {
        let ctl = Controller {
            weak: dlg.as_weak(),
            state: state.clone(),
            workdir: workdir.clone(),
            refresh: refresh.clone(),
        };
        let ctl = Rc::new(ctl);
        let c = ctl.clone();
        dlg.on_pull(move || c.pull(false));
        let c = ctl.clone();
        dlg.on_push(move || c.push(false));
    }

    crate::snapshot::arm(&dlg);
    dlg.run()?;

    let mut st = state.borrow_mut();
    st.settings
        .set(&format!(r"{sync_key}\Pull"), dlg.get_pull_choice());
    st.settings
        .set(&format!(r"{sync_key}\Push"), dlg.get_push_choice());
    st.settings.push_history(
        &wd_key(r"History\SyncURL", repo),
        "url",
        dlg.get_url().trim(),
        25,
    );
    save(&st.settings);
    Ok(0)
}

fn options(d: &SyncDialog, repo: &Repo) -> SyncOptions {
    let locals = repo.local_branches().unwrap_or_default();
    let url = d.get_url().trim().to_owned();
    let remote_branch = d.get_remote_branch().trim().to_owned();
    let is_named_remote = repo.remotes().contains(&url);
    SyncOptions {
        local_branch: locals
            .get(d.get_local_index().max(0) as usize)
            .cloned()
            .unwrap_or_default(),
        tracking_ref_exists: is_named_remote && repo.has_remote_tracking_ref(&url, &remote_branch),
        url,
        remote_branch,
        force: d.get_force(),
        verbose: true,
        is_named_remote,
    }
}

struct Controller {
    weak: slint::Weak<SyncDialog>,
    state: Rc<RefCell<State>>,
    workdir: std::path::PathBuf,
    refresh: Rc<dyn Fn()>,
}

impl Controller {
    fn ask(self: &Rc<Self>, text: &str, buttons: &[&str], answer: Answer) {
        let Some(d) = self.weak.upgrade() else { return };
        self.state.borrow_mut().pending = Some(answer);
        d.set_notice_text(text.into());
        d.set_notice_buttons(strings(buttons));
        d.set_notice_error(false);
    }

    fn error(&self, text: &str) {
        if let Some(d) = self.weak.upgrade() {
            d.set_notice_text(text.into());
            d.set_notice_buttons(strings::<&str>(&[]));
            d.set_notice_error(true);
        }
    }

    /// Offers to configure tracking (`AskSetTrackedBranch`, default on).
    /// Returns `true` when the action should wait for the user's answer.
    fn maybe_ask_tracking(
        self: &Rc<Self>,
        repo: &Repo,
        o: &SyncOptions,
        resume: Rc<dyn Fn()>,
    ) -> bool {
        let ask = self
            .state
            .borrow()
            .settings
            .bool("AskSetTrackedBranch", true);
        if !ask
            || !o.is_named_remote
            || o.remote_branch.is_empty()
            || o.local_branch.is_empty()
            || repo.tracking_branch(&o.local_branch).is_some()
        {
            return false;
        }
        let text = format!(
            "For the current branch \"{}\" no tracked branch is configured.\nDo you want to set the selected branch \"{}/{}\" as tracked branch?\nThis will automatically select it for push and pull in the future.",
            o.local_branch, o.url, o.remote_branch
        );
        let (workdir, o) = (self.workdir.clone(), o.clone());
        self.ask(
            &text,
            &["Yes", "No"],
            Box::new(move |i| {
                if i == 0 {
                    if let Ok(repo) = Repo::discover(&workdir) {
                        let _ = repo
                            .config_set_local(&format!("branch.{}.remote", o.local_branch), &o.url);
                        let _ = repo.config_set_local(
                            &format!("branch.{}.merge", o.local_branch),
                            &format!("refs/heads/{}", o.remote_branch),
                        );
                    }
                }
                resume();
            }),
        );
        true
    }

    fn pull(self: &Rc<Self>, tracking_answered: bool) {
        let Some(d) = self.weak.upgrade() else { return };
        let Ok(repo) = Repo::discover(&self.workdir) else {
            return;
        };
        let mut o = options(&d, &repo);
        let mut kind = SyncPull::ALL[d.get_pull_choice().clamp(0, 5) as usize];
        if o.url.is_empty() {
            self.error("Remote URL must not be empty.");
            return;
        }
        if kind == SyncPull::Pull {
            let current = repo.current_branch().ok().flatten().unwrap_or_default();
            if o.local_branch != current {
                let me = self.clone();
                let branch = o.local_branch.clone();
                self.ask(
                    "Pull requires the selected local branch to be the current branch.\nDo you want to switch to the selected branch?",
                    &[&format!("Switch to \"{branch}\""), "Abort"],
                    Box::new(move |i| {
                        if i == 0 {
                            let args = vec![
                                "checkout".to_owned(),
                                "--end-of-options".to_owned(),
                                branch.clone(),
                                "--".to_owned(),
                            ];
                            let me2 = me.clone();
                            me.run_git(vec![args], Box::new(move |ok| if ok { me2.pull(false) }));
                        }
                    }),
                );
                return;
            }
            if !tracking_answered {
                let me = self.clone();
                if self.maybe_ask_tracking(&repo, &o, Rc::new(move || me.pull(true))) {
                    return;
                }
            }
            let rebase_cfg = repo
                .config_get(&format!("branch.{current}.rebase"))
                .or_else(|| repo.config_get("pull.rebase"));
            if matches!(rebase_cfg.as_deref(), Some("true" | "merges")) {
                kind = SyncPull::FetchRebase;
            } else if o.is_named_remote
                && repo
                    .tracking_branch(&o.local_branch)
                    .is_some_and(|(r, b)| r == o.url && b == o.remote_branch)
            {
                o.remote_branch.clear();
            }
        }
        let args = sync_pull(kind, &o);
        let upstream = if o.is_named_remote && !o.remote_branch.is_empty() {
            format!("remotes/{}/{}", o.url, o.remote_branch)
        } else {
            "FETCH_HEAD".to_owned()
        };
        let workdir = self.workdir.clone();
        self.run_git(
            vec![args],
            Box::new(move |ok| {
                if ok && kind == SyncPull::FetchRebase {
                    spawn_launcher(
                        &Invocation::new("rebase")
                            .with_paths([workdir.clone()])
                            .with("upstream", upstream.clone()),
                    );
                }
            }),
        );
    }

    fn push(self: &Rc<Self>, tracking_answered: bool) {
        let Some(d) = self.weak.upgrade() else { return };
        let Ok(repo) = Repo::discover(&self.workdir) else {
            return;
        };
        let o = options(&d, &repo);
        if o.url.is_empty() {
            self.error("Remote URL must not be empty.");
            return;
        }
        let tags = d.get_push_choice() == 1;
        if !tags && !tracking_answered {
            let me = self.clone();
            if self.maybe_ask_tracking(&repo, &o, Rc::new(move || me.push(true))) {
                return;
            }
        }
        self.run_git(vec![sync_push(&o, tags)], Box::new(|_| {}));
    }

    /// Runs git commands, streaming into the Log tab; `done` runs on the UI thread.
    fn run_git(self: &Rc<Self>, commands: Vec<Vec<String>>, done: Box<dyn Fn(bool)>) {
        let Some(d) = self.weak.upgrade() else { return };
        d.set_running(true);
        d.set_current_tab(0);
        let output = self.state.borrow().output.clone();
        let weak = self.weak.clone();
        let workdir = self.workdir.clone();
        let refresh = self.refresh.clone();
        // `done`/`refresh` are not Send: park them on the UI thread until the command exits.
        ON_DONE.with(|slot| *slot.borrow_mut() = Some((done, refresh)));
        std::thread::spawn(move || {
            let git = koopagit_core::git_cli::Git::new(&workdir);
            let mut ok = true;
            for args in &commands {
                output
                    .lock()
                    .unwrap()
                    .push_line(&format!("git {}", args.join(" ")));
                push_text(&weak, &output);
                let mut cmd = git.command(args);
                cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
                let mut child = match cmd.spawn() {
                    Ok(c) => c,
                    Err(e) => {
                        output.lock().unwrap().push_line(&format!("error: {e}"));
                        ok = false;
                        break;
                    }
                };
                let readers: Vec<_> = [
                    child
                        .stdout
                        .take()
                        .map(|s| Box::new(s) as Box<dyn Read + Send>),
                    child
                        .stderr
                        .take()
                        .map(|s| Box::new(s) as Box<dyn Read + Send>),
                ]
                .into_iter()
                .flatten()
                .map(|mut r| {
                    let output = output.clone();
                    let weak = weak.clone();
                    std::thread::spawn(move || {
                        let mut buf = [0u8; 4096];
                        while let Ok(n) = r.read(&mut buf) {
                            if n == 0 {
                                break;
                            }
                            output
                                .lock()
                                .unwrap()
                                .push_str(&String::from_utf8_lossy(&buf[..n]));
                            push_text(&weak, &output);
                        }
                    })
                })
                .collect();
                for r in readers {
                    let _ = r.join();
                }
                let code = child.wait().ok().and_then(|s| s.code());
                let line = if code == Some(0) {
                    "Success".to_owned()
                } else {
                    format!(
                        "git did not exit cleanly (exit code {})",
                        code.map_or_else(|| "?".to_owned(), |c| c.to_string())
                    )
                };
                output.lock().unwrap().push_line(&line);
                push_text(&weak, &output);
                if code != Some(0) {
                    ok = false;
                    break;
                }
            }
            let _ = weak.upgrade_in_event_loop(move |d| {
                d.set_running(false);
                if let Some((done, refresh)) = ON_DONE.with(|slot| slot.borrow_mut().take()) {
                    refresh();
                    done(ok);
                }
            });
        });
    }
}

fn push_text(weak: &slint::Weak<SyncDialog>, output: &Arc<Mutex<OutputBuffer>>) {
    let text = output.lock().unwrap().text();
    let _ = weak.upgrade_in_event_loop(move |d| d.set_output(text.into()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use koopagit_core::git_cli::Git;

    #[test]
    fn changelist_lists_files_since_merge_base() {
        let dir = tempfile::tempdir().unwrap();
        let git = Git::new(dir.path());
        git.run(["init", "-q", "-b", "main"]).unwrap();
        git.run(["config", "user.name", "T"]).unwrap();
        git.run(["config", "user.email", "t@e"]).unwrap();
        std::fs::write(dir.path().join("a.txt"), "a\n").unwrap();
        git.run(["add", "."]).unwrap();
        git.run(["commit", "-qm", "base"]).unwrap();
        git.run(["branch", "remote-side"]).unwrap();
        std::fs::write(dir.path().join("a.txt"), "a\nb\n").unwrap();
        std::fs::write(dir.path().join("n.txt"), "n\n").unwrap();
        git.run(["add", "."]).unwrap();
        git.run(["commit", "-qm", "local"]).unwrap();
        let repo = Repo::discover(dir.path()).unwrap();

        let refresh: LateRefresh = Rc::default();
        let list = ChangeList::new(2, repo.workdir(), refresh);
        let base = merge_base(&repo, "main", "remote-side").unwrap();
        list.fill(&repo, Some("main"), Some(&base));
        let paths: Vec<String> = list
            .items
            .borrow()
            .iter()
            .map(|i| i.entry.path.clone())
            .collect();
        assert_eq!(paths, ["a.txt", "n.txt"]);
        assert_eq!(list.model.row_count(), 2);
        assert_eq!(
            list.revs.borrow().clone(),
            Some(("main".to_owned(), base.clone()))
        );
        list.fill(&repo, None, None);
        assert_eq!(list.model.row_count(), 0);
        assert!(list.revs.borrow().is_none());
    }
}
