//! `/command:rebase [/upstream:<ref>] [/force]` and the cherry-pick mode
//! `/command:rebase /cherrypick /revs:<sha>,<sha>,...` (oldest first) —
//! `CRebaseDlg`.
//!
//! The replay itself is [`koopagit_core::rebase::Sequencer`] (TortoiseGit's
//! cherry-pick engine). The list shows the newest commit at the top; the bottom
//! row is the oldest commit, which "Squash (with commit below)" can't target.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use koopagit_core::Repo;
use koopagit_core::git_cli::Git;
use koopagit_core::launcher::Invocation;
use koopagit_core::rebase::{
    self as engine, Plan, Progress, RebaseAction, Sequencer, Step, StepState, Stop,
};
use koopagit_core::settings::Settings;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::menus::ActionId;
use crate::menus::rebase::{self as menu, RebaseMenuAction as M, RebaseMenuInput, cycle_action};
use crate::refs::{check_user_data, string_model};
use crate::rowsel::ListSel;
use crate::{Context, DialogResult, RebaseDialog, RebaseRow, spawn_launcher};

const ADD_CHERRY_PICKED_FROM: &str = r"TortoiseProc\CherrypickAddCherryPickedFrom";

fn action_code(a: RebaseAction) -> i32 {
    match a {
        RebaseAction::Pick => 0,
        RebaseAction::Squash => 1,
        RebaseAction::Edit => 2,
        RebaseAction::Skip => 3,
    }
}

fn action_from(i: i32) -> RebaseAction {
    match i {
        1 => RebaseAction::Squash,
        2 => RebaseAction::Edit,
        3 => RebaseAction::Skip,
        _ => RebaseAction::Pick,
    }
}

/// Whether `row` (display order) may be squashed: not the bottom row, and
/// outside cherry-pick mode only commits with exactly one parent.
fn can_squash(steps: &[Step], row: usize, cherry_pick: bool) -> bool {
    row + 1 < steps.len() && (cherry_pick || steps[row].parents.len() == 1)
}

/// Sets `action` on the given rows, skipping started rows and rows that can't
/// be squashed (`SetSelectedRebaseAction`).
pub fn set_rows(steps: &mut [Step], rows: &[usize], action: RebaseAction, cherry_pick: bool) {
    for &r in rows {
        if r >= steps.len() || steps[r].state != StepState::Pending {
            continue;
        }
        if action == RebaseAction::Squash && !can_squash(steps, r, cherry_pick) {
            continue;
        }
        steps[r].action = action;
    }
}

/// The split button: 0 Pick ALL, 1 Squash ALL, 2 Edit ALL, 3 Skip unselected,
/// 4 Squash unselected, 5 Edit unselected.
pub fn set_all(steps: &mut [Step], code: i32, selected: &[usize], cherry_pick: bool) {
    let (action, only_unselected) = match code {
        0 => (RebaseAction::Pick, false),
        1 => (RebaseAction::Squash, false),
        2 => (RebaseAction::Edit, false),
        3 => (RebaseAction::Skip, true),
        4 => (RebaseAction::Squash, true),
        _ => (RebaseAction::Edit, true),
    };
    let rows: Vec<usize> = (0..steps.len())
        .filter(|r| !only_unselected || !selected.contains(r))
        .collect();
    set_rows(steps, &rows, action, cherry_pick);
}

/// Moves the selected rows by one (or to the top/bottom with `to_end`),
/// keeping them as a block. Returns the new positions of the moved rows.
pub fn move_rows(
    steps: &mut Vec<Step>,
    selected: &[usize],
    delta: i32,
    to_end: bool,
) -> Vec<usize> {
    let mut sel: Vec<usize> = selected
        .iter()
        .copied()
        .filter(|&r| r < steps.len())
        .collect();
    sel.sort_unstable();
    if sel.is_empty() {
        return sel;
    }
    let picked: Vec<Step> = sel.iter().map(|&r| steps[r].clone()).collect();
    let mut rest: Vec<Step> = steps
        .iter()
        .enumerate()
        .filter(|(i, _)| !sel.contains(i))
        .map(|(_, s)| s.clone())
        .collect();
    let first = sel[0];
    let insert_at = if to_end {
        if delta < 0 { 0 } else { rest.len() }
    } else if delta < 0 {
        first.saturating_sub(1)
    } else {
        (first + 1).min(rest.len())
    };
    let n = picked.len();
    rest.splice(insert_at..insert_at, picked);
    *steps = rest;
    (insert_at..insert_at + n).collect()
}

/// `CheckRebaseCondition` for the chosen branch/upstream.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Condition {
    FastForward,
    UpToDate,
    Normal,
}

fn condition(repo: &Repo, branch: &str, upstream: &str, force: bool) -> Condition {
    // The merge base equals the upstream (this includes identical refs).
    if !force && repo.is_ancestor(upstream, branch) {
        return Condition::UpToDate;
    }
    if repo.is_ancestor(branch, upstream) {
        return Condition::FastForward;
    }
    Condition::Normal
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Choose,
    Stopped,
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stashed {
    No,
    /// `rebase.autostash`: pop silently at the end.
    Auto,
    /// The user agreed to stash: ask before popping.
    Asked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    None,
    StashBeforeStart,
    /// The pre-rebase hook failed: Abort / Proceed anyway.
    HookFailed,
    PopAfter,
    Finished,
    Conflict,
}

struct State {
    workdir: PathBuf,
    cherry_pick: bool,
    branches: Vec<String>,
    upstreams: Vec<String>,
    /// Display order: newest first.
    steps: Vec<Step>,
    seq: Option<Sequencer>,
    stage: Stage,
    stashed: Stashed,
    pending: Pending,
    /// The user chose "Proceed anyway" after the pre-rebase hook failed.
    hook_overridden: bool,
    settings: Settings,
}

pub fn run(ctx: &mut Context) -> DialogResult<i32> {
    if !check_user_data(ctx) {
        return Ok(1);
    }
    let repo = &ctx.repo;
    let inv = ctx.invocation.clone();
    let cherry_pick = inv.has("cherrypick");
    let current = repo.current_branch()?.unwrap_or_default();
    let branches = repo.local_branches()?;
    let mut upstreams = branches.clone();
    upstreams.extend(
        repo.remote_branches()?
            .into_iter()
            .map(|b| format!("remotes/{b}")),
    );
    if repo.git_dir().join("FETCH_HEAD").exists() {
        upstreams.push("FETCH_HEAD".to_owned());
    }
    upstreams.extend(repo.tags()?);
    if let Some(u) = inv.get("upstream").filter(|u| !u.is_empty()) {
        if !upstreams.iter().any(|x| x == u) {
            upstreams.push(u.to_owned());
        }
    }

    let dlg = RebaseDialog::new()?;
    dlg.set_dialog_title(
        ctx.window_title(if cherry_pick { "Cherry Pick" } else { "Rebase" })
            .into(),
    );
    dlg.set_cherry_pick(cherry_pick);
    dlg.set_branches(string_model(&branches));
    dlg.set_upstreams(string_model(&upstreams));
    dlg.set_force(inv.has("force"));
    dlg.set_add_cherry_picked_from(ctx.settings.bool(ADD_CHERRY_PICKED_FROM, false));
    dlg.set_continue_text(if cherry_pick { "Start" } else { "Start Rebase" }.into());
    dlg.set_branch_index(
        branches
            .iter()
            .position(|b| *b == current)
            .map_or(-1, |i| i as i32),
    );
    let default_upstream = inv.get("upstream").map(str::to_owned).or_else(|| {
        let git = repo.git();
        let get = |k: String| {
            git.run(["config", "--get", &k])
                .ok()
                .map(|v| v.trim().to_owned())
        };
        let remote = get(format!("branch.{current}.remote"))?;
        let merge = get(format!("branch.{current}.merge"))?;
        Some(format!(
            "remotes/{remote}/{}",
            merge.trim_start_matches("refs/heads/")
        ))
    });
    dlg.set_upstream_index(
        default_upstream
            .and_then(|u| upstreams.iter().position(|x| *x == u))
            .map_or(-1, |i| i as i32),
    );

    let mut initial_steps = Vec::new();
    if cherry_pick {
        let revs: Vec<String> = inv
            .get("revs")
            .unwrap_or_default()
            .split(',')
            .filter(|r| !r.trim().is_empty())
            .map(|r| r.trim().to_owned())
            .collect();
        initial_steps = engine::cherry_pick_steps(repo, &revs)?;
        initial_steps.reverse();
    }

    let state = Rc::new(RefCell::new(State {
        workdir: repo.workdir().to_owned(),
        cherry_pick,
        branches,
        upstreams,
        steps: initial_steps,
        seq: None,
        stage: Stage::Choose,
        stashed: Stashed::No,
        pending: Pending::None,
        hook_overridden: false,
        settings: ctx.settings.clone(),
    }));
    let sel = Rc::new(ListSel::default());
    let model = Rc::new(VecModel::<RebaseRow>::default());
    dlg.set_rows(ModelRc::from(model.clone()));

    let ui = Rc::new(Ui {
        state: state.clone(),
        sel: sel.clone(),
        model: model.clone(),
        weak: dlg.as_weak(),
    });
    ui.refresh_list();
    {
        let ui = ui.clone();
        dlg.on_refs_changed(move || ui.refresh_list());
    }
    {
        let ui = ui.clone();
        dlg.on_refresh(move || ui.refresh_list());
    }
    {
        let ui = ui.clone();
        dlg.on_options_changed(move || ui.save_options());
    }
    {
        let ui = ui.clone();
        dlg.on_swap(move || ui.swap());
    }
    {
        let ui = ui.clone();
        dlg.on_set_action(move |a| ui.set_action(a));
    }
    {
        let ui = ui.clone();
        dlg.on_set_all(move |code| {
            let selected = ui.sel.sel.borrow().to_vec();
            let cp = ui.state.borrow().cherry_pick;
            set_all(&mut ui.state.borrow_mut().steps, code, &selected, cp);
            ui.paint();
        });
    }
    {
        let ui = ui.clone();
        dlg.on_move_rows(move |delta, to_end| ui.move_rows(delta, to_end));
    }
    {
        let ui = ui.clone();
        dlg.on_row_clicked(move |row, toggle, range| {
            ui.sel.click(row as usize, toggle, range);
            ui.paint();
        });
    }
    {
        let ui = ui.clone();
        dlg.on_row_right(move |row| {
            ui.sel.right(row as usize);
            ui.paint();
        });
    }
    {
        let ui = ui.clone();
        dlg.on_key_step(move |delta, extend| {
            let n = ui.model.row_count();
            ui.sel.key(delta, extend, n);
            ui.paint();
        });
    }
    {
        let ui = ui.clone();
        dlg.on_menu_for(move |row| {
            let ui2 = ui.clone();
            ui.sel.cache.get(row, move || ui2.menu(row as usize))
        });
    }
    {
        let ui = ui.clone();
        dlg.on_menu_action(move |row, id| ui.menu_action(row as usize, id));
    }
    {
        let ui = ui.clone();
        dlg.on_go(move || ui.go());
    }
    {
        let ui = ui.clone();
        dlg.on_skip(move || {
            let r = ui.with_seq(|seq, repo| seq.skip_current(repo));
            ui.after(r);
        });
    }
    {
        let ui = ui.clone();
        dlg.on_abort(move || ui.abort());
    }
    {
        let ui = ui.clone();
        dlg.on_notice_clicked(move |i| ui.notice_clicked(i));
    }
    crate::snapshot::arm(&dlg);
    dlg.run()?;
    let finished = state.borrow().stage == Stage::Finished;
    Ok(if finished { 0 } else { 1 })
}

struct Ui {
    state: Rc<RefCell<State>>,
    sel: Rc<ListSel>,
    model: Rc<VecModel<RebaseRow>>,
    weak: slint::Weak<RebaseDialog>,
}

impl Ui {
    fn dlg(&self) -> Option<RebaseDialog> {
        self.weak.upgrade()
    }

    fn repo(&self) -> Option<Repo> {
        Repo::discover(&self.state.borrow().workdir).ok()
    }

    fn launch(&self, command: &str) -> Invocation {
        Invocation::new(command).with_paths([self.state.borrow().workdir.clone()])
    }

    fn notice(&self, pending: Pending, text: &str, buttons: &[&str], error: bool) {
        self.state.borrow_mut().pending = pending;
        if let Some(d) = self.dlg() {
            d.set_notice_text(text.into());
            let labels: Vec<SharedString> = buttons.iter().map(|b| (*b).into()).collect();
            d.set_notice_buttons(ModelRc::new(VecModel::from(labels)));
            d.set_notice_error(error);
        }
    }

    fn selection(&self) -> Option<(String, String)> {
        let d = self.dlg()?;
        let st = self.state.borrow();
        let b = st.branches.get(d.get_branch_index() as usize)?.clone();
        let u = st.upstreams.get(d.get_upstream_index() as usize)?.clone();
        Some((b, u))
    }

    /// Pushes the steps (and the selection) into the list model.
    fn paint(&self) {
        let st = self.state.borrow();
        let rows: Vec<RebaseRow> = st
            .steps
            .iter()
            .enumerate()
            .map(|(i, s)| RebaseRow {
                action: action_code(s.action),
                hash: s.id[..s.id.len().min(8)].into(),
                subject: s.subject.as_str().into(),
                author: s.author.as_str().into(),
                state: match s.state {
                    StepState::Pending => 0,
                    StepState::Current => 1,
                    StepState::Done => 2,
                },
                selected: self.sel.is_selected(i),
            })
            .collect();
        drop(st);
        self.model.set_vec(rows);
        self.sel.cache.invalidate();
    }

    /// Recomputes the commit list for the chosen branch/upstream (rebase mode).
    fn refresh_list(&self) {
        let Some(d) = self.dlg() else { return };
        if self.state.borrow().stage != Stage::Choose {
            return;
        }
        let Some(repo) = self.repo() else { return };
        if self.state.borrow().cherry_pick {
            let n = self.state.borrow().steps.len();
            d.set_status_text(format!("{n} commit(s) will be cherry-picked.").into());
            d.set_continue_enabled(n > 0);
            self.paint();
            return;
        }
        let Some((branch, upstream)) = self.selection() else {
            self.state.borrow_mut().steps.clear();
            self.paint();
            d.set_status_text("Select a branch and an upstream.".into());
            d.set_continue_enabled(false);
            return;
        };
        let mut steps = engine::rebase_steps(&repo, &upstream, &branch, d.get_preserve_merges())
            .unwrap_or_default();
        steps.reverse();
        let n = steps.len();
        self.state.borrow_mut().steps = steps;
        self.sel.reset(n, false);
        self.paint();
        let onto = d.get_onto_enabled() && !d.get_onto_text().trim().is_empty();
        match condition(&repo, &branch, &upstream, d.get_force()) {
            Condition::FastForward if !onto => {
                d.set_status_text(
                    format!("Fast-forward: \"{branch}\" will be moved to \"{upstream}\".").into(),
                );
                d.set_continue_enabled(true);
            }
            Condition::UpToDate if !onto => {
                d.set_status_text("Current branch is up to date.".into());
                d.set_continue_enabled(false);
            }
            _ => {
                d.set_status_text(
                    format!("{n} commit(s) will be rebased onto \"{upstream}\".").into(),
                );
                d.set_continue_enabled(n > 0);
            }
        }
    }

    fn save_options(&self) {
        let Some(d) = self.dlg() else { return };
        let mut st = self.state.borrow_mut();
        st.settings
            .set_bool(ADD_CHERRY_PICKED_FROM, d.get_add_cherry_picked_from());
        let _ = st.settings.save();
    }

    fn swap(&self) {
        let Some(d) = self.dlg() else { return };
        let Some((branch, upstream)) = self.selection() else {
            return;
        };
        let st = self.state.borrow();
        let (Some(b), Some(u)) = (
            st.branches.iter().position(|x| *x == upstream),
            st.upstreams.iter().position(|x| *x == branch),
        ) else {
            return;
        };
        drop(st);
        d.set_branch_index(b as i32);
        d.set_upstream_index(u as i32);
        self.refresh_list();
    }

    fn set_action(&self, code: i32) {
        let selected = self.sel.sel.borrow().to_vec();
        let cp = self.state.borrow().cherry_pick;
        let mut st = self.state.borrow_mut();
        if code == 4 {
            let n = st.steps.len();
            for &r in &selected {
                if r < n && st.steps[r].state == StepState::Pending {
                    let mut next = cycle_action(action_code(st.steps[r].action), r + 1 == n);
                    if next == 1 && !can_squash(&st.steps, r, cp) {
                        next = 0;
                    }
                    st.steps[r].action = action_from(next);
                }
            }
        } else {
            set_rows(&mut st.steps, &selected, action_from(code), cp);
        }
        drop(st);
        self.paint();
    }

    fn move_rows(&self, delta: i32, to_end: bool) {
        let selected = self.sel.sel.borrow().to_vec();
        let moved = move_rows(&mut self.state.borrow_mut().steps, &selected, delta, to_end);
        let mut s = self.sel.sel.borrow_mut();
        *s = Default::default();
        for (k, r) in moved.iter().enumerate() {
            s.click(
                *r,
                crate::selection::Mods {
                    toggle: k > 0,
                    range: false,
                },
            );
        }
        drop(s);
        self.paint();
    }

    fn menu(&self, row: usize) -> Vec<crate::menus::Entry> {
        let rows = self.sel.effective(row);
        let st = self.state.borrow();
        let focused = st.steps.get(row);
        menu::build(&RebaseMenuInput {
            selected: rows,
            row_count: st.steps.len(),
            editable: st.stage == Stage::Choose,
            focused_started: focused.is_some_and(|s| s.state != StepState::Pending),
            focused_parents: focused.map_or(1, |s| s.parents.len()),
            cherry_pick: st.cherry_pick,
        })
    }

    fn menu_action(&self, row: usize, id: i32) {
        let Some(action) = M::from_id(id) else { return };
        self.sel.right(row);
        let rows = self.sel.effective(row);
        let ids: Vec<String> = {
            let st = self.state.borrow();
            rows.iter()
                .filter_map(|&r| st.steps.get(r).map(|s| s.id.clone()))
                .collect()
        };
        let Some(id) = ids.first().cloned() else {
            return;
        };
        let workdir = self.state.borrow().workdir.clone();
        let copy = |format: &str| {
            let out = Git::new(&workdir)
                .run(
                    ["log", "--no-walk=unsorted", format]
                        .into_iter()
                        .map(str::to_owned)
                        .chain(ids.iter().cloned()),
                )
                .unwrap_or_default()
                .replace('\n', "\r\n");
            let _ = crate::clipboard::copy(out.trim_end());
        };
        match action {
            M::Pick => self.set_action(0),
            M::Squash => self.set_action(1),
            M::Edit => self.set_action(2),
            M::Skip => self.set_action(3),
            M::ShowLog => spawn_launcher(
                &self
                    .launch("log")
                    .with("endrev", id.clone())
                    .with("rev", id),
            ),
            M::UnifiedDiff => spawn_launcher(
                &self
                    .launch("showcompare")
                    .with("revision1", format!("{id}^"))
                    .with("revision2", id)
                    .with_switch("unified"),
            ),
            M::CompareWithPrevious => spawn_launcher(
                &self
                    .launch("showcompare")
                    .with("revision1", format!("{id}^"))
                    .with("revision2", id),
            ),
            M::CreateBranch => spawn_launcher(&self.launch("branch").with("rev", id)),
            M::CreateTag => spawn_launcher(&self.launch("tag").with("rev", id)),
            M::Export => spawn_launcher(&self.launch("export").with("rev", id)),
            M::CopyFull => {
                copy("--format=Revision: %H%nAuthor: %an <%ae>%nDate: %ad%nMessage:%n%B")
            }
            M::CopySha => copy("--format=%H"),
            M::CopySubjects => copy("--format=* %s%n"),
            M::CopyMessages => copy("--format=* %B%n"),
        }
    }

    fn with_seq(
        &self,
        f: impl FnOnce(&mut Sequencer, &Repo) -> koopagit_core::Result<Progress>,
    ) -> koopagit_core::Result<Progress> {
        let repo = Repo::discover(&self.state.borrow().workdir)?;
        let mut st = self.state.borrow_mut();
        match st.seq.as_mut() {
            Some(seq) => f(seq, &repo),
            None => Ok(Progress::Finished),
        }
    }

    fn go(&self) {
        let stage = self.state.borrow().stage;
        match stage {
            Stage::Choose => self.start(),
            Stage::Stopped => {
                let msg = self
                    .dlg()
                    .filter(|d| d.get_message_editable())
                    .map(|d| d.get_message_text().to_string());
                let r = self.with_seq(|seq, repo| seq.continue_with(repo, msg.as_deref()));
                self.after(r);
            }
            Stage::Finished => {
                if let Some(d) = self.dlg() {
                    let _ = d.hide();
                }
            }
        }
    }

    fn start(&self) {
        let Some(d) = self.dlg() else { return };
        let Some(repo) = self.repo() else { return };
        let cherry_pick = self.state.borrow().cherry_pick;
        let git = repo.git();
        // `CheckRebaseCondition` (RebaseDlg.cpp:946): a dirty working tree
        // is stashed first, then the pre-rebase hook runs, and only then the
        // fast-forward or the rebase itself.
        let dirty = git
            .run(["status", "--porcelain", "--untracked-files=no"])
            .map(|o| !o.trim().is_empty())
            .unwrap_or(false);
        if dirty && self.state.borrow().stashed == Stashed::No {
            let autostash = !cherry_pick
                && git
                    .run(["config", "--bool", "--get", "rebase.autostash"])
                    .is_ok_and(|v| v.trim() == "true");
            if autostash {
                if git.run(["stash"]).is_ok() {
                    self.state.borrow_mut().stashed = Stashed::Auto;
                }
            } else {
                self.notice(
                    Pending::StashBeforeStart,
                    "The current working tree is not clean. Do you want to stash the changes?",
                    &["Stash", "Abort"],
                    false,
                );
                return;
            }
        }
        if !cherry_pick {
            let Some((branch, upstream)) = self.selection() else {
                return;
            };
            if !self.state.borrow().hook_overridden {
                if let Err(e) = crate::hookrun::pre_rebase(repo.workdir(), &upstream, &branch) {
                    // IDS_HOOKFAILED_TASK2..4; "Abort" is the default.
                    self.notice(
                        Pending::HookFailed,
                        &format!("Hook script failed\n{e}"),
                        &["Abort", "Proceed anyway"],
                        true,
                    );
                    return;
                }
            }
            let onto = d.get_onto_enabled() && !d.get_onto_text().trim().is_empty();
            if !onto
                && condition(&repo, &branch, &upstream, d.get_force()) == Condition::FastForward
            {
                // Fast-forward: move the branch to the upstream.
                let r = git
                    .run([
                        "checkout",
                        "--no-track",
                        "-f",
                        "-B",
                        &branch,
                        "--end-of-options",
                        &upstream,
                        "--",
                    ])
                    .and_then(|_| {
                        git.run(["reset", "--hard", "--end-of-options", &upstream, "--"])
                    });
                match r {
                    Ok(_) => self.finished("Fast-forwarded."),
                    Err(e) => self.notice(Pending::None, &e.to_string(), &[], true),
                }
                return;
            }
        }
        // Oldest first for the engine.
        let mut steps = self.state.borrow().steps.clone();
        steps.reverse();
        let (branch, onto) = match self.selection() {
            Some((b, u)) if !cherry_pick => {
                let onto = if d.get_onto_enabled() && !d.get_onto_text().trim().is_empty() {
                    d.get_onto_text().trim().to_owned()
                } else {
                    u
                };
                (Some(b), onto)
            }
            _ => (None, String::new()),
        };
        let plan = Plan {
            branch,
            onto,
            steps,
            cherry_pick,
            add_cherry_picked_from: cherry_pick && d.get_add_cherry_picked_from(),
            preserve_merges: !cherry_pick && d.get_preserve_merges(),
        };
        match Sequencer::start(&repo, plan) {
            Ok(seq) => {
                let mut st = self.state.borrow_mut();
                st.seq = Some(seq);
                st.stage = Stage::Stopped;
                drop(st);
                d.set_editable(false);
                d.set_continue_text("Continue".into());
                let r = self.with_seq(|seq, repo| seq.run(repo));
                self.after(r);
            }
            Err(e) => self.notice(Pending::None, &e.to_string(), &[], true),
        }
    }

    /// Updates the UI after the sequencer ran.
    fn after(&self, r: koopagit_core::Result<Progress>) {
        let Some(d) = self.dlg() else { return };
        // Mirror the engine's step states into the list (newest first).
        let (log, done, total) = {
            let mut st = self.state.borrow_mut();
            let Some(seq) = st.seq.as_ref() else { return };
            let mut steps = seq.plan.steps.clone();
            steps.reverse();
            let log = seq.log.join("\n");
            let done = steps.iter().filter(|s| s.state == StepState::Done).count();
            let total = steps.len().max(1);
            st.steps = steps;
            (log, done, total)
        };
        self.paint();
        d.set_output_text(log.into());
        d.set_progress(done as f32 / total as f32);
        d.set_message_editable(false);
        d.set_skip_visible(false);
        match r {
            Err(e) => {
                d.set_tab(1);
                d.set_skip_visible(true);
                d.set_status_text(format!("Error: {e}").into());
            }
            Ok(Progress::Finished) => {
                let finish = {
                    let mut st = self.state.borrow_mut();
                    let repo = Repo::discover(&st.workdir);
                    match (st.seq.as_mut(), repo) {
                        (Some(seq), Ok(repo)) => seq.finish(&repo).map(|_| ()),
                        _ => Ok(()),
                    }
                };
                match finish {
                    Ok(()) => self.finished("Success"),
                    Err(e) => d.set_status_text(format!("Error: {e}").into()),
                }
            }
            Ok(Progress::Stopped(stop)) => {
                d.set_skip_visible(true);
                let subject = |i: usize| {
                    let st = self.state.borrow();
                    let n = st.steps.len();
                    st.steps
                        .get(n.saturating_sub(i + 1))
                        .map(|s| format!("{} {}", &s.id[..s.id.len().min(8)], s.subject))
                        .unwrap_or_default()
                };
                match stop {
                    Stop::Edit { index, message } => {
                        d.set_tab(0);
                        d.set_message_text(message.into());
                        d.set_message_editable(true);
                        d.set_status_text(
                            format!(
                                "Edit {}: change the files or the message, then click Continue.",
                                subject(index)
                            )
                            .into(),
                        );
                    }
                    Stop::Conflict { index, output } => {
                        d.set_tab(1);
                        d.set_output_text(format!("{}\n{output}", d.get_output_text()).into());
                        d.set_status_text(
                            format!(
                                "Conflicts while applying {}. Resolve them, then click Continue.",
                                subject(index)
                            )
                            .into(),
                        );
                        self.notice(
                            Pending::Conflict,
                            "There are conflicts.",
                            &["Resolve...", "Check for modifications"],
                            true,
                        );
                    }
                    Stop::Failed { index, output } => {
                        d.set_tab(1);
                        d.set_output_text(format!("{}\n{output}", d.get_output_text()).into());
                        d.set_status_text(
                            format!(
                                "Applying {} failed. Skip it, Continue after fixing, or Abort.",
                                subject(index)
                            )
                            .into(),
                        );
                    }
                }
            }
        }
    }

    fn finished(&self, text: &str) {
        let Some(d) = self.dlg() else { return };
        let stashed = {
            let mut st = self.state.borrow_mut();
            st.stage = Stage::Finished;
            st.stashed
        };
        d.set_editable(false);
        d.set_progress(1.0);
        d.set_continue_text("Done".into());
        d.set_abort_text("Close".into());
        d.set_skip_visible(false);
        d.set_status_text(text.into());
        match stashed {
            Stashed::Auto => self.stash_pop(),
            Stashed::Asked => self.notice(
                Pending::PopAfter,
                "Do you want to stash pop now?",
                &["Yes", "No"],
                false,
            ),
            Stashed::No => self.notice(Pending::Finished, text, &["Show Log", "Push"], false),
        }
    }

    fn stash_pop(&self) {
        let workdir = self.state.borrow().workdir.clone();
        let out = Git::new(&workdir).output(["stash", "pop"]);
        self.state.borrow_mut().stashed = Stashed::No;
        if let Ok(o) = out {
            if !o.success() {
                self.notice(Pending::None, o.stderr.trim(), &[], true);
            }
        }
    }

    fn abort(&self) {
        let (stage, has_seq, stashed) = {
            let st = self.state.borrow();
            (st.stage, st.seq.is_some(), st.stashed)
        };
        if stage == Stage::Stopped && has_seq {
            let r = {
                let mut st = self.state.borrow_mut();
                let repo = Repo::discover(&st.workdir);
                match (st.seq.as_mut(), repo) {
                    (Some(seq), Ok(repo)) => seq.abort(&repo),
                    _ => Ok(()),
                }
            };
            if let Err(e) = r {
                self.notice(Pending::None, &e.to_string(), &[], true);
                return;
            }
            if stashed != Stashed::No {
                self.stash_pop();
            }
        }
        if let Some(d) = self.dlg() {
            let _ = d.hide();
        }
    }

    fn notice_clicked(&self, i: i32) {
        let pending = self.state.borrow().pending;
        self.notice(Pending::None, "", &[], false);
        match (pending, i) {
            (Pending::StashBeforeStart, 0) => {
                let workdir = self.state.borrow().workdir.clone();
                match Git::new(&workdir).run(["stash"]) {
                    Ok(_) => {
                        self.state.borrow_mut().stashed = Stashed::Asked;
                        self.start();
                    }
                    Err(e) => self.notice(Pending::None, &e.to_string(), &[], true),
                }
            }
            (Pending::HookFailed, 1) => {
                self.state.borrow_mut().hook_overridden = true;
                self.start();
            }
            (Pending::PopAfter, 0) => self.stash_pop(),
            (Pending::Finished, 0) => spawn_launcher(&self.launch("log")),
            (Pending::Finished, 1) => spawn_launcher(&self.launch("push")),
            (Pending::Conflict, 0) => spawn_launcher(&self.launch("resolve")),
            (Pending::Conflict, 1) => spawn_launcher(&self.launch("repostatus")),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(id: &str, parents: usize) -> Step {
        Step {
            id: id.into(),
            subject: id.into(),
            author: String::new(),
            parents: vec!["p".into(); parents],
            action: RebaseAction::Pick,
            state: StepState::Pending,
        }
    }

    fn actions(steps: &[Step]) -> Vec<i32> {
        steps.iter().map(|s| action_code(s.action)).collect()
    }

    #[test]
    fn set_all_rules() {
        let mut s = vec![step("c", 1), step("m", 2), step("a", 1)];
        set_all(&mut s, 1, &[], false);
        // Merge commit and the bottom row are not squashed.
        assert_eq!(actions(&s), [1, 0, 0]);
        set_all(&mut s, 3, &[0], false);
        assert_eq!(actions(&s), [1, 3, 3]);
        set_all(&mut s, 0, &[], false);
        set_all(&mut s, 1, &[], true);
        assert_eq!(actions(&s), [1, 1, 0]);
        s[0].state = StepState::Done;
        set_rows(&mut s, &[0, 1], RebaseAction::Edit, false);
        assert_eq!(actions(&s), [1, 2, 0]);
    }

    #[test]
    fn moving_blocks() {
        let mut s = vec![step("a", 1), step("b", 1), step("c", 1), step("d", 1)];
        let ids = |s: &[Step]| s.iter().map(|x| x.id.clone()).collect::<Vec<_>>().join("");
        assert_eq!(move_rows(&mut s, &[1, 2], -1, false), [0, 1]);
        assert_eq!(ids(&s), "bcad");
        assert_eq!(move_rows(&mut s, &[0, 1], 1, false), [1, 2]);
        assert_eq!(ids(&s), "abcd");
        assert_eq!(move_rows(&mut s, &[0], 1, true), [3]);
        assert_eq!(ids(&s), "bcda");
        assert_eq!(move_rows(&mut s, &[3], -1, true), [0]);
        assert_eq!(ids(&s), "abcd");
        assert_eq!(move_rows(&mut s, &[0], -1, false), [0]);
    }

    #[test]
    fn rebase_condition() {
        let dir = tempfile::tempdir().unwrap();
        let git = Git::new(dir.path());
        git.run(["init", "-q", "-b", "main"]).unwrap();
        git.run(["config", "user.name", "T"]).unwrap();
        git.run(["config", "user.email", "t@e"]).unwrap();
        git.run(["commit", "-q", "--allow-empty", "-m", "a"])
            .unwrap();
        git.run(["branch", "old"]).unwrap();
        git.run(["commit", "-q", "--allow-empty", "-m", "b"])
            .unwrap();
        let repo = Repo::discover(dir.path()).unwrap();
        assert_eq!(
            condition(&repo, "old", "main", false),
            Condition::FastForward
        );
        assert_eq!(condition(&repo, "main", "old", false), Condition::UpToDate);
        assert_eq!(condition(&repo, "main", "old", true), Condition::Normal);
    }
}
