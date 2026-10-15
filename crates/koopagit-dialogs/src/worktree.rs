//! `/command:newworktree`, `/command:dropnewworktree` (`CCreateWorktreeDlg`) and
//! `/command:worktreelist` (`CWorktreeListDlg`).

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use koopagit_core::Repo;
use koopagit_core::extras::{self, Worktree, WorktreeAdd};
use koopagit_core::launcher::{CloseOnEnd, Invocation};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::menus::worktree::{WorktreeAction, WorktreeRow, menu};
use crate::menus::{ActionId, MenuCache};
use crate::progress::{PostAction, ProgressJob, When};
use crate::refs::Versions;
use crate::remote_util::{strings, valid_branch_name};
use crate::selection::{Mods, Selection};
use crate::{Context, DialogResult, NewWorktreeDialog, WorktreeListDialog, WorktreeRowData};

// ---------------------------------------------------------------- New Worktree

/// `/command:newworktree`; `target` is the directory from `dropnewworktree`.
pub fn new_worktree(ctx: &mut Context, target: Option<String>) -> DialogResult<i32> {
    let versions = Rc::new(Versions::load(ctx, false));
    let dlg = NewWorktreeDialog::new()?;
    dlg.set_dialog_title(ctx.window_title("New Worktree").into());
    let branch = ctx
        .repo
        .current_branch()
        .ok()
        .flatten()
        .unwrap_or_else(|| "(no branch)".to_owned());
    dlg.set_head_label(format!("HEAD ({branch})").into());
    dlg.set_branches(strings(&versions.branches));
    dlg.set_tags(strings(&versions.tags));
    let repo_dir = ctx.repo.workdir().to_string_lossy().into_owned();
    let dir = target.unwrap_or_else(|| extras::default_worktree_dir(&repo_dir));
    dlg.set_dir(dir.into());

    let version_of = {
        let versions = versions.clone();
        move |d: &NewWorktreeDialog| {
            versions.selected(
                d.get_mode(),
                d.get_branch_index(),
                d.get_tag_index(),
                &d.get_commit_text(),
            )
        }
    };
    let version_of = Rc::new(version_of);

    // OnBnClickedCheckBranch: without a new branch, Commit/Tag/remote branch
    // can only be checked out detached.
    let branch_toggled = {
        let version_of = version_of.clone();
        Rc::new(move |d: &NewWorktreeDialog| {
            let mode = d.get_mode();
            let v = version_of(d);
            let detach = !d.get_new_branch()
                && (mode == 3 || mode == 2 || (mode == 1 && v.starts_with("remotes/")));
            d.set_detach(detach);
            d.set_detach_enabled(!detach);
        })
    };
    let version_changed = {
        let version_of = version_of.clone();
        let branch_toggled = branch_toggled.clone();
        Rc::new(move |d: &NewWorktreeDialog| {
            let (name, branch) =
                extras::worktree_branch_proposal(d.get_mode(), &version_of(d), &d.get_dir());
            d.set_branch_name(name.into());
            d.set_new_branch(branch);
            branch_toggled(d);
        })
    };
    version_changed(&dlg);
    {
        let weak = dlg.as_weak();
        let f = version_changed.clone();
        dlg.on_version_changed(move || {
            if let Some(d) = weak.upgrade() {
                f(&d);
            }
        });
    }
    {
        let weak = dlg.as_weak();
        let f = branch_toggled.clone();
        dlg.on_branch_toggled(move || {
            if let Some(d) = weak.upgrade() {
                f(&d);
            }
        });
    }
    {
        let weak = dlg.as_weak();
        dlg.on_detach_toggled(move || {
            if let Some(d) = weak.upgrade()
                && d.get_detach()
            {
                d.set_new_branch(false);
            }
        });
    }
    {
        let weak = dlg.as_weak();
        dlg.on_browse(move || {
            let Some(d) = weak.upgrade() else { return };
            let cur = PathBuf::from(d.get_dir().as_str());
            let start = cur.parent().map(|p| p.to_owned());
            if let Some(p) = crate::pick::folder("Directory", start.as_deref()) {
                d.set_dir(p.to_string_lossy().as_ref().into());
            }
        });
    }
    let decided: Rc<RefCell<Option<WorktreeAdd>>> = Rc::default();
    {
        let weak = dlg.as_weak();
        let decided = decided.clone();
        let version_of = version_of.clone();
        let repo_dir = ctx.repo.workdir().to_owned();
        dlg.on_ok(move || {
            let Some(d) = weak.upgrade() else { return };
            let dir = d.get_dir().trim().to_owned();
            if dir.is_empty() {
                d.set_notice_text("This field is required and must not be empty.".into());
                return;
            }
            let new_branch = d.get_new_branch() && !d.get_detach();
            let name = d.get_branch_name().trim().to_owned();
            if new_branch {
                let ok = Repo::discover(&repo_dir).is_ok_and(|r| valid_branch_name(&r, &name));
                if !ok {
                    d.set_notice_text("Branch/Tag name must not be empty or is invalid.".into());
                    return;
                }
            }
            *decided.borrow_mut() = Some(WorktreeAdd {
                path: dir,
                version: version_of(&d),
                checkout: d.get_checkout(),
                force: d.get_force(),
                detach: d.get_detach(),
                new_branch: new_branch.then_some(name),
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

    let Some(add) = decided.borrow_mut().take() else {
        return Ok(1);
    };
    let mut post = Vec::new();
    if ctx.repo.workdir().join(".gitmodules").exists() {
        post.push(PostAction::on_success(
            "Update Submodules",
            Invocation::new("subupdate").with("bkpath", add.path.clone()),
        ));
    }
    let ok = crate::progress::run(ProgressJob {
        title: ctx.window_title("New Worktree"),
        git: ctx.repo.git(),
        commands: vec![extras::worktree_add_args(&add)],
        post_actions: post,
        close_on_end: ctx.invocation.close_on_end().unwrap_or_default(),
    })?;
    Ok(if ok { 0 } else { 1 })
}

/// `/command:dropnewworktree /droptarget:<dir>`.
pub fn drop_new_worktree(ctx: &mut Context) -> DialogResult<i32> {
    let target = ctx
        .invocation
        .get("droptarget")
        .unwrap_or_default()
        .to_owned();
    let repo_dir = ctx.repo.workdir().to_string_lossy().into_owned();
    let dir = extras::drop_worktree_dir(&target, &repo_dir);
    new_worktree(ctx, Some(dir))
}

// ---------------------------------------------------------------- Worktree List

/// Pending question in the list dialog's notice bar.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ask {
    None,
    Remove { force: bool },
}

struct ListState {
    worktrees: Vec<Worktree>,
    selection: Selection,
}

fn row_data(w: &Worktree, selected: bool) -> WorktreeRowData {
    WorktreeRowData {
        path: w.path.clone().into(),
        hash: w.head.clone().into(),
        branch: w.branch.clone().into(),
        // The main worktree has no lock state (`m_isBaseRepo`).
        locked: if w.is_main {
            SharedString::new()
        } else if w.locked {
            "Locked".into()
        } else {
            "Unlocked".into()
        },
        reason: if w.locked {
            w.lock_reason.clone().into()
        } else {
            SharedString::new()
        },
        selected,
    }
}

pub fn list(ctx: &mut Context) -> DialogResult<i32> {
    let dlg = WorktreeListDialog::new()?;
    dlg.set_dialog_title(ctx.window_title("Worktree List").into());
    let workdir = ctx.repo.workdir().to_owned();
    let state = Rc::new(RefCell::new(ListState {
        worktrees: Vec::new(),
        selection: Selection::default(),
    }));
    let cache = Rc::new(MenuCache::default());
    let ask = Rc::new(Cell::new(Ask::None));

    let model = Rc::new(VecModel::<WorktreeRowData>::default());
    dlg.set_rows(ModelRc::from(model.clone()));
    // Full rebuild after the list changed.
    let render = {
        let state = state.clone();
        let model = model.clone();
        let cache = cache.clone();
        Rc::new(move || {
            let st = state.borrow();
            let rows: Vec<WorktreeRowData> = st
                .worktrees
                .iter()
                .enumerate()
                .map(|(i, w)| row_data(w, st.selection.contains(i)))
                .collect();
            model.set_vec(rows);
            cache.invalidate();
        })
    };
    // Selection-only update: changes row data in place so row items (and an
    // open context menu) are not recreated.
    let render_selection = {
        let state = state.clone();
        let model = model.clone();
        let cache = cache.clone();
        Rc::new(move || {
            let st = state.borrow();
            for i in 0..model.row_count() {
                if let Some(mut r) = model.row_data(i) {
                    let sel = st.selection.contains(i);
                    if r.selected != sel {
                        r.selected = sel;
                        model.set_row_data(i, r);
                    }
                }
            }
            cache.invalidate();
        })
    };
    let reload = {
        let state = state.clone();
        let render = render.clone();
        let workdir = workdir.clone();
        let weak = dlg.as_weak();
        Rc::new(move || {
            match Repo::discover(&workdir).and_then(|r| r.worktrees()) {
                Ok(list) => {
                    let mut st = state.borrow_mut();
                    st.selection.clamp(list.len());
                    st.worktrees = list;
                }
                Err(e) => {
                    if let Some(d) = weak.upgrade() {
                        notice(&d, &e.to_string(), &[], true);
                    }
                }
            }
            render();
        })
    };
    reload();

    {
        let state = state.clone();
        let render_selection = render_selection.clone();
        dlg.on_row_clicked(move |i, toggle, range| {
            state
                .borrow_mut()
                .selection
                .click(i as usize, Mods { toggle, range });
            render_selection();
        });
    }
    {
        let state = state.clone();
        dlg.on_row_activated(move |i| {
            if let Some(w) = state.borrow().worktrees.get(i as usize) {
                let _ = crate::osopen::explore_to(&PathBuf::from(&w.path));
            }
        });
    }
    {
        let state = state.clone();
        let cache = cache.clone();
        let render_selection = render_selection.clone();
        dlg.on_menu_for(move |row| {
            // A right-click on an unselected row selects only that row. The
            // highlight is updated after the menu has been built.
            let changed = {
                let mut st = state.borrow_mut();
                let before = st.selection.clone();
                st.selection.right_click(row as usize);
                before != st.selection
            };
            if changed {
                cache.invalidate();
                let render_selection = render_selection.clone();
                slint::Timer::single_shot(std::time::Duration::ZERO, move || render_selection());
            }
            cache.get(row, || {
                let st = state.borrow();
                let rows: Vec<WorktreeRow> = st
                    .selection
                    .rows()
                    .filter_map(|i| st.worktrees.get(i))
                    .map(|w| WorktreeRow {
                        is_main: w.is_main,
                        locked: w.locked,
                    })
                    .collect();
                menu(&rows)
            })
        });
    }
    {
        let state = state.clone();
        let weak = dlg.as_weak();
        let ask = ask.clone();
        let reload = reload.clone();
        let workdir = workdir.clone();
        dlg.on_menu_action(move |id| {
            let Some(d) = weak.upgrade() else { return };
            let Some(action) = WorktreeAction::from_id(id) else {
                return;
            };
            let selected: Vec<Worktree> = {
                let st = state.borrow();
                st.selection
                    .rows()
                    .filter_map(|i| st.worktrees.get(i).cloned())
                    .collect()
            };
            match action {
                WorktreeAction::Open => {
                    if let [w] = selected.as_slice() {
                        let _ = crate::osopen::explore_to(&PathBuf::from(&w.path));
                    }
                }
                WorktreeAction::Lock | WorktreeAction::Unlock => {
                    let lock = action == WorktreeAction::Lock;
                    let git = koopagit_core::git_cli::Git::new(&workdir);
                    let mut done = 0;
                    let mut error = None;
                    for w in selected.iter().filter(|w| !w.is_main) {
                        let verb = if lock { "lock" } else { "unlock" };
                        match git.output(["worktree", verb, "--", &w.path]) {
                            Ok(o) if o.success() => done += 1,
                            Ok(o) => {
                                error = Some(o.stderr);
                                break;
                            }
                            Err(e) => {
                                error = Some(e.to_string());
                                break;
                            }
                        }
                    }
                    reload();
                    match error {
                        Some(e) => notice(&d, &e, &[], true),
                        None if lock => {
                            notice(&d, &format!("Locked {done} worktree(s)."), &[], false)
                        }
                        None => notice(
                            &d,
                            &format!("Successfully unlocked {done} worktree(s)."),
                            &[],
                            false,
                        ),
                    }
                }
                WorktreeAction::Remove | WorktreeAction::RemoveWithForce => {
                    ask.set(Ask::Remove {
                        force: action == WorktreeAction::RemoveWithForce,
                    });
                    notice(
                        &d,
                        "Do you really want to delete the selected worktree(s)?",
                        &["Yes", "No"],
                        false,
                    );
                }
            }
        });
    }
    {
        let state = state.clone();
        let weak = dlg.as_weak();
        let ask = ask.clone();
        let reload = reload.clone();
        let workdir = workdir.clone();
        dlg.on_notice_clicked(move |i| {
            let Some(d) = weak.upgrade() else { return };
            notice(&d, "", &[], false);
            let Ask::Remove { force } = ask.replace(Ask::None) else {
                return;
            };
            if i != 0 {
                return;
            }
            let paths: Vec<String> = {
                let st = state.borrow();
                st.selection
                    .rows()
                    .filter_map(|r| st.worktrees.get(r))
                    .filter(|w| !w.is_main)
                    .map(|w| w.path.clone())
                    .collect()
            };
            if paths.is_empty() {
                return;
            }
            let commands = paths
                .iter()
                .map(|p| extras::worktree_remove_args(p, force))
                .collect();
            let post = if force {
                Vec::new()
            } else {
                vec![PostAction::git(
                    "Force remove",
                    When::Failure,
                    paths
                        .iter()
                        .map(|p| extras::worktree_remove_args(p, true))
                        .collect(),
                )]
            };
            let reload = reload.clone();
            let job = ProgressJob {
                title: "Remove worktree".to_owned(),
                git: koopagit_core::git_cli::Git::new(&workdir),
                commands,
                post_actions: post,
                close_on_end: CloseOnEnd::Manual,
            };
            if let Err(e) = crate::progress::start(job, move |_| reload()) {
                notice(&d, &e.to_string(), &[], true);
            }
        });
    }
    {
        let reload = reload.clone();
        let weak = dlg.as_weak();
        let workdir = workdir.clone();
        dlg.on_prune(move || {
            let reload = reload.clone();
            let job = ProgressJob {
                title: "Prune worktrees".to_owned(),
                git: koopagit_core::git_cli::Git::new(&workdir),
                commands: vec![vec!["worktree".to_owned(), "prune".to_owned()]],
                post_actions: Vec::new(),
                close_on_end: CloseOnEnd::Manual,
            };
            if let Err(e) = crate::progress::start(job, move |_| reload())
                && let Some(d) = weak.upgrade()
            {
                notice(&d, &e.to_string(), &[], true);
            }
        });
    }
    {
        // "Add" opens New Worktree in its own process and refreshes afterwards.
        let weak = dlg.as_weak();
        let workdir = workdir.clone();
        dlg.on_add(move || {
            let exe = std::env::current_exe().unwrap_or_else(|_| "koopagit-proc".into());
            let inv = Invocation::new("newworktree").with_paths([workdir.clone()]);
            let Ok(mut child) = std::process::Command::new(exe).args(inv.to_args()).spawn() else {
                return;
            };
            let weak = weak.clone();
            std::thread::spawn(move || {
                let _ = child.wait();
                let _ = weak.upgrade_in_event_loop(|d| d.invoke_refresh());
            });
        });
    }
    {
        let reload = reload.clone();
        dlg.on_refresh(move || reload());
    }
    {
        let weak = dlg.as_weak();
        dlg.on_ok(move || {
            if let Some(d) = weak.upgrade() {
                let _ = d.hide();
            }
        });
    }
    crate::window::prepare(&dlg);
    dlg.run()?;
    Ok(0)
}

fn notice(d: &WorktreeListDialog, text: &str, buttons: &[&str], error: bool) {
    d.set_notice_text(text.into());
    let labels: Vec<SharedString> = buttons.iter().map(|b| (*b).into()).collect();
    d.set_notice_buttons(ModelRc::new(VecModel::from(labels)));
    d.set_notice_error(error);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_texts() {
        let main = Worktree {
            path: "/r".into(),
            is_main: true,
            ..Default::default()
        };
        assert_eq!(row_data(&main, false).locked.as_str(), "");
        let w = Worktree {
            path: "/w".into(),
            locked: true,
            lock_reason: "usb".into(),
            ..Default::default()
        };
        let r = row_data(&w, true);
        assert_eq!(
            (r.locked.as_str(), r.reason.as_str(), r.selected),
            ("Locked", "usb", true)
        );
    }
}
