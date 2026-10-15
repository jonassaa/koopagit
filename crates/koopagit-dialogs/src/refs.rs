//! Switch/Checkout, Create Branch/Tag, Merge, Abort Merge and Stash
//! (`CGitSwitchDlg`, `CCreateBranchTagDlg`, `CMergeDlg`, `CMergeAbortDlg`,
//! `CStashSaveDlg`, `CAppUtils::Stash*`).

use std::cell::RefCell;
use std::rc::Rc;

use koopagit_core::Repo;
use koopagit_core::commands::{
    self, MergeOptions, ResetType, StashUntracked, SwitchOptions, TriState,
};
use koopagit_core::launcher::{CloseOnEnd, Invocation};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::progress::{PostAction, ProgressJob, When};
use crate::{
    AbortMergeDialog, BranchTagDialog, Context, DialogResult, MergeDialog, StashDialog,
    SwitchDialog, msgbox, spawn_launcher,
};

/// Lists offered by the "choose version" block.
#[derive(Debug, Clone, Default)]
pub struct Versions {
    /// Local branches, then `remotes/<remote>/<branch>`, then `FETCH_HEAD`.
    pub branches: Vec<String>,
    pub tags: Vec<String>,
}

impl Versions {
    pub fn load(ctx: &Context, skip_current: bool) -> Self {
        let repo = &ctx.repo;
        let current = repo.current_branch().ok().flatten();
        let mut branches: Vec<String> = repo
            .local_branches()
            .unwrap_or_default()
            .into_iter()
            .filter(|b| !(skip_current && Some(b) == current.as_ref()))
            .collect();
        branches.extend(
            repo.remote_branches()
                .unwrap_or_default()
                .into_iter()
                .map(|b| format!("remotes/{b}")),
        );
        if ctx.settings.bool("BranchesIncludeFetchHead", true)
            && repo.git_dir().join("FETCH_HEAD").exists()
        {
            branches.push("FETCH_HEAD".to_owned());
        }
        let mut tags = repo.tags().unwrap_or_default();
        if ctx.settings.bool("SortTagsReversed", false) {
            tags.reverse();
        }
        Versions { branches, tags }
    }

    /// Finds `rev` in the lists: `(mode, branch index, tag index)`.
    pub fn locate(&self, rev: &str) -> (i32, i32, i32) {
        let short = rev
            .strip_prefix("refs/heads/")
            .or_else(|| rev.strip_prefix("refs/"))
            .unwrap_or(rev);
        if let Some(i) = self.branches.iter().position(|b| b == short) {
            return (1, i as i32, 0);
        }
        let tag = rev.strip_prefix("refs/tags/").unwrap_or(rev);
        if let Some(i) = self.tags.iter().position(|t| t == tag) {
            return (2, 0, i as i32);
        }
        (3, 0, 0)
    }

    /// `ChooseVersion::GetSelectedRef`: branch or tag name, prefixed with
    /// `refs/heads/` / `refs/tags/` when the short name is ambiguous.
    pub fn selected(&self, mode: i32, branch: i32, tag: i32, commit: &str) -> String {
        match mode {
            0 => "HEAD".to_owned(),
            1 => {
                let b = self
                    .branches
                    .get(branch as usize)
                    .cloned()
                    .unwrap_or_default();
                if self.tags.contains(&b) {
                    format!("refs/heads/{b}")
                } else {
                    b
                }
            }
            2 => {
                let t = self.tags.get(tag as usize).cloned().unwrap_or_default();
                if self.branches.contains(&t) {
                    format!("refs/tags/{t}")
                } else {
                    t
                }
            }
            _ => commit.trim().to_owned(),
        }
    }
}

pub(crate) fn string_model(items: &[String]) -> ModelRc<SharedString> {
    let v: Vec<SharedString> = items.iter().map(|s| s.as_str().into()).collect();
    ModelRc::new(VecModel::from(v))
}

/// `git check-ref-format --branch`.
pub(crate) fn valid_branch_name(repo: &Repo, name: &str) -> bool {
    !name.trim().is_empty()
        && repo
            .git()
            .output(["check-ref-format", "--branch", name])
            .is_ok_and(|o| o.success())
}

pub(crate) fn ref_exists(repo: &Repo, full: &str) -> bool {
    repo.git()
        .output(["show-ref", "--verify", "--quiet", full])
        .is_ok_and(|o| o.success())
}

/// The remote prefix of a remote-tracking ref (`remotes/<r>/x` or `<r>/x`), if any.
fn remote_branch_part(version: &str, remotes: &[String]) -> Option<String> {
    let (name, create) = commands::switch_default_branch_name(version, remotes);
    create.then_some(name)
}

/// `CheckUserData` for operations that create commits.
pub(crate) fn check_user_data(ctx: &Context) -> bool {
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

fn tristate(v: i32) -> TriState {
    match v {
        1 => TriState::On,
        0 => TriState::Off,
        _ => TriState::Default,
    }
}

fn has_submodules(ctx: &Context) -> bool {
    ctx.repo.workdir().join(".gitmodules").exists()
}

// ---------------------------------------------------------------- Switch

/// `/command:switch [/rev:<ref>]`.
pub fn switch(ctx: &mut Context) -> DialogResult<i32> {
    let versions = Versions::load(ctx, false);
    let remotes = ctx.repo.remotes();
    let dlg = SwitchDialog::new()?;
    dlg.set_dialog_title(ctx.window_title("Switch/Checkout").into());
    dlg.set_branches(string_model(&versions.branches));
    dlg.set_tags(string_model(&versions.tags));
    if let Some(rev) = ctx.invocation.get("rev").filter(|r| !r.is_empty()) {
        let (mode, b, t) = versions.locate(rev);
        dlg.set_mode(mode);
        dlg.set_branch_index(b);
        dlg.set_tag_index(t);
        if mode == 3 {
            dlg.set_commit_text(rev.into());
        }
    }
    let versions = Rc::new(versions);
    let settings = ctx.settings.clone();
    let default_name = {
        let versions = versions.clone();
        let weak = dlg.as_weak();
        let workdir = ctx.repo.workdir().to_owned();
        move || {
            let Some(d) = weak.upgrade() else { return };
            let mode = d.get_mode();
            let version = versions.selected(
                mode,
                d.get_branch_index(),
                d.get_tag_index(),
                d.get_commit_text().as_str(),
            );
            if let Some(name) = remote_branch_part(&version, &remotes) {
                d.set_branch_name(name.into());
                d.set_create_branch(true);
                d.set_track_enabled(true);
                d.set_track(2);
                return;
            }
            let shown = if mode == 3 {
                Repo::discover(&workdir)
                    .ok()
                    .and_then(|r| r.rev_parse(&version).ok())
                    .map(|h| h[..h.len().min(8)].to_owned())
                    .unwrap_or_else(|| version.clone())
            } else {
                version.clone()
            };
            d.set_branch_name(format!("Branch_{shown}").into());
            d.set_track_enabled(false);
            d.set_track(0);
            let create = match mode {
                3 => settings.bool("SwitchToCommitNewBranch", true),
                2 => settings.bool("SwitchToTagNewBranch", true),
                _ => false,
            };
            d.set_create_branch(create);
            if !create {
                d.set_override_branch(false);
            }
        }
    };
    let default_name = Rc::new(default_name);
    default_name();
    {
        let f = default_name.clone();
        dlg.on_version_changed(move || f());
    }
    let chosen: Rc<RefCell<Option<SwitchOptions>>> = Rc::default();
    {
        let weak = dlg.as_weak();
        let chosen = chosen.clone();
        let versions = versions.clone();
        let workdir = ctx.repo.workdir().to_owned();
        dlg.on_ok(move || {
            let Some(d) = weak.upgrade() else { return };
            let Ok(repo) = Repo::discover(&workdir) else { return };
            let version = versions.selected(
                d.get_mode(),
                d.get_branch_index(),
                d.get_tag_index(),
                d.get_commit_text().as_str(),
            );
            if version.is_empty() {
                d.set_notice_text("Please select a branch, tag or commit.".into());
                return;
            }
            let new_branch = d.get_create_branch().then(|| d.get_branch_name().to_string());
            if let Some(b) = &new_branch {
                if !valid_branch_name(&repo, b) {
                    d.set_notice_text("Branch/Tag name must not be empty or is invalid.".into());
                    return;
                }
                if !d.get_override_branch() && ref_exists(&repo, &format!("refs/heads/{b}")) {
                    d.set_notice_text(
                        "Branch already exists. Choose a different name or check \"Override branch if exists\"."
                            .into(),
                    );
                    return;
                }
            }
            *chosen.borrow_mut() = Some(SwitchOptions {
                version,
                new_branch,
                force: d.get_force(),
                merge: d.get_merge(),
                track: if d.get_track_enabled() {
                    tristate(d.get_track())
                } else {
                    TriState::Default
                },
                override_branch: d.get_override_branch(),
            });
            let _ = d.hide();
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
    crate::window::prepare(&dlg);
    dlg.run()?;
    let mode = dlg.get_mode();
    if mode == 3 || mode == 2 {
        let key = if mode == 3 {
            "SwitchToCommitNewBranch"
        } else {
            "SwitchToTagNewBranch"
        };
        if chosen.borrow().is_some() {
            ctx.settings.set_bool(key, dlg.get_create_branch());
            let _ = ctx.settings.save();
        }
    }
    let Some(opts) = chosen.take() else {
        return Ok(1);
    };
    Ok(if perform_switch(ctx, &opts)? { 0 } else { 1 })
}

/// `CAppUtils::PerformSwitch` with its post-command buttons.
pub fn perform_switch(ctx: &Context, opts: &SwitchOptions) -> DialogResult<bool> {
    let mut post = Vec::new();
    if has_submodules(ctx) {
        post.push(PostAction::on_success(
            "Update submodules",
            ctx.launch("subupdate")
                .with("bkpath", ctx.repo.workdir().to_string_lossy()),
        ));
    }
    post.push(PostAction::on_success("Merge", ctx.launch("merge")));
    post.push(PostAction::on_success("Pull", ctx.launch("pull")));
    post.push(PostAction::on_success("Commit", ctx.launch("commit")));
    if opts.merge {
        post.push(PostAction::on_failure("Resolve", ctx.launch("resolve")));
    } else {
        post.push(PostAction::on_failure(
            "Stash Save",
            ctx.launch("stashsave"),
        ));
    }
    post.push(PostAction::on_failure(
        "Retry",
        ctx.launch("switch").with("rev", opts.version.clone()),
    ));
    let ok = crate::progress::run(ProgressJob {
        title: ctx.window_title("Switch/Checkout"),
        git: ctx.repo.git(),
        commands: vec![commands::switch(opts)],
        post_actions: post,
        close_on_end: ctx.invocation.close_on_end().unwrap_or_default(),
    })?;
    Ok(ok)
}

// ---------------------------------------------------------------- Branch / Tag

/// `/command:branch` and `/command:tag` (`[/rev:<ref>] [/name:<name>]`).
pub fn branch_tag(ctx: &mut Context, is_tag: bool) -> DialogResult<i32> {
    let repo = &ctx.repo;
    let versions = Versions::load(ctx, false);
    let remotes = repo.remotes();
    let current = repo.current_branch()?.unwrap_or_default();
    let dlg = BranchTagDialog::new()?;
    let dialog_name = if is_tag {
        "Create Tag"
    } else {
        "Create Branch"
    };
    dlg.set_dialog_title(ctx.window_title(dialog_name).into());
    dlg.set_is_tag(is_tag);
    dlg.set_head_label(format!("HEAD ({current})").into());
    dlg.set_branches(string_model(&versions.branches));
    dlg.set_tags(string_model(&versions.tags));
    dlg.set_name(ctx.invocation.get("name").unwrap_or_default().into());
    if let Some(rev) = ctx.invocation.get("rev").filter(|r| !r.is_empty()) {
        let (mode, b, t) = versions.locate(rev);
        dlg.set_mode(mode);
        dlg.set_branch_index(b);
        dlg.set_tag_index(t);
        if mode == 3 {
            dlg.set_commit_text(rev.into());
        }
    }
    // KoopaGit only opens work trees, so "Switch to new branch" is always available.
    dlg.set_switch_enabled(true);
    dlg.set_switch_to(ctx.settings.bool("NewBranchSwitchTo", false));
    dlg.set_push(ctx.settings.bool("PushTag", false));
    let signing_key = repo
        .git()
        .run(["config", "--get", "user.signingkey"])
        .unwrap_or_default();
    dlg.set_sign_enabled(!signing_key.trim().is_empty());

    let versions = Rc::new(versions);
    let last_default: Rc<RefCell<String>> = Rc::default();
    let on_version = {
        let versions = versions.clone();
        let weak = dlg.as_weak();
        let last_default = last_default.clone();
        move || {
            let Some(d) = weak.upgrade() else { return };
            let mode = d.get_mode();
            let version = versions.selected(
                mode,
                d.get_branch_index(),
                d.get_tag_index(),
                d.get_commit_text().as_str(),
            );
            let remote = (mode == 1 && version.starts_with("remotes/"))
                .then(|| remote_branch_part(&version, &remotes))
                .flatten();
            d.set_track_enabled(!is_tag && remote.is_some());
            if let Some(name) = remote {
                let cur = d.get_name().to_string();
                if cur.is_empty() || cur == *last_default.borrow() {
                    d.set_name(name.clone().into());
                    *last_default.borrow_mut() = name;
                }
            }
        }
    };
    let on_version = Rc::new(on_version);
    on_version();
    {
        let f = on_version.clone();
        dlg.on_version_changed(move || f());
    }
    {
        let weak = dlg.as_weak();
        let last_default = last_default.clone();
        dlg.on_name_edited(move || {
            if let Some(d) = weak.upgrade() {
                if d.get_name().as_str() != last_default.borrow().as_str() {
                    d.set_track(0);
                }
            }
        });
    }
    struct Choice {
        name: String,
        base: String,
        force: bool,
        track: TriState,
        switch_to: bool,
        sign: bool,
        push: bool,
        message: String,
    }
    let chosen: Rc<RefCell<Option<Choice>>> = Rc::default();
    {
        let weak = dlg.as_weak();
        let chosen = chosen.clone();
        let versions = versions.clone();
        let workdir = repo.workdir().to_owned();
        let current = current.clone();
        dlg.on_ok(move || {
            let Some(d) = weak.upgrade() else { return };
            let Ok(repo) = Repo::discover(&workdir) else {
                return;
            };
            let name = d.get_name().trim().to_owned();
            let message = d.get_message().to_string();
            let sign = is_tag && d.get_sign();
            if sign && message.trim().is_empty() {
                d.set_notice_text("You must enter a log message for the commit".into());
                return;
            }
            let valid = if is_tag {
                !name.is_empty()
                    && repo
                        .git()
                        .output(["check-ref-format", &format!("refs/tags/{name}")])
                        .is_ok_and(|o| o.success())
            } else {
                valid_branch_name(&repo, &name)
            };
            if !valid {
                d.set_notice_text("Branch/Tag name must not be empty or is invalid.".into());
                return;
            }
            let full = if is_tag {
                format!("refs/tags/{name}")
            } else {
                format!("refs/heads/{name}")
            };
            if ref_exists(&repo, &full) {
                if !is_tag && name == current {
                    d.set_notice_text("Cannot force the current branch.".into());
                    return;
                }
                if !d.get_force() {
                    d.set_notice_text(
                        "A branch/tag with this name already exists. Use \"Force\" to overwrite it."
                            .into(),
                    );
                    return;
                }
            }
            *chosen.borrow_mut() = Some(Choice {
                name,
                base: versions.selected(
                    d.get_mode(),
                    d.get_branch_index(),
                    d.get_tag_index(),
                    d.get_commit_text().as_str(),
                ),
                force: d.get_force(),
                track: if d.get_track_enabled() {
                    tristate(d.get_track())
                } else {
                    TriState::Default
                },
                switch_to: !is_tag && d.get_switch_to(),
                sign,
                push: is_tag && d.get_push(),
                message,
            });
            let _ = d.hide();
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
    crate::window::prepare(&dlg);
    dlg.run()?;
    let Some(c) = chosen.take() else {
        return Ok(1);
    };
    if is_tag {
        ctx.settings.set_bool("PushTag", c.push);
    } else {
        ctx.settings.set_bool("NewBranchSwitchTo", c.switch_to);
    }
    let _ = ctx.settings.save();

    let git = ctx.repo.git();
    let base = if c.base.is_empty() {
        "HEAD".to_owned()
    } else {
        c.base.clone()
    };
    let out = if is_tag {
        let file = (!c.message.trim().is_empty())
            .then(|| crate::temp_message_file(&c.message))
            .transpose()?;
        let args = commands::create_tag(
            &c.name,
            &base,
            c.force,
            c.sign,
            file.as_ref().map(|f| f.to_string_lossy()).as_deref(),
        );
        let out = git.output(args)?;
        if let Some(f) = file {
            let _ = std::fs::remove_file(f);
        }
        out
    } else {
        git.output(commands::create_branch(&c.name, &base, c.force, c.track))?
    };
    if !out.success() {
        msgbox::error("KoopaGit", &format!("{}{}", out.stdout, out.stderr));
        return Ok(1);
    }
    if is_tag {
        if c.push {
            spawn_launcher(
                &ctx.launch("push")
                    .with("branch", format!("refs/tags/{}", c.name)),
            );
        }
        return Ok(0);
    }
    let description = c.message.replace('\r', "");
    let key = format!("branch.{}.description", c.name);
    if description.trim().is_empty() {
        let _ = git.output(["config", "--unset", &key]);
    } else {
        let _ = git.output(["config", &key, description.trim()]);
    }
    if c.switch_to {
        let ok = perform_switch(
            ctx,
            &SwitchOptions {
                version: c.name,
                ..Default::default()
            },
        )?;
        return Ok(if ok { 0 } else { 1 });
    }
    Ok(0)
}

// ---------------------------------------------------------------- Merge

const STRATEGIES: [&str; 5] = ["", "resolve", "recursive", "ours", "subtree"];
const STRATEGY_OPTIONS: [&str; 11] = [
    "",
    "ours",
    "theirs",
    "patience",
    "ignore-space-change",
    "ignore-all-space",
    "ignore-space-at-eol",
    "renormalize",
    "no-renormalize",
    "rename-threshold",
    "subtree",
];
const AUTO_MESSAGE: &str = "<Auto Generated by Git>";

/// `/command:merge` and `/command:merge /abort`.
pub fn merge(ctx: &mut Context) -> DialogResult<i32> {
    if ctx.invocation.has("abort") {
        return merge_abort(ctx);
    }
    if !check_user_data(ctx) {
        return Ok(1);
    }
    // Upstream refuses to merge while a (TortoiseGit) rebase is in progress.
    if ctx.repo.state().rebase_active {
        crate::msgbox::error(
            "KoopaGit",
            "A rebase is in progress. Continue or abort the rebase before merging.",
        );
        return Ok(1);
    }
    let repo = &ctx.repo;
    let versions = Versions::load(ctx, true);
    let current = repo.current_branch()?.unwrap_or_default();
    let dlg = MergeDialog::new()?;
    dlg.set_dialog_title(ctx.window_title("Merge").into());
    dlg.set_current_branch(current.clone().into());
    dlg.set_branches(string_model(&versions.branches));
    dlg.set_tags(string_model(&versions.tags));
    // Preselect the tracked/push remote branch of the current branch.
    let git = repo.git();
    let cfg = |k: String| {
        git.run(["config", "--get", &k])
            .map(|v| v.trim().to_owned())
            .unwrap_or_default()
    };
    let remote = cfg(format!("branch.{current}.pushremote"));
    let remote = if remote.is_empty() {
        cfg(format!("branch.{current}.remote"))
    } else {
        remote
    };
    let rb = cfg(format!("branch.{current}.merge"));
    let rb = rb.trim_start_matches("refs/heads/");
    let initial = ctx
        .invocation
        .get("rev")
        .map(str::to_owned)
        .unwrap_or_else(|| format!("remotes/{remote}/{rb}"));
    let (mode, b, t) = versions.locate(&initial);
    if mode != 3 {
        dlg.set_mode(mode);
        dlg.set_branch_index(b);
        dlg.set_tag_index(t);
    } else if ctx.invocation.has("rev") {
        // `/rev:<commit>` from the log's "Merge to ..." entry.
        dlg.set_mode(3);
        dlg.set_commit_text(initial.clone().into());
    }
    let log = cfg("merge.log".to_owned()).parse::<i32>().unwrap_or(0);
    dlg.set_log_count(if log > 0 { log } else { 20 });

    let chosen: Rc<RefCell<Option<MergeOptions>>> = Rc::default();
    let versions = Rc::new(versions);
    {
        let weak = dlg.as_weak();
        let chosen = chosen.clone();
        let versions = versions.clone();
        dlg.on_ok(move || {
            let Some(d) = weak.upgrade() else { return };
            let version = versions.selected(
                d.get_mode(),
                d.get_branch_index(),
                d.get_tag_index(),
                d.get_commit_text().as_str(),
            );
            if version.is_empty() {
                d.set_notice_text("Please select a branch, tag or commit.".into());
                return;
            }
            let strategy = STRATEGIES[d.get_strategy_index().clamp(0, 4) as usize];
            let option = STRATEGY_OPTIONS[d.get_option_index().clamp(0, 10) as usize];
            let message = d.get_message().to_string();
            let message_file =
                (!d.get_squash() && message.trim() != AUTO_MESSAGE && !message.trim().is_empty())
                    .then(|| crate::temp_message_file(&message).ok())
                    .flatten()
                    .map(|p| p.to_string_lossy().into_owned());
            *chosen.borrow_mut() = Some(MergeOptions {
                version,
                no_ff: d.get_no_ff(),
                ff_only: d.get_ff_only(),
                squash: d.get_squash(),
                no_commit: d.get_no_commit(),
                allow_unrelated_histories: false,
                log: d.get_use_log().then_some(d.get_log_count().max(1) as u32),
                strategy: (!strategy.is_empty()).then(|| strategy.to_owned()),
                strategy_option: (strategy == "recursive" && !option.is_empty())
                    .then(|| option.to_owned()),
                strategy_param: matches!(option, "rename-threshold" | "subtree")
                    .then(|| d.get_strategy_param().to_string())
                    .filter(|p| !p.is_empty()),
                message_file,
            });
            let _ = d.hide();
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
    crate::window::prepare(&dlg);
    dlg.run()?;
    let Some(opts) = chosen.take() else {
        return Ok(1);
    };
    let mut post = vec![
        PostAction::on_failure("Resolve", ctx.launch("resolve")),
        PostAction::on_failure("Commit", ctx.launch("commit")),
        // `AppUtils.cpp:3313-3318`: offered when the histories share no merge base.
        PostAction::git(
            "Merge unrelated history",
            When::FailureWith("refusing to merge unrelated histories"),
            vec![commands::merge(&MergeOptions {
                allow_unrelated_histories: true,
                ..opts.clone()
            })],
        ),
        PostAction::on_failure("Stash Save", ctx.launch("stashsave")),
    ];
    if opts.no_commit || opts.squash {
        post.push(PostAction::on_success("Commit", ctx.launch("commit")));
    } else {
        // "Remove branch" for a merged local branch (`AppUtils.cpp:3350`).
        let is_local_branch = versions.branches.contains(&opts.version)
            && !opts.version.starts_with("remotes/")
            && opts.version != "FETCH_HEAD";
        if is_local_branch {
            post.push(PostAction::git(
                "Remove branch",
                When::Success,
                vec![vec![
                    "branch".to_owned(),
                    "-D".to_owned(),
                    "--".to_owned(),
                    opts.version.clone(),
                ]],
            ));
        }
        post.push(PostAction::on_success("Push", ctx.launch("push")));
    }
    let ok = crate::progress::run(ProgressJob {
        title: ctx.window_title("Merge"),
        git: ctx.repo.git(),
        commands: vec![commands::merge(&opts)],
        post_actions: post,
        close_on_end: ctx.invocation.close_on_end().unwrap_or_default(),
    })?;
    if let Some(f) = &opts.message_file {
        let _ = std::fs::remove_file(f);
    }
    Ok(if ok { 0 } else { 1 })
}

/// `CAppUtils::MergeAbort` — Abort Merge dialog then `git reset`.
fn merge_abort(ctx: &mut Context) -> DialogResult<i32> {
    let dlg = AbortMergeDialog::new()?;
    dlg.set_dialog_title(ctx.window_title("Abort Merge").into());
    let accepted = Rc::new(RefCell::new(false));
    {
        let weak = dlg.as_weak();
        let accepted = accepted.clone();
        dlg.on_ok(move || {
            *accepted.borrow_mut() = true;
            if let Some(d) = weak.upgrade() {
                let _ = d.hide();
            }
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
    {
        let workdir = ctx.repo.workdir().to_owned();
        dlg.on_show_modified(move || {
            spawn_launcher(&Invocation::new("repostatus").with_paths([workdir.clone()]));
        });
    }
    crate::window::prepare(&dlg);
    dlg.run()?;
    if !*accepted.borrow() {
        return Ok(1);
    }
    let kind = match dlg.get_reset_type() {
        1 => ResetType::Mixed,
        2 => ResetType::Hard,
        _ => ResetType::Merge,
    };
    let ok = crate::progress::run(ProgressJob {
        title: ctx.window_title("Reset"),
        git: ctx.repo.git(),
        commands: vec![commands::reset(kind, "HEAD")],
        post_actions: vec![],
        close_on_end: ctx.invocation.close_on_end().unwrap_or_default(),
    })?;
    Ok(if ok { 0 } else { 1 })
}

// ---------------------------------------------------------------- Stash

const INCLUDE_UNTRACKED_WARNING: &str = "You have checked \"include untracked\". This includes a \"git clean -fd\" call, so ignored files/directories are removed w/o using recycle bin.";

/// `/command:stashsave [/msg:<text>]`.
pub fn stash_save(ctx: &mut Context) -> DialogResult<i32> {
    if !check_user_data(ctx) {
        return Ok(1);
    }
    let dlg = StashDialog::new()?;
    dlg.set_dialog_title(ctx.window_title("Stash").into());
    dlg.set_message(ctx.invocation.get("msg").unwrap_or_default().into());
    let skip_warning = ctx.settings.bool("NoStashIncludeUntrackedWarning", false);
    let chosen: Rc<RefCell<Option<(String, StashUntracked)>>> = Rc::default();
    let finish = {
        let weak = dlg.as_weak();
        let chosen = chosen.clone();
        move || {
            let Some(d) = weak.upgrade() else { return };
            let untracked = if d.get_include_untracked() {
                StashUntracked::IncludeUntracked
            } else if d.get_all() {
                StashUntracked::All
            } else {
                StashUntracked::None
            };
            *chosen.borrow_mut() = Some((d.get_message().to_string(), untracked));
            let _ = d.hide();
        }
    };
    let finish = Rc::new(finish);
    {
        let weak = dlg.as_weak();
        let finish = finish.clone();
        dlg.on_ok(move || {
            let Some(d) = weak.upgrade() else { return };
            if d.get_include_untracked() && !skip_warning {
                d.set_notice_text(INCLUDE_UNTRACKED_WARNING.into());
                d.set_notice_buttons(string_model(&["Continue".into(), "Abort".into()]));
                return;
            }
            finish();
        });
    }
    {
        let weak = dlg.as_weak();
        let finish = finish.clone();
        dlg.on_notice_clicked(move |i| {
            let Some(d) = weak.upgrade() else { return };
            d.set_notice_text("".into());
            d.set_notice_buttons(string_model(&[]));
            if i == 0 {
                finish();
            }
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
    crate::window::prepare(&dlg);
    dlg.run()?;
    let Some((message, untracked)) = chosen.take() else {
        return Ok(1);
    };
    let ok = crate::progress::run(ProgressJob {
        title: ctx.window_title("Stash"),
        git: ctx.repo.git(),
        commands: vec![commands::stash_save(&message, untracked)],
        post_actions: vec![
            PostAction::on_success("Stash Pop", ctx.launch("stashpop")),
            PostAction::on_success("Stash Apply", ctx.launch("stashapply")),
        ],
        close_on_end: ctx.invocation.close_on_end().unwrap_or_default(),
    })?;
    Ok(if ok { 0 } else { 1 })
}

/// `/command:stashapply` and `/command:stashpop`.
pub fn stash_apply_pop(ctx: &mut Context, pop: bool) -> DialogResult<i32> {
    let args = if pop {
        commands::stash_pop()
    } else {
        commands::stash_apply(ctx.invocation.get("ref").unwrap_or_default())
    };
    let out = ctx.repo.git().output(args)?;
    let text = format!("{}{}", out.stdout, out.stderr);
    let conflicts = text.contains("CONFLICT");
    let what = if pop { "Stash Pop" } else { "Stash Apply" };
    if out.success() || (out.code == Some(1) && conflicts) {
        let message = if conflicts {
            format!(
                "{what} successful, but there are conflicts.\n\n{text}\nDo you want to see the changes?"
            )
        } else {
            format!("{what} successful.\n\n{text}\nDo you want to see the changes?")
        };
        if msgbox::ask(&ctx.window_title(what), &message, &["Yes", "No"]) == Some(0) {
            spawn_launcher(&ctx.launch("repostatus"));
        }
        Ok(0)
    } else {
        msgbox::error(
            &ctx.window_title(what),
            &format!("{what} failed!\n\n{text}"),
        );
        Ok(1)
    }
}

/// Runs a plain progress job with a close-on-end default (used by other modules).
pub(crate) fn run_job(
    ctx: &Context,
    title: &str,
    commands: Vec<Vec<String>>,
    post_actions: Vec<PostAction>,
) -> DialogResult<bool> {
    Ok(crate::progress::run(ProgressJob {
        title: ctx.window_title(title),
        git: ctx.repo.git(),
        commands,
        post_actions,
        close_on_end: ctx.invocation.close_on_end().unwrap_or(
            match ctx.settings.dword("AutoCloseGitProgress", 0) {
                1 => CloseOnEnd::IfNoOptions,
                2 => CloseOnEnd::IfNoErrors,
                _ => CloseOnEnd::Manual,
            },
        ),
    })?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_selection() {
        let v = Versions {
            branches: vec!["main".into(), "v1".into(), "remotes/origin/main".into()],
            tags: vec!["v1".into(), "v2".into()],
        };
        assert_eq!(v.selected(1, 0, 0, ""), "main");
        assert_eq!(v.selected(1, 1, 0, ""), "refs/heads/v1");
        assert_eq!(v.selected(2, 0, 0, ""), "refs/tags/v1");
        assert_eq!(v.selected(2, 0, 1, ""), "v2");
        assert_eq!(v.selected(3, 0, 0, " abc "), "abc");
        assert_eq!(v.selected(0, 0, 0, ""), "HEAD");
        assert_eq!(v.locate("refs/heads/main"), (1, 0, 0));
        assert_eq!(v.locate("remotes/origin/main"), (1, 2, 0));
        assert_eq!(v.locate("v2"), (2, 0, 1));
        assert_eq!(v.locate("1234abc"), (3, 0, 0));
        assert_eq!(
            remote_branch_part("remotes/origin/dev", &["origin".into()]),
            Some("dev".into())
        );
        assert_eq!(remote_branch_part("main", &["origin".into()]), None);
    }
}
