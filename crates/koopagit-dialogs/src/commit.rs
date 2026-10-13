//! `/command:commit` — `CCommitDlg` and `CAppUtils::Commit`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use koopagit_core::commit::{self, AuthorDate, CommitOptions, CommitRow};
use koopagit_core::launcher::CloseOnEnd;
use koopagit_core::settings::{keys, workdir_key};
use koopagit_core::status::{Action, StatusEntry, StatusOptions};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::filelist::{self, CheckLink, Item};
use crate::filelist_actions::{FileListEnv, FileListHost, FileListMenu};
use crate::filelist_menu::PopMask;
use crate::progress::{PostAction, ProgressJob};
use crate::{CommitDialog, Context, DialogResult, FileListBridge, msgbox, spawn_launcher};

/// The split-button entries (`CommitLastAction`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommitAction {
    Commit,
    ReCommit,
    CommitPush,
}

impl CommitAction {
    fn from_index(i: i32) -> Self {
        match i {
            1 => CommitAction::ReCommit,
            2 => CommitAction::CommitPush,
            _ => CommitAction::Commit,
        }
    }
}

/// What the dialog decided to do.
struct Decision {
    action: CommitAction,
    job: ProgressJob,
    message: String,
    amend: bool,
    message_file: std::path::PathBuf,
}

/// Values that survive a ReCommit / failed commit round trip.
#[derive(Default, Clone)]
struct Carry {
    message: Option<String>,
}

/// Pending answer to a question shown in the notice bar.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pending {
    None,
    Conflicts,
    SignOff,
    Template,
    Cancel,
}

struct State {
    items: Vec<Item>,
    numstat: HashMap<String, (u32, u32)>,
    amend_message: String,
    no_amend_message: String,
    template: String,
    pending: Pending,
    /// Validation steps already confirmed by the user for this OK click.
    confirmed: Vec<Pending>,
    decision: Option<Decision>,
}

pub fn run(ctx: &mut Context) -> DialogResult<i32> {
    if !check_user_data(ctx) {
        return Ok(1);
    }
    let inv = &ctx.invocation;
    if inv.has("logmsg") && inv.has("logmsgfile") {
        msgbox::error(
            "KoopaGit",
            "Both /logmsg and /logmsgfile were specified!\nOnly one of those can be specified.",
        );
        return Ok(1);
    }
    let mut carry = Carry {
        message: match (inv.get("logmsg"), inv.get("logmsgfile")) {
            (Some(m), _) => Some(m.to_owned()),
            (_, Some(f)) => std::fs::read_to_string(f).ok(),
            _ => None,
        },
    };
    loop {
        let Some(decision) = show(ctx, &carry)? else {
            return Ok(1);
        };
        let ok = crate::progress::run(decision.job)?;
        let _ = std::fs::remove_file(&decision.message_file);
        if !ok {
            // A failed commit keeps the dialog open with the message.
            carry.message = Some(decision.message);
            continue;
        }
        crate::hookrun::post_commit(ctx.repo.workdir(), decision.amend);
        let max = ctx.settings.dword(keys::MAX_HISTORY_ITEMS, 25) as usize;
        let key = format!(r"History\commit{}", workdir_key(ctx.repo.workdir()));
        ctx.settings
            .push_history(&key, "logmsgs", decision.message.trim(), max);
        let _ = ctx.settings.save();
        match decision.action {
            CommitAction::Commit => return Ok(0),
            CommitAction::ReCommit => carry.message = None,
            CommitAction::CommitPush => {
                push_after_commit(ctx, decision.amend)?;
                return Ok(0);
            }
        }
    }
}

/// `CheckUserData`: user.name and user.email must be set.
fn check_user_data(ctx: &Context) -> bool {
    let (name, email) = ctx.repo.user_identity();
    if !name.is_empty() && !email.is_empty() {
        return true;
    }
    if msgbox::ask(
        "KoopaGit",
        "User name and email must be set before commit.\nDo you want to set these now?",
        &["Yes", "No"],
    ) == Some(0)
    {
        spawn_launcher(&ctx.launch("settings").with("page", "gitconfig"));
    }
    false
}

/// Commit & Push: push directly when an upstream is configured and not amending,
/// otherwise open the Push dialog.
fn push_after_commit(ctx: &Context, amend: bool) -> DialogResult {
    let branch = ctx.repo.current_branch()?.unwrap_or_default();
    let git = ctx.repo.git();
    let remote = git
        .run(["config", "--get", &format!("branch.{branch}.remote")])
        .map(|s| s.trim().to_owned())
        .unwrap_or_default();
    let merge = git
        .run(["config", "--get", &format!("branch.{branch}.merge")])
        .map(|s| s.trim().trim_start_matches("refs/heads/").to_owned())
        .unwrap_or_default();
    if amend || remote.is_empty() || merge.is_empty() {
        spawn_launcher(&ctx.launch("push"));
        return Ok(());
    }
    let cmd = koopagit_core::commands::push(&koopagit_core::commands::PushOptions {
        remote,
        local: branch,
        remote_branch: merge,
        ..Default::default()
    });
    crate::progress::run(ProgressJob {
        title: ctx.window_title("Push"),
        git: ctx.repo.git(),
        commands: cmd,
        post_actions: vec![],
        close_on_end: CloseOnEnd::IfNoErrors,
    })?;
    Ok(())
}

fn whole_project_forced(ctx: &Context) -> bool {
    ctx.rel_paths().is_empty()
}

fn whole_project_key(ctx: &Context) -> String {
    format!(
        r"TortoiseProc\ShowWholeProject\{}",
        workdir_key(ctx.repo.workdir())
    )
}

/// Loads the file list (`StatusThread`): changes under the selected paths plus
/// every staged change, optionally unversioned files; in amend mode also the
/// files of the last commit.
fn load_items(ctx: &Context, show_unversioned: bool, whole: bool, amend: bool) -> Vec<Item> {
    let filters = if whole { vec![] } else { ctx.rel_paths() };
    let entries = ctx
        .repo
        .status(
            StatusOptions {
                unversioned: show_unversioned,
                ignored: false,
            },
            &[],
        )
        .unwrap_or_default();
    let in_filter = |p: &str| {
        filters.is_empty()
            || filters
                .iter()
                .any(|f| p == f || p.starts_with(&format!("{f}/")))
    };
    let mut entries: Vec<StatusEntry> = entries
        .into_iter()
        .filter(|e| in_filter(&e.path) || e.staged.is_some())
        .collect();
    if amend {
        if let Ok(out) = ctx.repo.git().run([
            "diff-tree",
            "--no-commit-id",
            "--name-status",
            "-r",
            "-z",
            "HEAD",
        ]) {
            let mut it = out.split('\0');
            while let (Some(st), Some(path)) = (it.next(), it.next()) {
                if path.is_empty() || entries.iter().any(|e| e.path == path) {
                    continue;
                }
                let action = match st.chars().next() {
                    Some('A') => Action::Added,
                    Some('D') => Action::Deleted,
                    _ => Action::Modified,
                };
                entries.push(StatusEntry {
                    path: path.to_owned(),
                    old_path: None,
                    staged: Some(action),
                    unstaged: None,
                });
            }
            entries.sort_by(|a, b| a.path.cmp(&b.path));
        }
    }
    let select = ctx.settings.bool(keys::SELECT_FILES_FOR_COMMIT, true);
    let skip_missing = ctx.settings.bool(keys::AUTOSELECT_MISSING_FILES, false);
    entries
        .into_iter()
        .map(|entry| {
            let action = entry.action();
            let is_dir = ctx.repo.workdir().join(&entry.path).is_dir();
            let checked = select
                && action != Action::Unversioned
                && !(skip_missing && action == Action::Missing);
            Item {
                entry,
                checked,
                is_dir,
            }
        })
        .collect()
}

fn show(ctx: &Context, carry: &Carry) -> DialogResult<Option<Decision>> {
    let dlg = CommitDialog::new()?;
    let repo = &ctx.repo;
    let forced_whole = whole_project_forced(ctx);
    let whole = forced_whole || ctx.settings.bool(&whole_project_key(ctx), false);
    let show_unversioned = ctx.settings.bool("AddBeforeCommit", true);
    let has_head = repo.head_id()?.is_some();
    let merge_active = repo.merge_active();

    dlg.set_dialog_title(
        crate::window_title(
            &if whole {
                repo.workdir().to_owned()
            } else {
                ctx.title_path()
            },
            "Commit",
        )
        .into(),
    );
    dlg.set_branch(
        repo.current_branch()?
            .unwrap_or_else(|| "(no branch)".to_owned())
            .into(),
    );
    dlg.set_merge_active(merge_active);
    dlg.set_whole_project(whole);
    dlg.set_whole_project_enabled(!forced_whole);
    dlg.set_show_unversioned(show_unversioned);
    dlg.set_amend_enabled(has_head);
    let (name, email) = repo.user_identity();
    dlg.set_author(format!("{name} <{email}>").into());
    dlg.set_date_text(crate::format::iso_datetime(crate::format::now(), 0).into());
    dlg.set_commit_action(ctx.settings.dword("CommitLastAction", 0).min(2) as i32);

    let template = repo.initial_commit_message();
    let initial = carry.message.clone().unwrap_or_else(|| template.clone());
    let initial = crate::hookrun::start_commit(repo.workdir(), &ctx.rel_paths(), initial);
    dlg.set_message(initial.clone().into());

    let state = Rc::new(RefCell::new(State {
        items: load_items(ctx, show_unversioned, whole, false),
        numstat: if has_head {
            repo.numstat("HEAD")
        } else {
            HashMap::new()
        },
        amend_message: String::new(),
        no_amend_message: String::new(),
        template,
        pending: Pending::None,
        confirmed: Vec::new(),
        decision: None,
    }));

    let model = Rc::new(VecModel::<crate::FileRow>::default());
    dlg.set_rows(ModelRc::from(model.clone()));

    // File-list context menu and multi-selection (spec-menus §1, CommitDlg mask).
    let list_menu = {
        let items_state = state.clone();
        let weak = dlg.as_weak();
        FileListMenu::new(
            FileListEnv::working_tree(
                0,
                repo.workdir(),
                PopMask::COMMIT_DLG,
                &ctx.window_title("Commit"),
            ),
            FileListHost {
                items: Box::new(move || items_state.borrow().items.clone()),
                model: model.clone(),
                refresh: Box::new(move || {
                    if let Some(d) = weak.upgrade() {
                        d.invoke_refresh();
                    }
                }),
            },
        )
    };
    FileListMenu::install(
        &dlg.global::<FileListBridge>(),
        std::slice::from_ref(&list_menu),
    );
    wire_list_keys(&dlg, &list_menu);
    // Message editor context menu (worker C).
    {
        let key = format!(r"History\commit{}", workdir_key(repo.workdir()));
        let state = state.clone();
        crate::commit_message_menu::wire(
            &dlg,
            repo.workdir().to_owned(),
            ctx.settings.history(&key, "logmsgs"),
            Rc::new(move || {
                state
                    .borrow()
                    .items
                    .iter()
                    .filter(|i| i.checked)
                    .map(|i| {
                        (
                            i.entry.action().name().to_owned(),
                            filelist::path_text(&i.entry),
                        )
                    })
                    .collect()
            }),
        );
    }

    let sync = {
        let state = state.clone();
        let model = model.clone();
        let list_menu = list_menu.clone();
        let weak = dlg.as_weak();
        let min_size = repo
            .git()
            .run(["config", "--get", "tgit.logminsize"])
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0usize);
        Rc::new(move |rebuild: bool| {
            let Some(d) = weak.upgrade() else { return };
            let st = state.borrow();
            if rebuild {
                let rows: Vec<crate::FileRow> = st
                    .items
                    .iter()
                    .map(|i| filelist::to_row(i, &st.numstat))
                    .collect();
                model.set_vec(rows);
                list_menu.after_rebuild();
            }
            d.set_stats_text(filelist::stats_text(&st.items).into());
            let enabled: Vec<bool> = CheckLink::ALL
                .iter()
                .map(|l| st.items.iter().any(|i| l.matches(i)))
                .collect();
            d.set_link_enabled(ModelRc::new(VecModel::from(enabled)));
            let any_checked = st.items.iter().any(|i| i.checked);
            d.set_ok_enabled(commit::can_commit(
                d.get_message().as_str(),
                min_size,
                d.get_message_only(),
                any_checked,
                false,
                merge_active,
            ));
        })
    };
    sync(true);

    if state.borrow().items.is_empty() && !merge_active && !show_unversioned {
        let has_unversioned = !repo
            .status(StatusOptions::with_unversioned(), &ctx.rel_paths())
            .unwrap_or_default()
            .is_empty();
        if has_unversioned {
            notice(
                &dlg,
                &state,
                Pending::None,
                "No files were changed or added since\nthe last commit.\nDo you want to see the unversioned files?",
                &[],
                false,
            );
        }
    }

    {
        let sync = sync.clone();
        dlg.on_options_changed(move || sync(false));
    }
    {
        let state = state.clone();
        let sync = sync.clone();
        dlg.on_row_toggled(move |i, checked| {
            if let Some(item) = state.borrow_mut().items.get_mut(i as usize) {
                item.checked = checked;
            }
            sync(false);
        });
    }
    {
        let state = state.clone();
        let sync = sync.clone();
        dlg.on_check_links(move |i, shift| {
            if let Some(link) = CheckLink::ALL.get(i as usize) {
                link.apply(&mut state.borrow_mut().items, shift);
            }
            sync(true);
        });
    }
    {
        let state = state.clone();
        let workdir = repo.workdir().to_owned();
        dlg.on_row_activated(move |i| {
            if let Some(item) = state.borrow().items.get(i as usize) {
                spawn_launcher(
                    &koopagit_core::launcher::Invocation::new("diff")
                        .with_paths([workdir.join(&item.entry.path)]),
                );
            }
        });
    }
    {
        let state = state.clone();
        let sync = sync.clone();
        let weak = dlg.as_weak();
        let ctx_paths = ctx.paths.clone();
        let repo_dir = repo.workdir().to_owned();
        let settings = ctx.settings.clone();
        dlg.on_refresh(move || {
            let Some(d) = weak.upgrade() else { return };
            let Ok(repo) = koopagit_core::Repo::discover(&repo_dir) else {
                return;
            };
            let tmp = Context {
                repo,
                settings: settings.clone(),
                invocation: koopagit_core::launcher::Invocation::new("commit"),
                paths: ctx_paths.clone(),
            };
            let old: HashMap<String, bool> = state
                .borrow()
                .items
                .iter()
                .map(|i| (i.entry.path.clone(), i.checked))
                .collect();
            let mut items = load_items(
                &tmp,
                d.get_show_unversioned(),
                d.get_whole_project(),
                d.get_amend(),
            );
            for item in &mut items {
                if let Some(c) = old.get(&item.entry.path) {
                    item.checked = *c;
                }
            }
            let base = if d.get_amend() { "HEAD~1" } else { "HEAD" };
            let mut st = state.borrow_mut();
            st.items = items;
            st.numstat = tmp.repo.numstat(base);
            drop(st);
            sync(true);
        });
    }
    {
        let state = state.clone();
        let weak = dlg.as_weak();
        let list_menu = list_menu.clone();
        let head_msg = repo.head_message().unwrap_or_default();
        let head_author = repo
            .git()
            .run(["log", "-1", "--format=%an <%ae>%n%at", "HEAD"])
            .unwrap_or_default();
        let author_default = format!("{name} <{email}>");
        dlg.on_amend_toggled(move || {
            let Some(d) = weak.upgrade() else { return };
            let mut st = state.borrow_mut();
            let mut lines = head_author.lines();
            if d.get_amend() {
                if st.amend_message.is_empty() {
                    st.amend_message = head_msg.clone();
                }
                st.no_amend_message = d.get_message().into();
                d.set_message(st.amend_message.clone().into());
                if let Some(a) = lines.next() {
                    d.set_author(a.into());
                }
                if let Some(t) = lines.next().and_then(|t| t.parse::<i64>().ok()) {
                    d.set_date_text(crate::format::iso_datetime(t, 0).into());
                }
            } else {
                st.amend_message = d.get_message().into();
                d.set_message(st.no_amend_message.clone().into());
                d.set_author(author_default.clone().into());
            }
            drop(st);
            list_menu.set_amend(d.get_amend());
            d.invoke_refresh();
        });
    }
    {
        let weak = dlg.as_weak();
        let (n, e) = (name.clone(), email.clone());
        let sync = sync.clone();
        dlg.on_sign_off(move || {
            if let Some(d) = weak.upgrade() {
                d.set_message(commit::add_sign_off(d.get_message().as_str(), &n, &e).into());
                sync(false);
            }
        });
    }
    {
        let state = state.clone();
        let weak = dlg.as_weak();
        dlg.on_cancel(move || {
            let Some(d) = weak.upgrade() else { return };
            let st = state.borrow();
            let changed = d.get_message().as_str().trim() != st.template.trim();
            if (changed || !st.items.is_empty()) && st.pending != Pending::Cancel {
                drop(st);
                notice(
                    &d,
                    &state,
                    Pending::Cancel,
                    "Are you sure you want to cancel?",
                    &["Yes", "No"],
                    false,
                );
                return;
            }
            let _ = d.hide();
        });
    }

    let commit_ctx = CommitInputs {
        workdir: repo.workdir().to_owned(),
        title: ctx.window_title("Commit"),
        has_head,
        name: name.clone(),
        email: email.clone(),
        warn_no_signoff: repo
            .git()
            .run(["config", "--bool", "--get", "tgit.warnnosignedoffby"])
            .is_ok_and(|v| v.trim() == "true"),
        strip_comments: ctx.settings.bool(keys::STRIP_COMMENTED_LINES, false),
        merge_active,
        is_git_svn: repo.common_dir().join("svn").exists(),
        bugtraq: repo.bugtraq(),
    };
    // Bug-ID field, shown only when `bugtraq.message` is set.
    dlg.set_bug_id_visible(commit_ctx.bugtraq.enabled());
    dlg.set_bug_id_label(commit_ctx.bugtraq.label.as_str().into());
    if let Some(id) = ctx.invocation.get("bugid") {
        dlg.set_bug_id(id.into());
    }
    let commit_ctx = Rc::new(commit_ctx);
    {
        let state = state.clone();
        let weak = dlg.as_weak();
        let ci = commit_ctx.clone();
        dlg.on_commit(move || {
            if let Some(d) = weak.upgrade() {
                state.borrow_mut().confirmed.clear();
                try_commit(&d, &state, &ci);
            }
        });
    }
    {
        let state = state.clone();
        let weak = dlg.as_weak();
        let ci = commit_ctx.clone();
        dlg.on_notice_clicked(move |i| {
            let Some(d) = weak.upgrade() else { return };
            let pending = state.borrow().pending;
            clear_notice(&d, &state);
            match (pending, i) {
                (Pending::Cancel, 0) => {
                    let _ = d.hide();
                }
                (Pending::Conflicts, 1) | (Pending::Template, 0) | (Pending::SignOff, 1) => {
                    state.borrow_mut().confirmed.push(pending);
                    try_commit(&d, &state, &ci);
                }
                (Pending::SignOff, 0) => {
                    d.set_message(
                        commit::add_sign_off(d.get_message().as_str(), &ci.name, &ci.email).into(),
                    );
                    state.borrow_mut().confirmed.push(pending);
                    try_commit(&d, &state, &ci);
                }
                (Pending::Conflicts, 0) => {
                    let first = state
                        .borrow()
                        .items
                        .iter()
                        .position(|i| i.entry.action() == Action::Unmerged);
                    if let Some(p) = first {
                        d.set_current_row(p as i32);
                    }
                }
                _ => {}
            }
        });
    }

    crate::snapshot::arm(&dlg);
    dlg.run()?;

    let decision = state.borrow_mut().decision.take();
    let mut settings = ctx.settings.clone();
    if !forced_whole {
        settings.set_bool(&whole_project_key(ctx), dlg.get_whole_project());
    }
    settings.set_bool("AddBeforeCommit", dlg.get_show_unversioned());
    settings.set("CommitLastAction", dlg.get_commit_action());
    let _ = settings.save();
    Ok(decision)
}

/// Enter diffs every selected row and Space checks them, through the same
/// callbacks as a double-click and a checkbox click.
fn wire_list_keys(dlg: &CommitDialog, list_menu: &FileListMenu) {
    let weak = dlg.as_weak();
    list_menu.set_on_activate(move |row| {
        if let Some(d) = weak.upgrade() {
            d.invoke_row_activated(row as i32);
        }
    });
    let weak = dlg.as_weak();
    list_menu.set_on_toggle(move |row, checked| {
        if let Some(d) = weak.upgrade() {
            d.invoke_row_toggled(row as i32, checked);
        }
    });
}

struct CommitInputs {
    workdir: std::path::PathBuf,
    title: String,
    has_head: bool,
    name: String,
    email: String,
    warn_no_signoff: bool,
    strip_comments: bool,
    merge_active: bool,
    is_git_svn: bool,
    bugtraq: koopagit_core::bugtraq::BugTraq,
}

fn notice(
    d: &CommitDialog,
    state: &Rc<RefCell<State>>,
    pending: Pending,
    text: &str,
    buttons: &[&str],
    error: bool,
) {
    state.borrow_mut().pending = pending;
    d.set_notice_text(text.into());
    let labels: Vec<SharedString> = buttons.iter().map(|b| (*b).into()).collect();
    d.set_notice_buttons(ModelRc::new(VecModel::from(labels)));
    d.set_notice_error(error);
}

fn clear_notice(d: &CommitDialog, state: &Rc<RefCell<State>>) {
    notice(d, state, Pending::None, "", &[], false);
}

/// `CCommitDlg::OnOK`: validation in upstream order, then build the job.
fn try_commit(d: &CommitDialog, state: &Rc<RefCell<State>>, ci: &CommitInputs) {
    let confirmed = state.borrow().confirmed.clone();
    let items = state.borrow().items.clone();
    let message_only = d.get_message_only();

    if !confirmed.contains(&Pending::Conflicts)
        && items.iter().any(|i| i.entry.action() == Action::Unmerged)
    {
        notice(
            d,
            state,
            Pending::Conflicts,
            "One or more files are in a conflicted state.",
            &["OK", "Ignore"],
            true,
        );
        return;
    }
    let new_branch = d
        .get_new_branch()
        .then(|| d.get_new_branch_name().to_string());
    if let Some(b) = &new_branch {
        let git = koopagit_core::git_cli::Git::new(&ci.workdir);
        if b.trim().is_empty()
            || !git
                .output(["check-ref-format", "--branch", b])
                .is_ok_and(|o| o.success())
        {
            notice(
                d,
                state,
                Pending::None,
                "Branch/Tag name must not be empty or is invalid.",
                &[],
                true,
            );
            return;
        }
        if git
            .output([
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{b}"),
            ])
            .is_ok_and(|o| o.success())
        {
            notice(
                d,
                state,
                Pending::None,
                "A branch with this name already exists. Delete the existing branch or use a different name.",
                &[],
                true,
            );
            return;
        }
    }
    let mut message = d.get_message().to_string();
    if ci.strip_comments {
        message = commit::strip_comments(&message);
    }
    if message.trim().is_empty() {
        notice(
            d,
            state,
            Pending::None,
            "You must enter a log message for the commit",
            &[],
            true,
        );
        return;
    }
    let template = state.borrow().template.clone();
    if !confirmed.contains(&Pending::Template)
        && !template.trim().is_empty()
        && message.trim() == template.trim()
    {
        notice(
            d,
            state,
            Pending::Template,
            "You did not edit the message template.\nProceed without individual commit message?",
            &["Proceed", "No"],
            false,
        );
        return;
    }
    if ci.warn_no_signoff
        && !confirmed.contains(&Pending::SignOff)
        && !commit::has_sign_off(&message, &ci.name, &ci.email)
    {
        notice(
            d,
            state,
            Pending::SignOff,
            "You haven't entered your Signed-Off-By line!",
            &["Add it", "Commit w/o", "Abort"],
            false,
        );
        return;
    }

    let amend = d.get_amend();
    let rows: Vec<CommitRow> = items
        .iter()
        .map(|i| CommitRow {
            entry: i.entry.clone(),
            checked: i.checked && !message_only,
            is_dir: i.is_dir,
        })
        .collect();
    if !(message_only || rows.iter().any(|r| r.checked) || amend || ci.merge_active) {
        notice(d, state, Pending::None, "Nothing to commit", &[], true);
        return;
    }
    // Bug-ID field (`bugtraq.message`) and the pre-commit hook script.
    let bug_id = d.get_bug_id().to_string();
    if !ci.bugtraq.valid_id(&bug_id) {
        notice(
            d,
            state,
            Pending::None,
            "Only numbers (optionally separated by commas)\nare allowed!",
            &[],
            true,
        );
        return;
    }
    let mut message = ci.bugtraq.apply(&message, &bug_id);
    let checked: Vec<String> = rows
        .iter()
        .filter(|r| r.checked)
        .map(|r| r.entry.path.clone())
        .collect();
    match crate::hookrun::pre_commit(&ci.workdir, &checked, &message) {
        Ok(Some(m)) => message = m,
        Ok(None) => {}
        Err(e) => {
            notice(d, state, Pending::None, &e, &[], true);
            return;
        }
    }
    let plan = commit::plan_index(&rows, ci.has_head, amend);
    let file = match crate::temp_message_file(&message) {
        Ok(f) => f,
        Err(e) => {
            notice(
                d,
                state,
                Pending::None,
                &format!("Could not write message file: {e}"),
                &[],
                true,
            );
            return;
        }
    };
    let opts = CommitOptions {
        amend,
        author: d.get_set_author().then(|| d.get_author().to_string()),
        date: d.get_set_date().then(|| {
            if amend && d.get_date_reset() {
                AuthorDate::Now
            } else {
                AuthorDate::At(d.get_date_text().to_string())
            }
        }),
        message_only,
    };
    let mut commands = Vec::new();
    if let Some(b) = &new_branch {
        commands.push(vec!["branch".to_owned(), "--".to_owned(), b.clone()]);
        commands.push(vec![
            "checkout".to_owned(),
            "--end-of-options".to_owned(),
            b.clone(),
            "--".to_owned(),
        ]);
    }
    commands.extend(plan.before);
    commands.push(commit::commit_args(&opts, &file.to_string_lossy()));
    commands.extend(plan.after);

    let action = CommitAction::from_index(d.get_commit_action());
    let launch =
        |c: &str| koopagit_core::launcher::Invocation::new(c).with_paths([ci.workdir.clone()]);
    let mut post = Vec::new();
    if ci.is_git_svn {
        post.push(PostAction::on_success(
            "SVN DCommit...",
            launch("svndcommit"),
        ));
    }
    post.push(PostAction::on_success("Push...", launch("push")));
    post.push(PostAction::on_success("Pull...", launch("pull")));
    post.push(PostAction::on_success("ReCommit", launch("commit")));
    post.push(PostAction::on_success("Create Tag...", launch("tag")));
    let close_on_end = if action == CommitAction::Commit {
        CloseOnEnd::Manual
    } else {
        CloseOnEnd::IfNoErrors
    };
    state.borrow_mut().decision = Some(Decision {
        action,
        job: ProgressJob {
            title: ci.title.clone(),
            git: koopagit_core::git_cli::Git::new(&ci.workdir),
            commands,
            post_actions: if action == CommitAction::Commit {
                post
            } else {
                vec![]
            },
            close_on_end,
        },
        message,
        amend,
        message_file: file,
    });
    let _ = d.hide();
}
