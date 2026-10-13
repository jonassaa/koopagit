//! `/command:pull` and `/command:fetch` — `CPullFetchDlg`, `CAppUtils::DoPull`
//! and `CAppUtils::DoFetch` (including "Launch Rebase After Fetch").

use std::cell::RefCell;
use std::rc::Rc;

use koopagit_core::Repo;
use koopagit_core::commands::{self, PullFetchOptions};
use koopagit_core::launcher::{CloseOnEnd, Invocation};
use koopagit_core::remote::{ALL_REMOTES, remote_choices};
use koopagit_core::settings::Settings;
use slint::ComponentHandle;

use crate::progress::{PostAction, ProgressJob, When};
use crate::remote_util::{
    merged, prune_default_label, remote_branches_of, save, strings, tags_default_label, tri, wd_key,
};
use crate::{Context, DialogResult, PullFetchDialog, msgbox, spawn_launcher};

/// What OK decided.
struct Decision {
    job: ProgressJob,
    /// Upstream to rebase onto after a successful fetch.
    rebase_upstream: Option<String>,
}

pub fn run_pull(ctx: &mut Context) -> DialogResult<i32> {
    run(ctx, false)
}

pub fn run_fetch(ctx: &mut Context) -> DialogResult<i32> {
    run(ctx, true)
}

fn rebase_in_progress(repo: &Repo) -> bool {
    let g = repo.git_dir();
    g.join("rebase-merge").exists() || g.join("rebase-apply").exists()
}

/// `branch.<cur>.rebase` / `pull.rebase` set to true or `merges`.
fn configured_rebase(repo: &Repo, branch: Option<&str>) -> bool {
    let v = branch
        .and_then(|b| repo.config_get(&format!("branch.{b}.rebase")))
        .or_else(|| repo.config_get("pull.rebase"));
    matches!(
        v.as_deref(),
        Some("true" | "merges" | "preserve" | "interactive")
    )
}

fn run(ctx: &mut Context, is_fetch: bool) -> DialogResult<i32> {
    let repo = &ctx.repo;
    if !is_fetch && rebase_in_progress(repo) {
        msgbox::error(
            "KoopaGit",
            "A rebase is in progress. Finish or abort it before pulling.",
        );
        return Ok(1);
    }
    let caption = if is_fetch { "Fetch" } else { "Pull" };
    let dlg = PullFetchDialog::new()?;
    dlg.set_dialog_title(ctx.window_title(caption).into());
    dlg.set_is_fetch(is_fetch);

    let current = repo.current_branch()?;
    let tracked = current.as_deref().and_then(|b| repo.tracking_branch(b));
    let remotes = repo.remotes();
    let choices = if is_fetch {
        remote_choices(&remotes)
    } else {
        remotes.clone()
    };
    dlg.set_remotes(strings(&choices));
    let saved = ctx
        .settings
        .string(&wd_key(r"History\PullRemote", repo), "");
    let wanted = ctx
        .invocation
        .get("remote")
        .map(str::to_owned)
        .or_else(|| tracked.as_ref().map(|t| t.0.clone()))
        .or_else(|| (!saved.is_empty()).then_some(saved))
        .unwrap_or_default();
    let index = choices
        .iter()
        .position(|c| *c == wanted)
        .or_else(|| {
            // Fetch with several remotes: "- all -" is first; prefer a real remote.
            (choices.first().map(String::as_str) == Some(ALL_REMOTES)).then_some(1)
        })
        .unwrap_or(0);
    dlg.set_remote_index(index.min(choices.len().saturating_sub(1)) as i32);
    dlg.set_url_items(strings(&ctx.settings.history(r"History\PullURLS", "url")));

    let branch_history = ctx.settings.history(r"History\PullRemoteBranch", "br");
    let remote_branch = tracked
        .as_ref()
        .map(|t| t.1.clone())
        .or_else(|| current.clone())
        .unwrap_or_default();
    let named_fetch_all = ctx.settings.bool("NamedRemoteFetchAll", true);
    if is_fetch && named_fetch_all {
        dlg.set_remote_branch_enabled(false);
    } else {
        dlg.set_remote_branch(remote_branch.into());
    }
    let sel_remote = choices.get(index).cloned().unwrap_or_default();
    dlg.set_remote_branch_items(strings(&merged(
        branch_history,
        remote_branches_of(repo, &sel_remote),
    )));
    dlg.set_show_depth(repo.is_shallow());
    dlg.set_use_depth(repo.is_shallow());

    let opt_key = format!(
        r"{}_{}",
        wd_key(r"TortoiseProc\PullFetch", repo),
        if is_fetch { 0 } else { 1 }
    );
    dlg.set_ff_only(!is_fetch && ctx.settings.bool(&format!(r"{opt_key}\ffonly"), false));
    let locked = !is_fetch && configured_rebase(repo, current.as_deref());
    dlg.set_rebase_locked(locked);
    dlg.set_rebase(locked || ctx.settings.bool(&format!(r"{opt_key}\rebase"), false));
    dlg.set_rebase_enabled(!repo.gix().is_bare());

    let update_defaults = {
        let weak = dlg.as_weak();
        let workdir = repo.workdir().to_owned();
        let choices = choices.clone();
        move || {
            let Some(d) = weak.upgrade() else { return };
            let Ok(repo) = Repo::discover(&workdir) else {
                return;
            };
            let remote = choices
                .get(d.get_remote_index().max(0) as usize)
                .filter(|r| *r != ALL_REMOTES)
                .cloned()
                .unwrap_or_default();
            d.set_tags_default(tags_default_label(&repo, &remote).into());
            d.set_prune_default(prune_default_label(&repo, &remote).into());
        }
    };
    update_defaults();
    dlg.on_remote_changed(update_defaults);

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

    let decision: Rc<RefCell<Option<Decision>>> = Rc::new(RefCell::new(None));
    let inputs = Rc::new(Inputs {
        workdir: repo.workdir().to_owned(),
        title: ctx.window_title(caption),
        choices,
        tracked,
        old_head: repo.head_id()?,
        has_submodules: repo.workdir().join(".gitmodules").exists(),
        named_fetch_all,
        verbose: ctx.settings.bool("FetchVerbose", true),
        opt_key,
        settings: RefCell::new(ctx.settings.clone()),
        is_fetch,
    });
    {
        let weak = dlg.as_weak();
        let decision = decision.clone();
        let inputs = inputs.clone();
        dlg.on_ok(move || {
            if let Some(d) = weak.upgrade() {
                if let Some(dec) = on_ok(&d, &inputs) {
                    *decision.borrow_mut() = Some(dec);
                    let _ = d.hide();
                }
            }
        });
    }
    dlg.on_notice_clicked(|_| {});

    crate::snapshot::arm(&dlg);
    dlg.run()?;
    let Some(dec) = decision.borrow_mut().take() else {
        return Ok(1);
    };
    let ok = crate::progress::run(dec.job)?;
    if ok {
        if let Some(upstream) = dec.rebase_upstream {
            rebase_after_fetch(&ctx.repo, &upstream);
        }
    }
    Ok(if ok { 0 } else { 1 })
}

struct Inputs {
    workdir: std::path::PathBuf,
    title: String,
    choices: Vec<String>,
    tracked: Option<(String, String)>,
    old_head: Option<String>,
    has_submodules: bool,
    named_fetch_all: bool,
    verbose: bool,
    opt_key: String,
    settings: RefCell<Settings>,
    is_fetch: bool,
}

fn error(d: &PullFetchDialog, text: &str) {
    d.set_notice_text(text.into());
    d.set_notice_buttons(strings::<&str>(&[]));
    d.set_notice_error(true);
}

/// `CPullFetchDlg::OnBnClickedOk` and building the git command.
fn on_ok(d: &PullFetchDialog, i: &Inputs) -> Option<Decision> {
    let repo = Repo::discover(&i.workdir).ok()?;
    let use_url = d.get_use_url();
    let choice = i
        .choices
        .get(d.get_remote_index().max(0) as usize)
        .cloned()
        .unwrap_or_default();
    let remote = if use_url {
        d.get_url().trim().to_owned()
    } else {
        choice.clone()
    };
    if remote.is_empty() {
        error(
            d,
            if use_url {
                "Remote URL must not be empty."
            } else {
                "Remote name must not be empty."
            },
        );
        return None;
    }
    let all_remotes = i.is_fetch && !use_url && remote == ALL_REMOTES;
    let typed_branch = d.get_remote_branch().trim().to_owned();
    let rebase = d.get_rebase() && !use_url;
    if !i.is_fetch && rebase && typed_branch.is_empty() {
        error(d, "You asked to pull, but did not specify a remote branch.");
        return None;
    }
    // With a named remote, an empty branch lets git use the configured refspecs
    // and update the tracking refs.
    let mut remote_branch = typed_branch.clone();
    if !use_url && i.named_fetch_all {
        let is_tracked = i
            .tracked
            .as_ref()
            .is_some_and(|(r, b)| *r == remote && *b == typed_branch);
        if i.is_fetch || is_tracked {
            remote_branch.clear();
        }
    }
    let depth = if d.get_show_depth() && d.get_use_depth() {
        match d.get_depth().trim().parse::<u32>() {
            Ok(n) => Some(n),
            Err(_) => {
                error(d, "Depth must be a number.");
                return None;
            }
        }
    } else {
        None
    };

    {
        let mut s = i.settings.borrow_mut();
        if use_url {
            s.push_history(r"History\PullURLS", "url", &remote, 25);
        } else {
            s.set(&wd_key(r"History\PullRemote", &repo), &remote);
        }
        s.push_history(r"History\PullRemoteBranch", "br", &typed_branch, 25);
        s.set_bool(&format!(r"{}\ffonly", i.opt_key), d.get_ff_only());
        s.set_bool(&format!(r"{}\rebase", i.opt_key), d.get_rebase());
        save(&s);
    }

    let o = PullFetchOptions {
        remote: remote.clone(),
        remote_branch: remote_branch.clone(),
        squash: d.get_squash(),
        no_commit: d.get_no_commit(),
        no_ff: d.get_no_ff(),
        ff_only: d.get_ff_only(),
        depth,
        tags: tri(d.get_tags()),
        prune: tri(d.get_prune()),
        allow_unrelated_histories: false,
        all_remotes,
        verbose: i.verbose,
    };
    let launch = |c: &str| Invocation::new(c).with_paths([i.workdir.clone()]);
    let fetch_like = i.is_fetch || rebase;
    let (cmd, post) = if fetch_like {
        let mut post = vec![
            PostAction::on_failure("Retry", launch(if i.is_fetch { "fetch" } else { "pull" })),
            PostAction::on_success("Show log", launch("log")),
            PostAction::on_success("Fetch...", launch("fetch")),
            PostAction::on_success("Switch/Checkout...", launch("switch")),
        ];
        if all_remotes {
            post.push(PostAction::on_failure("Show log", launch("log")));
        }
        if !rebase {
            post.insert(3, PostAction::on_success("Rebase...", launch("rebase")));
        }
        (commands::fetch(&o), post)
    } else {
        let mut post = vec![
            PostAction {
                label: "Resolve...".into(),
                when: When::FailureWith("CONFLICT"),
                invocation: launch("resolve"),
                git: Vec::new(),
            },
            PostAction {
                label: "Commit...".into(),
                when: When::FailureWith("CONFLICT"),
                invocation: launch("commit"),
                git: Vec::new(),
            },
            PostAction::on_failure("Pull...", launch("pull")),
            PostAction::on_failure("Stash Save...", launch("stashsave")),
        ];
        if let Some(old) = &i.old_head {
            post.push(PostAction::on_success(
                "Pulled Diff",
                launch("showcompare")
                    .with("revision1", old.clone())
                    .with("revision2", "HEAD"),
            ));
            post.push(PostAction::on_success(
                "Pulled Log",
                launch("log").with("range", format!("{old}..HEAD")),
            ));
        }
        if i.has_submodules {
            post.push(PostAction::on_success(
                "Submodule Update...",
                launch("subupdate").with("bkpath", i.workdir.to_string_lossy()),
            ));
        }
        (commands::pull(&o), post)
    };
    let rebase_upstream = rebase.then(|| {
        let branch = if !typed_branch.is_empty() {
            Some(typed_branch.clone())
        } else {
            i.tracked.as_ref().map(|t| t.1.clone())
        };
        match branch {
            Some(b) if !use_url => format!("remotes/{remote}/{b}"),
            _ => "FETCH_HEAD".to_owned(),
        }
    });
    Some(Decision {
        job: ProgressJob {
            title: i.title.clone(),
            git: repo.git(),
            commands: vec![cmd],
            post_actions: if rebase { vec![] } else { post },
            close_on_end: if rebase {
                CloseOnEnd::IfNoErrors
            } else {
                CloseOnEnd::Manual
            },
        },
        rebase_upstream,
    })
}

/// `RebaseAfterFetch`: fast-forward check, then the Rebase dialog.
fn rebase_after_fetch(repo: &Repo, upstream: &str) {
    let git = repo.git();
    let is_ancestor = |a: &str, b: &str| {
        git.output(["merge-base", "--is-ancestor", a, b])
            .is_ok_and(|o| o.success())
    };
    if is_ancestor(upstream, "HEAD") {
        let ask = msgbox::ask(
            "KoopaGit",
            "The current branch is up to date with the upstream branch.\nDo you want to rebase anyway?",
            &["Yes", "No"],
        );
        if ask != Some(0) {
            return;
        }
    } else if is_ancestor("HEAD", upstream) {
        match msgbox::ask(
            "KoopaGit",
            &format!("The current branch can be fast-forwarded to \"{upstream}\"."),
            &["Merge", "Rebase", "Abort"],
        ) {
            Some(0) => {
                let _ = crate::progress::run(ProgressJob {
                    title: "Merge".to_owned(),
                    git: repo.git(),
                    commands: vec![vec![
                        "merge".into(),
                        "--ff-only".into(),
                        "--".into(),
                        upstream.to_owned(),
                    ]],
                    post_actions: vec![],
                    close_on_end: CloseOnEnd::IfNoErrors,
                });
                return;
            }
            Some(1) => {}
            _ => return,
        }
    }
    spawn_launcher(
        &Invocation::new("rebase")
            .with_paths([repo.workdir()])
            .with("upstream", upstream),
    );
}
