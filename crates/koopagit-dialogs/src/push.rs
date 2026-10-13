//! `/command:push` — `CPushDlg` and `CAppUtils::DoPush`.

use std::cell::RefCell;
use std::rc::Rc;

use koopagit_core::commands::{self, PushOptions, RecurseSubmodules};
use koopagit_core::launcher::{CloseOnEnd, Invocation};
use koopagit_core::remote::{ALL_REMOTES, remote_choices};
use slint::ComponentHandle;

use crate::progress::{PostAction, ProgressJob, When};
use crate::remote_util::{merged, remote_branches_of, save, strings, valid_branch_name, wd_key};
use crate::{Context, DialogResult, PushDialog, spawn_launcher};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ask {
    None,
    AllBranches,
    BothEmpty,
    LocalEmpty,
}

struct State {
    pending: Ask,
    confirmed: Vec<Ask>,
    job: Option<ProgressJob>,
}

fn recurse_from_config(v: Option<String>) -> i32 {
    match v.as_deref() {
        Some("check") => 1,
        Some("on-demand") => 2,
        _ => 0,
    }
}

fn recurse(i: i32) -> RecurseSubmodules {
    match i {
        1 => RecurseSubmodules::Check,
        2 => RecurseSubmodules::OnDemand,
        _ => RecurseSubmodules::None,
    }
}

/// `/command:push [/branch:<ref>]`: one dialog per selected work tree; KoopaGit
/// launches with a single repository.
pub fn run(ctx: &mut Context) -> DialogResult<i32> {
    let repo = &ctx.repo;
    let dlg = PushDialog::new()?;
    dlg.set_dialog_title(ctx.window_title("Push").into());

    let current = repo.current_branch()?;
    let branch_arg = ctx.invocation.get("branch").map(str::to_owned);
    let local = match &branch_arg {
        Some(b) => b.strip_prefix("refs/heads/").unwrap_or(b).to_owned(),
        None => match &current {
            Some(b) => b.clone(),
            None if repo.head_id()?.is_some() => "HEAD".to_owned(),
            None => String::new(),
        },
    };
    dlg.set_local(local.clone().into());
    dlg.set_local_items(strings(&repo.local_branches()?));
    if branch_arg.is_none() {
        dlg.set_all_branches(ctx.settings.bool(
            &(wd_key(r"TortoiseProc\Push", repo) + r"\AllBranches"),
            false,
        ));
    }

    let remotes = repo.remotes();
    let choices = remote_choices(&remotes);
    dlg.set_has_all_entry(remotes.len() > 1);
    dlg.set_remotes(strings(&choices));
    let (push_remote, push_branch) = repo.push_target(&local);
    let saved = ctx
        .settings
        .string(&wd_key(r"History\PushRemote", repo), "");
    let all_saved = ctx.settings.bool(
        &(wd_key(r"TortoiseProc\Push", repo) + r"\AllRemotes"),
        false,
    );
    let wanted = if remotes.len() > 1 && (all_saved || saved == ALL_REMOTES) {
        ALL_REMOTES.to_owned()
    } else if !push_remote.is_empty() {
        push_remote.clone()
    } else if !saved.is_empty() {
        saved
    } else {
        remotes.first().cloned().unwrap_or_default()
    };
    dlg.set_remote_index(choices.iter().position(|c| *c == wanted).unwrap_or(0) as i32);

    let branch_history = ctx
        .settings
        .history(&wd_key(r"History\RemoteBranch", repo), "branch");
    let remote_branch = if !push_branch.is_empty() {
        push_branch
    } else if local != "HEAD" {
        local.clone()
    } else {
        String::new()
    };
    dlg.set_remote_branch(remote_branch.into());
    let selected_remote = if wanted == ALL_REMOTES {
        remotes.first().cloned().unwrap_or_default()
    } else {
        wanted.clone()
    };
    dlg.set_remote_branch_items(strings(&merged(
        branch_history,
        remote_branches_of(repo, &selected_remote),
    )));
    dlg.set_url_items(strings(
        &ctx.settings
            .history(&wd_key(r"History\PushURLS", repo), "url"),
    ));
    if let Some(b) = &current {
        dlg.set_set_upstream(repo.tracking_branch(b).is_none());
    }
    let recurse_cfg = recurse_from_config(repo.config_get("push.recurseSubmodules"));
    dlg.set_recurse_submodules(
        ctx.settings
            .dword(
                &wd_key(r"History\PushRecurseSubmodules", repo),
                recurse_cfg as u32,
            )
            .min(2) as i32,
    );
    dlg.set_push_option_items(strings(
        &ctx.settings
            .history(&wd_key(r"History\PushOption", repo), "option"),
    ));

    let state = Rc::new(RefCell::new(State {
        pending: Ask::None,
        confirmed: Vec::new(),
        job: None,
    }));

    {
        let workdir = repo.workdir().to_owned();
        dlg.on_manage(move || {
            spawn_launcher(
                &Invocation::new("settings")
                    .with_paths([workdir.clone()])
                    .with("page", "gitremote"),
            );
        });
    }
    {
        let weak = dlg.as_weak();
        dlg.on_cancel(move || {
            if let Some(d) = weak.upgrade() {
                let _ = d.hide();
            }
        });
    }
    // Callbacks need 'static data, so validation reads a snapshot of the context.
    let snapshot = Rc::new(Snapshot::new(ctx, recurse_cfg));
    {
        let weak = dlg.as_weak();
        let state = state.clone();
        let snap = snapshot.clone();
        dlg.on_ok(move || {
            if let Some(d) = weak.upgrade() {
                state.borrow_mut().confirmed.clear();
                try_ok(&d, &state, &snap);
            }
        });
    }
    {
        let weak = dlg.as_weak();
        let state = state.clone();
        let snap = snapshot.clone();
        dlg.on_notice_clicked(move |i| {
            let Some(d) = weak.upgrade() else { return };
            let pending = state.borrow().pending;
            notice(&d, &state, Ask::None, "", &[], false);
            if i == 0 && pending != Ask::None {
                state.borrow_mut().confirmed.push(pending);
                try_ok(&d, &state, &snap);
            }
        });
    }

    {
        let weak = dlg.as_weak();
        let snap = snapshot.clone();
        dlg.on_refresh(move || {
            if let Some(d) = weak.upgrade() {
                refresh(&d, &snap);
            }
        });
    }

    crate::snapshot::arm(&dlg);
    dlg.run()?;
    let job = state.borrow_mut().job.take();
    let Some(job) = job else { return Ok(1) };
    // Client hook scripts (Settings > Hook Scripts).
    if !crate::hookrun::pre_push_or_ask(ctx.repo.workdir()) {
        return Ok(1);
    }
    let ok = crate::progress::run(job)?;
    crate::hookrun::post_push(ctx.repo.workdir());
    Ok(if ok { 0 } else { 1 })
}

/// What the OK handler needs, detached from the borrowed context.
struct Snapshot {
    workdir: std::path::PathBuf,
    title: String,
    // Reloaded by F5.
    remotes: RefCell<Vec<String>>,
    choices: RefCell<Vec<String>>,
    configured_recurse: i32,
    settings: RefCell<koopagit_core::settings::Settings>,
    is_submodule: bool,
}

impl Snapshot {
    fn new(ctx: &Context, configured_recurse: i32) -> Self {
        let remotes = ctx.repo.remotes();
        // A submodule work tree has a `.git` file and an enclosing repository.
        let workdir = ctx.repo.workdir();
        let is_submodule = workdir.join(".git").is_file()
            && workdir
                .parent()
                .is_some_and(|p| koopagit_core::Repo::discover(p).is_ok());
        Snapshot {
            workdir: ctx.repo.workdir().to_owned(),
            title: ctx.window_title("Push"),
            choices: RefCell::new(remote_choices(&remotes)),
            remotes: RefCell::new(remotes),
            configured_recurse,
            settings: RefCell::new(ctx.settings.clone()),
            is_submodule,
        }
    }

    fn repo(&self) -> Option<koopagit_core::Repo> {
        koopagit_core::Repo::discover(&self.workdir).ok()
    }
}

fn notice(
    d: &PushDialog,
    state: &Rc<RefCell<State>>,
    ask: Ask,
    text: &str,
    buttons: &[&str],
    error: bool,
) {
    state.borrow_mut().pending = ask;
    d.set_notice_text(text.into());
    d.set_notice_buttons(strings(buttons));
    d.set_notice_error(error);
}

/// `CPushDlg::OnBnClickedOk` + `CAppUtils::DoPush`.
/// F5 (`CPushDlg::Refresh`): reload the local branches, the remotes (keeping
/// the selected one) and the remote branches of the selected remote.
fn refresh(d: &PushDialog, snap: &Snapshot) {
    let Some(repo) = snap.repo() else { return };
    let selected = snap
        .choices
        .borrow()
        .get(d.get_remote_index().max(0) as usize)
        .cloned();
    let remotes = repo.remotes();
    let choices = remote_choices(&remotes);
    d.set_local_items(strings(&repo.local_branches().unwrap_or_default()));
    d.set_has_all_entry(remotes.len() > 1);
    d.set_remotes(strings(&choices));
    d.set_remote_index(
        selected
            .and_then(|s| choices.iter().position(|c| *c == s))
            .unwrap_or(0) as i32,
    );
    let remote = match choices.get(d.get_remote_index() as usize) {
        Some(c) if c != ALL_REMOTES => c.clone(),
        _ => remotes.first().cloned().unwrap_or_default(),
    };
    let history = snap
        .settings
        .borrow()
        .history(&wd_key(r"History\RemoteBranch", &repo), "branch");
    d.set_remote_branch_items(strings(&merged(
        history,
        remote_branches_of(&repo, &remote),
    )));
    *snap.remotes.borrow_mut() = remotes;
    *snap.choices.borrow_mut() = choices;
}

fn try_ok(d: &PushDialog, state: &Rc<RefCell<State>>, snap: &Snapshot) {
    let Some(repo) = snap.repo() else { return };
    let confirmed = state.borrow().confirmed.clone();
    let use_url = d.get_use_url();
    let all_branches = d.get_all_branches();
    let choice = snap
        .choices
        .borrow()
        .get(d.get_remote_index().max(0) as usize)
        .cloned()
        .unwrap_or_default();
    let destinations: Vec<String> = if use_url {
        vec![d.get_url().trim().to_owned()]
    } else if choice == ALL_REMOTES {
        snap.remotes.borrow().clone()
    } else {
        vec![choice.clone()]
    };
    if destinations.iter().all(|r| r.is_empty()) {
        let msg = if use_url {
            "Remote URL must not be empty."
        } else {
            "Remote name must not be empty."
        };
        notice(d, state, Ask::None, msg, &[], true);
        return;
    }
    let local = d.get_local().trim().to_owned();
    let remote_branch = d.get_remote_branch().trim().to_owned();
    if all_branches && !confirmed.contains(&Ask::AllBranches) {
        notice(
            d,
            state,
            Ask::AllBranches,
            "Do you really want to push all local branches?",
            &["Yes", "No"],
            false,
        );
        return;
    }
    if !all_branches {
        if local.is_empty() && remote_branch.is_empty() && !confirmed.contains(&Ask::BothEmpty) {
            notice(
                d,
                state,
                Ask::BothEmpty,
                "The local branch name and the remote branch name are empty.\nContinue?",
                &["Yes", "No"],
                false,
            );
            return;
        }
        if local.is_empty() && !remote_branch.is_empty() && !confirmed.contains(&Ask::LocalEmpty) {
            notice(
                d,
                state,
                Ask::LocalEmpty,
                "The local branch/tag name is empty. This results in a remote removal.\nContinue?",
                &["Yes", "No"],
                false,
            );
            return;
        }
        if !remote_branch.is_empty() && !valid_branch_name(&repo, &remote_branch) {
            notice(
                d,
                state,
                Ask::None,
                "Branch/Tag name is invalid.",
                &[],
                true,
            );
            return;
        }
        let is_branch = repo
            .git()
            .output([
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{local}"),
            ])
            .is_ok_and(|o| o.success());
        let is_tag = repo
            .git()
            .output([
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/tags/{local}"),
            ])
            .is_ok_and(|o| o.success());
        if is_branch && is_tag {
            notice(
                d,
                state,
                Ask::None,
                "The entered source ref is ambiguous.\nYou have to make it unique (e.g. by adding \"refs/heads/\" as a prefix for branches or \"refs/tags/\" for tags)!",
                &[],
                true,
            );
            return;
        }
        if is_branch && !use_url && choice != ALL_REMOTES {
            if d.get_always_branch() {
                let _ =
                    repo.config_set_local(&format!("branch.{local}.pushbranch"), &remote_branch);
            }
            if d.get_always_remote() {
                let _ = repo.config_set_local(&format!("branch.{local}.pushremote"), &choice);
            }
        }
    }

    // Persist the dialog state.
    {
        let mut s = snap.settings.borrow_mut();
        let push_key = wd_key(r"TortoiseProc\Push", &repo);
        s.set_bool(&format!(r"{push_key}\AllBranches"), all_branches);
        s.set_bool(
            &format!(r"{push_key}\AllRemotes"),
            !use_url && choice == ALL_REMOTES,
        );
        s.set(
            &wd_key(r"History\PushRecurseSubmodules", &repo),
            d.get_recurse_submodules(),
        );
        if use_url {
            s.push_history(
                &wd_key(r"History\PushURLS", &repo),
                "url",
                d.get_url().trim(),
                25,
            );
        } else {
            s.set(&wd_key(r"History\PushRemote", &repo), &choice);
        }
        s.push_history(
            &wd_key(r"History\RemoteBranch", &repo),
            "branch",
            &remote_branch,
            25,
        );
        s.push_history(
            &wd_key(r"History\PushOption", &repo),
            "option",
            d.get_push_option().trim(),
            25,
        );
        save(&s);
    }

    let mut commands_list = Vec::new();
    for remote in destinations.iter().filter(|r| !r.is_empty()) {
        let o = PushOptions {
            remote: remote.clone(),
            local: if all_branches {
                String::new()
            } else {
                local.clone()
            },
            remote_branch: if all_branches {
                String::new()
            } else {
                remote_branch.clone()
            },
            all_branches,
            tags: d.get_tags(),
            force: d.get_force(),
            force_with_lease: d.get_force_with_lease(),
            set_upstream: d.get_set_upstream() && !use_url,
            recurse_submodules: recurse(d.get_recurse_submodules()),
            configured_recurse_submodules: recurse(snap.configured_recurse),
            push_option: d.get_push_option().trim().to_owned(),
        };
        commands_list.extend(commands::push(&o));
    }

    let launch = |c: &str| Invocation::new(c).with_paths([snap.workdir.clone()]);
    let mut post = vec![
        PostAction {
            label: "Pull...".into(),
            when: When::FailureWith("! [rejected]"),
            invocation: launch("pull"),
            git: Vec::new(),
        },
        PostAction {
            label: "Fetch...".into(),
            when: When::FailureWith("! [rejected]"),
            invocation: launch("fetch"),
            git: Vec::new(),
        },
        PostAction::on_failure("Push...", launch("push")),
        PostAction::on_success("Request pull...", launch("requestpull")),
        PostAction::on_success("Push...", launch("push")),
        PostAction::on_success("Switch/Checkout...", launch("switch")),
    ];
    if snap.is_submodule {
        if let Some(parent) = snap.workdir.parent() {
            post.push(PostAction::on_success(
                "Commit superproject...",
                Invocation::new("commit").with_paths([parent.to_owned()]),
            ));
        }
    }
    state.borrow_mut().job = Some(ProgressJob {
        title: snap.title.clone(),
        git: repo.git(),
        commands: commands_list,
        post_actions: post,
        close_on_end: CloseOnEnd::Manual,
    });
    let _ = d.hide();
}
