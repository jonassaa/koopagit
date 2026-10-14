//! `/command:repostatus` — Check for modifications (`CChangedDlg`, "Working Tree").

use std::cell::RefCell;
use std::rc::Rc;

use koopagit_core::Repo;
use koopagit_core::launcher::Invocation;
use koopagit_core::settings::workdir_key;
use koopagit_core::status::{Action, StatusOptions};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::filelist::{self, Item};
use crate::filelist_actions::{FileListEnv, FileListHost, FileListMenu};
use crate::filelist_menu::PopMask;
use crate::{Context, DialogResult, FileListBridge, WorkingTreeDialog, spawn_launcher};

/// Summary line like the dialog's info labels.
fn summary(items: &[Item]) -> String {
    let count = |f: &dyn Fn(Action) -> bool| items.iter().filter(|i| f(i.entry.action())).count();
    format!(
        "{} modified, {} added, {} deleted, {} conflicted, {} non-versioned, {} ignored",
        count(&|a| matches!(a, Action::Modified | Action::Renamed)),
        count(&|a| matches!(a, Action::Added | Action::Copied)),
        count(&|a| matches!(a, Action::Deleted | Action::Missing)),
        count(&|a| a == Action::Unmerged),
        count(&|a| a == Action::Unversioned),
        count(&|a| a == Action::Ignored),
    )
}

pub fn run(ctx: &mut Context) -> DialogResult<i32> {
    let repo = &ctx.repo;
    let whole_key = format!(
        r"TortoiseProc\ShowWholeProject\{}",
        workdir_key(repo.workdir())
    );
    let dlg = WorkingTreeDialog::new()?;
    dlg.set_dialog_title(ctx.window_title("Working Tree").into());
    let branch = repo
        .current_branch()?
        .unwrap_or_else(|| "(no branch)".to_owned());
    dlg.set_branch(branch.into());
    dlg.set_show_unversioned(ctx.settings.bool("AddBeforeCommit", true));
    dlg.set_show_staged(
        ctx.settings
            .bool(r"TortoiseProc\ChangedFilesIncludeStaged", true),
    );
    dlg.set_whole_project(ctx.settings.bool(&whole_key, false));
    // Unversioned/ignored filters are only enabled for a single directory target.
    let single_dir = ctx.paths.len() <= 1 && ctx.paths.first().is_none_or(|p| p.is_dir());
    dlg.set_filters_enabled(single_dir);

    let items: Rc<RefCell<Vec<Item>>> = Rc::default();
    let model = Rc::new(VecModel::<crate::FileRow>::default());
    dlg.set_rows(ModelRc::from(model.clone()));
    let workdir = repo.workdir().to_owned();
    let filters = ctx.rel_paths();

    // File-list context menu and multi-selection (ChangedDlg mask).
    let list_menu = {
        let items = items.clone();
        let weak = dlg.as_weak();
        FileListMenu::new(
            FileListEnv::working_tree(
                0,
                &workdir,
                PopMask::CHANGED_DLG,
                &ctx.window_title("Working Tree"),
            ),
            FileListHost {
                items: Box::new(move || items.borrow().clone()),
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
    {
        // Enter diffs every selected row (spec-keyboard §1.1).
        let weak = dlg.as_weak();
        list_menu.set_on_activate(move |row| {
            if let Some(d) = weak.upgrade() {
                d.invoke_row_activated(row as i32);
            }
        });
    }

    let refresh = {
        let weak = dlg.as_weak();
        let items = items.clone();
        let model = model.clone();
        let list_menu = list_menu.clone();
        let workdir = workdir.clone();
        let filters = filters.clone();
        Rc::new(move || {
            let Some(d) = weak.upgrade() else { return };
            let Ok(repo) = Repo::discover(&workdir) else {
                return;
            };
            let whole = d.get_whole_project();
            let f: Vec<String> = if whole { vec![] } else { filters.clone() };
            let entries = repo
                .status(
                    StatusOptions {
                        unversioned: d.get_show_unversioned(),
                        ignored: d.get_show_ignored(),
                    },
                    &[],
                )
                .unwrap_or_default();
            let staged = d.get_show_staged();
            let list: Vec<Item> = entries
                .into_iter()
                .filter(|e| {
                    f.is_empty()
                        || f.iter()
                            .any(|p| e.path == *p || e.path.starts_with(&format!("{p}/")))
                        || (staged && e.staged.is_some())
                })
                .map(|entry| Item {
                    checked: false,
                    is_dir: repo.workdir().join(&entry.path).is_dir(),
                    entry,
                })
                .collect();
            let numstat = repo.numstat("HEAD");
            model.set_vec(
                list.iter()
                    .map(|i| filelist::to_row(i, &numstat))
                    .collect::<Vec<_>>(),
            );
            d.set_summary_text(summary(&list).into());
            *items.borrow_mut() = list;
            list_menu.after_rebuild();
        })
    };
    refresh();
    {
        let f = refresh.clone();
        dlg.on_refresh(move || f());
    }
    {
        let items = items.clone();
        let workdir = workdir.clone();
        dlg.on_row_activated(move |i| {
            if let Some(it) = items.borrow().get(i as usize) {
                spawn_launcher(&Invocation::new("diff").with_paths([workdir.join(&it.entry.path)]));
            }
        });
    }
    let target: Vec<std::path::PathBuf> = if ctx.paths.is_empty() {
        vec![workdir.clone()]
    } else {
        ctx.paths.clone()
    };
    {
        let workdir = workdir.clone();
        dlg.on_branch_clicked(move || {
            spawn_launcher(&Invocation::new("log").with_paths([workdir.clone()]));
        });
    }
    {
        let target = target.clone();
        let refresh = refresh.clone();
        dlg.on_commit(move || {
            spawn_launcher(&Invocation::new("commit").with_paths(target.clone()));
            refresh();
        });
    }
    {
        let workdir = workdir.clone();
        dlg.on_stash(move |i| {
            let inv = match i {
                0 => Invocation::new("stashsave"),
                1 => Invocation::new("stashpop"),
                2 => Invocation::new("stashapply"),
                _ => Invocation::new("reflog").with("ref", "refs/stash"),
            };
            spawn_launcher(&inv.with_paths([workdir.clone()]));
        });
    }
    {
        let target = target.clone();
        dlg.on_save_diff(move || {
            spawn_launcher(
                &Invocation::new("showcompare")
                    .with_switch("unified")
                    .with_paths(target.clone())
                    .with("revision1", "HEAD")
                    .with("revision2", "0000000000000000000000000000000000000000"),
            );
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
    crate::window::prepare(&dlg);
    dlg.run()?;
    ctx.settings
        .set_bool("AddBeforeCommit", dlg.get_show_unversioned());
    ctx.settings.set_bool(
        r"TortoiseProc\ChangedFilesIncludeStaged",
        dlg.get_show_staged(),
    );
    ctx.settings.set_bool(&whole_key, dlg.get_whole_project());
    let _ = ctx.settings.save();
    Ok(0)
}
