//! File operations: Add, Revert, Resolve, Remove, Rename, Ignore, Clean up,
//! Create repository and Inaccessible (spec-dialogs §2.4–2.9, §2.11).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use koopagit_core::fileops::{self, CleanOptions, CleanType, IgnoreFile, IgnoreType};
use koopagit_core::launcher::Invocation;
use koopagit_core::settings::workdir_key;
use koopagit_core::status::{Action, StatusEntry, StatusOptions};
use koopagit_core::{Repo, git_cli::Git};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::filelist::{self, Item};
use crate::filelist_actions::{FileListEnv, FileListHost, FileListMenu};
use crate::filelist_menu::PopMask;
use crate::progress::{PostAction, When};
use crate::refs::run_job;
use crate::{
    CleanDialog, Context, CreateRepoDialog, DialogResult, FileListBridge, FileListDialog,
    IgnoreDialog, InputDialog, msgbox, spawn_launcher,
};

/// Whether `entry` lies under one of the repository-relative `filters`.
fn under(path: &str, filters: &[String]) -> bool {
    filters.is_empty()
        || filters
            .iter()
            .any(|f| path == f || path.starts_with(&format!("{f}/")))
}

/// Which rows a file-list dialog shows and pre-checks.
struct ListSpec {
    title: &'static str,
    empty: &'static str,
    show_numstat: bool,
    extra_check: &'static str,
    /// Context-menu entries this dialog allows (spec-menus §1.2).
    menu: PopMask,
}

/// Shows a FileListDialog; returns the checked entries, or `None` on cancel.
fn pick_files(
    ctx: &Context,
    spec: &ListSpec,
    load: Rc<dyn Fn(bool) -> Vec<Item>>,
) -> DialogResult<Option<Vec<StatusEntry>>> {
    let dlg = FileListDialog::new()?;
    dlg.set_dialog_title(ctx.window_title(spec.title).into());
    dlg.set_empty_text(spec.empty.into());
    dlg.set_show_numstat(spec.show_numstat);
    dlg.set_extra_check_text(spec.extra_check.into());
    let items = Rc::new(RefCell::new(load(false)));
    let numstat: HashMap<String, (u32, u32)> = if spec.show_numstat {
        ctx.repo.numstat("HEAD")
    } else {
        HashMap::new()
    };
    let model = Rc::new(VecModel::<crate::FileRow>::default());
    dlg.set_rows(ModelRc::from(model.clone()));
    let list_menu = {
        let items = items.clone();
        let weak = dlg.as_weak();
        FileListMenu::new(
            FileListEnv::working_tree(
                0,
                ctx.repo.workdir(),
                spec.menu,
                &ctx.window_title(spec.title),
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
        // Enter diffs every selected row; Space checks them (spec-keyboard §1.1).
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
    let sync = {
        let items = items.clone();
        let model = model.clone();
        let weak = dlg.as_weak();
        let list_menu = list_menu.clone();
        Rc::new(move || {
            let it = items.borrow();
            model.set_vec(
                it.iter()
                    .map(|i| filelist::to_row(i, &numstat))
                    .collect::<Vec<_>>(),
            );
            list_menu.after_rebuild();
            if let Some(d) = weak.upgrade() {
                d.set_stats_text(filelist::stats_text(&it).into());
            }
        })
    };
    sync();
    {
        let items = items.clone();
        let weak = dlg.as_weak();
        dlg.on_row_toggled(move |i, c| {
            if let Some(it) = items.borrow_mut().get_mut(i as usize) {
                it.checked = c;
            }
            if let Some(d) = weak.upgrade() {
                d.set_stats_text(filelist::stats_text(&items.borrow()).into());
            }
        });
    }
    {
        let items = items.clone();
        let sync = sync.clone();
        dlg.on_select_all(move |c| {
            for it in items.borrow_mut().iter_mut() {
                it.checked = c;
            }
            sync();
        });
    }
    {
        let items = items.clone();
        let sync = sync.clone();
        let weak = dlg.as_weak();
        let load = load.clone();
        dlg.on_extra_toggled(move || {
            if let Some(d) = weak.upgrade() {
                *items.borrow_mut() = load(d.get_extra_checked());
                sync();
            }
        });
    }
    {
        // F5 and file-list menu actions: reload, keeping the check marks.
        let items = items.clone();
        let sync = sync.clone();
        let weak = dlg.as_weak();
        dlg.on_refresh(move || {
            let Some(d) = weak.upgrade() else { return };
            let old: HashMap<String, bool> = items
                .borrow()
                .iter()
                .map(|i| (i.entry.path.clone(), i.checked))
                .collect();
            let mut fresh = load(d.get_extra_checked());
            for it in &mut fresh {
                if let Some(c) = old.get(&it.entry.path) {
                    it.checked = *c;
                }
            }
            *items.borrow_mut() = fresh;
            sync();
        });
    }
    {
        let items = items.clone();
        let workdir = ctx.repo.workdir().to_owned();
        dlg.on_row_activated(move |i| {
            if let Some(it) = items.borrow().get(i as usize) {
                spawn_launcher(&Invocation::new("diff").with_paths([workdir.join(&it.entry.path)]));
            }
        });
    }
    let accepted = Rc::new(RefCell::new(false));
    {
        let accepted = accepted.clone();
        let weak = dlg.as_weak();
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
    crate::window::prepare(&dlg);
    dlg.run()?;
    if !*accepted.borrow() {
        return Ok(None);
    }
    Ok(Some(
        items
            .borrow()
            .iter()
            .filter(|i| i.checked)
            .map(|i| i.entry.clone())
            .collect(),
    ))
}

fn to_items(
    ctx: &Context,
    entries: Vec<StatusEntry>,
    check: impl Fn(&StatusEntry) -> bool,
) -> Vec<Item> {
    entries
        .into_iter()
        .map(|entry| Item {
            checked: check(&entry),
            is_dir: ctx.repo.workdir().join(&entry.path).is_dir(),
            entry,
        })
        .collect()
}

// ---------------------------------------------------------------- Add

/// `/command:add`: files are added directly; folders open the Add dialog.
pub fn add(ctx: &mut Context) -> DialogResult<i32> {
    let rel = ctx.rel_paths();
    let all_files = !ctx.paths.is_empty() && ctx.paths.iter().all(|p| p.is_file());
    let paths: Vec<String> = if all_files {
        rel
    } else {
        let repo_dir = ctx.repo.workdir().to_owned();
        let filters = rel.clone();
        let workdir = repo_dir.clone();
        let load: Rc<dyn Fn(bool) -> Vec<Item>> = Rc::new(move |ignored| {
            let Ok(repo) = Repo::discover(&workdir) else {
                return vec![];
            };
            let entries = repo
                .status(
                    StatusOptions {
                        unversioned: true,
                        ignored,
                    },
                    &filters,
                )
                .unwrap_or_default()
                .into_iter()
                .filter(|e| {
                    matches!(
                        e.action(),
                        Action::Unversioned | Action::Ignored | Action::Missing
                    ) && under(&e.path, &filters)
                })
                .collect::<Vec<_>>();
            entries
                .into_iter()
                .map(|entry| Item {
                    checked: entry.action() == Action::Unversioned,
                    is_dir: repo.workdir().join(&entry.path).is_dir(),
                    entry,
                })
                .collect()
        });
        let spec = ListSpec {
            title: "Add",
            empty: "There is nothing to add.\nAll the files and folders are either under version control,\nhave been ignored or the global ignore configuration setting.",
            show_numstat: false,
            extra_check: "Include ignored files",
            menu: PopMask::ADD_DLG,
        };
        let Some(entries) = pick_files(ctx, &spec, load)? else {
            return Ok(1);
        };
        entries.into_iter().map(|e| e.path).collect()
    };
    if paths.is_empty() {
        return Ok(1);
    }
    let mut post = vec![PostAction::on_success("Commit...", ctx.launch("commit"))];
    // "Add as Executable (+x)" / "Add as Symlink" (RC:3984, 3989).
    let files: Vec<String> = paths
        .iter()
        .filter(|p| ctx.repo.workdir().join(p).is_file())
        .cloned()
        .collect();
    if !files.is_empty() {
        post.push(PostAction::git(
            "Add as Executable (+x)",
            When::Success,
            vec![koopagit_core::misccmds::add_executable_args(&files)],
        ));
        let symlinks: Vec<Vec<String>> = files
            .iter()
            .take(50)
            .filter_map(|f| {
                let blob = ctx.repo.git().run(["hash-object", "-w", "--", f]).ok()?;
                Some(koopagit_core::misccmds::add_symlink_args(blob.trim(), f))
            })
            .collect();
        if symlinks.len() == files.len() {
            post.push(PostAction::git("Add as Symlink", When::Success, symlinks));
        }
    }
    let ok = run_job(ctx, "Add", vec![fileops::add(&paths)], post)?;
    Ok(if ok { 0 } else { 1 })
}

// ---------------------------------------------------------------- Revert

/// `/command:revert`.
pub fn revert(ctx: &mut Context) -> DialogResult<i32> {
    let filters = ctx.rel_paths();
    let direct: Vec<String> = ctx
        .paths
        .iter()
        .filter(|p| p.is_file() || !p.exists())
        .filter_map(|p| ctx.repo.relative_path(p))
        .collect();
    let entries: Vec<StatusEntry> = ctx
        .repo
        .status(StatusOptions::default(), &[])?
        .into_iter()
        .filter(|e| under(&e.path, &filters))
        .collect();
    let initial = to_items(ctx, entries, |e| {
        direct.contains(&e.path)
            || e.action() == Action::Added
            || direct.is_empty() && filters.is_empty()
    });
    let first_load = Rc::new(RefCell::new(Some(initial)));
    let load: Rc<dyn Fn(bool) -> Vec<Item>> = {
        let workdir = ctx.repo.workdir().to_owned();
        let filters = filters.clone();
        Rc::new(move |_| {
            if let Some(items) = first_load.borrow_mut().take() {
                return items;
            }
            // Refresh: re-read the status; new rows start unchecked.
            let Ok(repo) = Repo::discover(&workdir) else {
                return vec![];
            };
            repo.status(StatusOptions::default(), &[])
                .unwrap_or_default()
                .into_iter()
                .filter(|e| under(&e.path, &filters))
                .map(|entry| Item {
                    checked: false,
                    is_dir: repo.workdir().join(&entry.path).is_dir(),
                    entry,
                })
                .collect()
        })
    };
    let spec = ListSpec {
        title: "Revert",
        empty: "No files or folders were modified. There is nothing for KoopaGit to revert!",
        show_numstat: true,
        extra_check: "",
        menu: PopMask::REVERT_DLG,
    };
    let Some(entries) = pick_files(ctx, &spec, load)? else {
        return Ok(1);
    };
    if entries.is_empty() {
        return Ok(1);
    }
    // `RevertWithRecycleBin`: move the files to the trash before git restores
    // them, except added files (and renamed ones unless
    // `RevertRenamedFilesWithRecycleBin`).
    if ctx.settings.bool("RevertWithRecycleBin", true) {
        let renamed_too = ctx.settings.bool("RevertRenamedFilesWithRecycleBin", true);
        let to_trash: Vec<PathBuf> = entries
            .iter()
            .filter(|e| match e.action() {
                Action::Added | Action::Copied => false,
                Action::Renamed => renamed_too,
                _ => true,
            })
            .map(|e| ctx.repo.workdir().join(&e.path))
            .filter(|p| p.is_file())
            .collect();
        let failed = crate::trashcan::move_to_trash(&to_trash);
        if !failed.is_empty() {
            let list: Vec<String> = failed.iter().map(|p| p.display().to_string()).collect();
            msgbox::error(
                "KoopaGit",
                &format!(
                    "Could not move these files to the trash; they will be reverted anyway:\n{}",
                    list.join("\n")
                ),
            );
        }
    }
    let ok = run_job(ctx, "Revert", fileops::revert(&entries), vec![])?;
    Ok(if ok { 0 } else { 1 })
}

// ---------------------------------------------------------------- Resolve

/// `/command:resolve [/noquestion] [/silent]`.
pub fn resolve(ctx: &mut Context) -> DialogResult<i32> {
    let filters = ctx.rel_paths();
    let conflicted: Vec<StatusEntry> = ctx
        .repo
        .status(StatusOptions::default(), &[])?
        .into_iter()
        .filter(|e| e.action() == Action::Unmerged && under(&e.path, &filters))
        .collect();
    let paths: Vec<String> = if ctx.invocation.has("noquestion") {
        if filters.is_empty() {
            conflicted.into_iter().map(|e| e.path).collect()
        } else {
            filters
        }
    } else {
        let first_load = Rc::new(RefCell::new(Some(to_items(ctx, conflicted, |_| true))));
        let load: Rc<dyn Fn(bool) -> Vec<Item>> = {
            let workdir = ctx.repo.workdir().to_owned();
            let filters = filters.clone();
            Rc::new(move |_| {
                if let Some(items) = first_load.borrow_mut().take() {
                    return items;
                }
                let Ok(repo) = Repo::discover(&workdir) else {
                    return vec![];
                };
                repo.status(StatusOptions::default(), &[])
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|e| e.action() == Action::Unmerged && under(&e.path, &filters))
                    .map(|entry| Item {
                        checked: true,
                        is_dir: repo.workdir().join(&entry.path).is_dir(),
                        entry,
                    })
                    .collect()
            })
        };
        let spec = ListSpec {
            title: "Resolve",
            empty: "No conflicted files.",
            show_numstat: false,
            extra_check: "",
            menu: PopMask::RESOLVE_DLG,
        };
        let Some(entries) = pick_files(ctx, &spec, load)? else {
            return Ok(1);
        };
        entries.into_iter().map(|e| e.path).collect()
    };
    if paths.is_empty() {
        return Ok(1);
    }
    if ctx.invocation.has("silent") {
        let out = ctx.repo.git().output(fileops::resolve(&paths))?;
        remove_merge_temp_files(ctx.repo.workdir(), &paths);
        return Ok(if out.success() { 0 } else { 1 });
    }
    let ok = run_job(
        ctx,
        "Resolve",
        vec![fileops::resolve(&paths)],
        vec![PostAction::on_success("Commit...", ctx.launch("commit"))],
    )?;
    if ok {
        remove_merge_temp_files(ctx.repo.workdir(), &paths);
    }
    Ok(if ok { 0 } else { 1 })
}

/// Deletes `<file>.BASE/.LOCAL/.REMOTE/.BACKUP` leftovers of merge tools.
fn remove_merge_temp_files(workdir: &Path, paths: &[String]) {
    for p in paths {
        for suffix in ["BASE", "LOCAL", "REMOTE", "BACKUP", "orig"] {
            let _ = std::fs::remove_file(workdir.join(format!("{p}.{suffix}")));
        }
    }
}

// ---------------------------------------------------------------- Remove

/// `/command:remove [/keep]`: confirmation, then one `git rm` per path.
pub fn remove(ctx: &mut Context) -> DialogResult<i32> {
    let keep = ctx.invocation.has("keep");
    let rel: Vec<String> = ctx
        .paths
        .iter()
        .filter_map(|p| ctx.repo.relative_path(p))
        .collect();
    if rel.is_empty() {
        return Ok(1);
    }
    let suffix = if keep { " from the index" } else { "" };
    let question = if rel.len() == 1 {
        format!("Do you really want to remove \"{}\"{suffix}?", rel[0])
    } else {
        format!(
            "Do you really want to remove the {} selected files/directories{suffix}?",
            rel.len()
        )
    };
    // RemoveCommand.cpp:124 and :149 default to the second button.
    if msgbox::ask_default("KoopaGit", &question, &["Remove", "Abort"], 1) != Some(0) {
        return Ok(1);
    }
    let git = ctx.repo.git();
    let mut removed = 0;
    for p in &rel {
        let out = git.output(fileops::remove(p, keep))?;
        if out.success() {
            removed += 1;
        } else if msgbox::ask_default(
            "KoopaGit",
            &format!("{}{}", out.stdout, out.stderr),
            &["Ignore", "Abort"],
            1,
        ) != Some(0)
        {
            break;
        }
    }
    msgbox::info("KoopaGit", &format!("{removed} files removed."));
    Ok(0)
}

// ---------------------------------------------------------------- Rename

/// Shows an input box; returns the entered text or `None` on cancel.
pub fn input(
    title: &str,
    label: &str,
    value: &str,
    validate: impl Fn(&str) -> Option<String> + 'static,
) -> DialogResult<Option<String>> {
    let dlg = InputDialog::new()?;
    dlg.set_dialog_title(title.into());
    dlg.set_label(label.into());
    dlg.set_value(value.into());
    let result: Rc<RefCell<Option<String>>> = Rc::default();
    {
        let weak = dlg.as_weak();
        let result = result.clone();
        dlg.on_ok(move || {
            let Some(d) = weak.upgrade() else { return };
            let v = d.get_value().to_string();
            if let Some(err) = validate(&v) {
                d.set_notice_text(err.into());
                return;
            }
            *result.borrow_mut() = Some(v);
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
    Ok(result.take())
}

/// `/command:rename`: `git mv` within the same directory.
pub fn rename(ctx: &mut Context) -> DialogResult<i32> {
    let Some(path) = ctx.paths.first().cloned() else {
        return Ok(1);
    };
    let Some(old_rel) = ctx.repo.relative_path(&path) else {
        return Ok(1);
    };
    let old_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let dir = path.parent().map(Path::to_owned).unwrap_or_default();
    let check_dir = dir.clone();
    let original = old_name.clone();
    let Some(new_name) = input(
        &ctx.window_title("Rename"),
        &format!("Enter the new name for \"{old_name}\":"),
        &old_name,
        move |v| {
            let v = v.trim();
            if v.is_empty() || v.contains('/') || v.contains('\\') {
                Some("Invalid name.".to_owned())
            } else if v != original && check_dir.join(v).exists() {
                Some("A file or folder with that name already exists.".to_owned())
            } else {
                None
            }
        },
    )?
    else {
        return Ok(1);
    };
    let new_name = new_name.trim();
    if new_name == old_name {
        return Ok(0);
    }
    let new_rel = match old_rel.rsplit_once('/') {
        Some((d, _)) => format!("{d}/{new_name}"),
        None => new_name.to_owned(),
    };
    let out = ctx.repo.git().output(fileops::rename(&old_rel, &new_rel))?;
    if !out.success() {
        msgbox::error("KoopaGit", &format!("{}{}", out.stdout, out.stderr));
        return Ok(1);
    }
    Ok(0)
}

// ---------------------------------------------------------------- Ignore

/// `/command:ignore [/onlymask] [/delete]`.
pub fn ignore(ctx: &mut Context) -> DialogResult<i32> {
    let dlg = IgnoreDialog::new()?;
    dlg.set_dialog_title(ctx.window_title("Ignore").into());
    // Upstream quirk: the file radio is initialised from the type (0), so the
    // root .gitignore is the effective default.
    dlg.set_ignore_type(0);
    dlg.set_ignore_file(0);
    let accepted = Rc::new(RefCell::new(false));
    {
        let accepted = accepted.clone();
        let weak = dlg.as_weak();
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
    crate::window::prepare(&dlg);
    dlg.run()?;
    if !*accepted.borrow() {
        return Ok(1);
    }
    let kind = if dlg.get_ignore_type() == 1 {
        IgnoreType::Recursive
    } else {
        IgnoreType::InFolder
    };
    let file = match dlg.get_ignore_file() {
        1 => IgnoreFile::Containing,
        2 => IgnoreFile::InfoExclude,
        _ => IgnoreFile::Root,
    };
    let only_mask = ctx.invocation.has("onlymask");
    let mut targets = Vec::new();
    for p in &ctx.paths {
        let Some(rel) = ctx.repo.relative_path(p) else {
            continue;
        };
        if let Some(entry) = fileops::ignore_entry(&rel, p.is_dir(), kind, file, only_mask) {
            if let Err(e) = ctx.repo.apply_ignore(&entry) {
                msgbox::error("KoopaGit", &format!("{} Open Failure\n{e}", entry.file));
                return Ok(1);
            }
        }
        targets.push(rel);
    }
    if ctx.invocation.has("delete") && !targets.is_empty() {
        let keep = msgbox::ask("KoopaGit", "Keep file locally?", &["Yes", "No"]) == Some(0);
        let git = ctx.repo.git();
        let mut removed = 0;
        for t in &targets {
            let out = if keep {
                git.output(["rm", "--cache", "-r", "-f", "--", t])?
            } else {
                git.output(fileops::remove(t, false))?
            };
            if out.success() {
                removed += 1;
            } else if msgbox::ask(
                "KoopaGit",
                &format!("{}{}", out.stdout, out.stderr),
                &["OK", "Cancel"],
            ) != Some(0)
            {
                break;
            }
        }
        msgbox::info("KoopaGit", &format!("{removed} files removed."));
    }
    Ok(0)
}

// ---------------------------------------------------------------- Cleanup

/// `/command:cleanup` — the Clean dialog, then `git clean`.
pub fn cleanup(ctx: &mut Context) -> DialogResult<i32> {
    let key = workdir_key(ctx.repo.workdir());
    let type_key = format!(r"History\CleanType\{key}");
    let dir_key = format!(r"History\CleanDir\{key}");
    let dlg = CleanDialog::new()?;
    dlg.set_dialog_title(ctx.window_title("Clean").into());
    dlg.set_clean_type(ctx.settings.dword(&type_key, 0).min(2) as i32);
    dlg.set_directories(ctx.settings.bool(&dir_key, true));
    dlg.set_no_recycle(!ctx.settings.bool("RevertWithRecycleBin", true));
    // Post buttons after a dry run re-launch the clean without the dialog
    // (`/noui /cleantype:N /dirs:0|1 /nestedrepos:0|1 /submodules:0|1 /recycle:0|1`).
    let noui = ctx.invocation.has("noui");
    if noui {
        let inv = &ctx.invocation;
        dlg.set_clean_type(inv.get_long("cleantype").unwrap_or(0).clamp(0, 2) as i32);
        dlg.set_directories(inv.get_long("dirs") == Some(1));
        dlg.set_nested_repos(inv.get_long("nestedrepos") == Some(1));
        dlg.set_submodules(inv.get_long("submodules") == Some(1));
        dlg.set_no_recycle(inv.get_long("recycle") != Some(1));
        dlg.set_dry_run(false);
    }
    let accepted = Rc::new(RefCell::new(noui));
    {
        let accepted = accepted.clone();
        let weak = dlg.as_weak();
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
    if !noui {
        crate::window::prepare(&dlg);
        dlg.run()?;
    }
    if !*accepted.borrow() {
        return Ok(1);
    }
    let kind = CleanType::from_index(dlg.get_clean_type() as u32);
    if !noui {
        ctx.settings.set(&type_key, kind.index());
        ctx.settings.set_bool(&dir_key, dlg.get_directories());
        let _ = ctx.settings.save();
    }

    // The pathspec is the selected directory, or a selected file's directory.
    let path = ctx.paths.first().map(|p| {
        if p.is_dir() {
            p.clone()
        } else {
            p.parent().map(Path::to_owned).unwrap_or_default()
        }
    });
    let rel = path
        .and_then(|p| ctx.repo.relative_path(p))
        .filter(|r| !r.is_empty());
    let opts = CleanOptions {
        kind,
        directories: dlg.get_directories(),
        nested_repos: dlg.get_nested_repos(),
        dry_run: dlg.get_dry_run(),
        path: rel.clone(),
    };
    let mut cmds = vec![fileops::clean(&opts)];
    if dlg.get_submodules() {
        // Same clean in every initialised submodule (recursively).
        let mut sub = vec![
            "submodule".to_owned(),
            "foreach".to_owned(),
            "--recursive".to_owned(),
        ];
        sub.push(format!(
            "git {}",
            fileops::clean(&CleanOptions {
                path: None,
                ..opts.clone()
            })
            .join(" ")
        ));
        cmds.push(sub);
    }
    let use_recycle = !dlg.get_no_recycle();
    if use_recycle && !opts.dry_run {
        return clean_to_trash(ctx, &opts, dlg.get_submodules());
    }
    let mut post = vec![PostAction::on_failure("Retry", ctx.launch("cleanup"))];
    if opts.dry_run {
        // "Clean to recycle bin" / "Clean permanently" (RC:5074-5075); the
        // preferred one first, depending on "Do not use recycle bin".
        let relaunch = |recycle: bool| {
            let mut inv = ctx
                .launch("cleanup")
                .with_switch("noui")
                .with("cleantype", kind.index().to_string())
                .with("dirs", u8::from(opts.directories).to_string())
                .with("nestedrepos", u8::from(opts.nested_repos).to_string())
                .with("submodules", u8::from(dlg.get_submodules()).to_string())
                .with("recycle", u8::from(recycle).to_string());
            if let Some(p) = ctx.paths.first() {
                inv = inv.with_paths([p.clone()]);
            }
            inv
        };
        let to_trash = PostAction::on_success("Clean to recycle bin", relaunch(true));
        let permanent = PostAction::on_success("Clean permanently", relaunch(false));
        if use_recycle {
            post.extend([to_trash, permanent]);
        } else {
            post.extend([permanent, to_trash]);
        }
    }
    let ok = run_job(ctx, "Clean", cmds, post)?;
    Ok(if ok { 0 } else { 1 })
}

/// Recycle-bin clean (`CleanupCommand.cpp:68-100, 229-275`): ask git what it
/// would remove, then move those paths to the trash.
fn clean_to_trash(ctx: &Context, opts: &CleanOptions, submodules: bool) -> DialogResult<i32> {
    let dry = CleanOptions {
        dry_run: true,
        ..opts.clone()
    };
    let git = ctx.repo.git();
    let mut args = vec!["-c".to_owned(), "core.quotepath=false".to_owned()];
    args.extend(fileops::clean(&dry));
    let mut out = git.run(&args)?;
    if submodules {
        let inner = format!(
            "git -c core.quotepath=false {}",
            fileops::clean(&CleanOptions {
                path: None,
                ..dry.clone()
            })
            .join(" ")
        );
        out.push_str(&git.run(["submodule", "foreach", "--recursive", &inner])?);
    }
    let paths: Vec<PathBuf> = koopagit_core::misccmds::parse_clean_dry_run(&out)
        .into_iter()
        .map(|p| ctx.repo.workdir().join(p))
        .collect();
    if paths.is_empty() {
        msgbox::info("Clean", "Nothing to clean.");
        return Ok(0);
    }
    let failed = crate::trashcan::move_to_trash(&paths);
    if failed.is_empty() {
        msgbox::info(
            "Clean",
            &format!("{} item(s) moved to the recycle bin.", paths.len()),
        );
        Ok(0)
    } else {
        let list: Vec<String> = failed.iter().map(|p| p.display().to_string()).collect();
        msgbox::error(
            "Clean",
            &format!("Could not move to the recycle bin:\n{}", list.join("\n")),
        );
        Ok(1)
    }
}

// ---------------------------------------------------------------- Create repository

/// Folders that should not become repository roots (platform equivalents of
/// TortoiseGit's special-folder list).
fn is_special_folder(dir: &Path) -> bool {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    dir.parent().is_none()
        || home.as_deref() == Some(dir)
        || home
            .as_ref()
            .is_some_and(|h| ["Desktop", "Documents"].iter().any(|d| h.join(d) == dir))
}

/// `/command:repocreate` — the Git Init dialog.
pub fn repocreate(inv: &Invocation) -> DialogResult<i32> {
    let folder = inv
        .paths()?
        .into_iter()
        .next()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default();
    let folder = if folder.is_absolute() {
        folder
    } else {
        std::env::current_dir()?.join(folder)
    };
    let title = crate::window_title(&folder, "Git Init");
    if is_special_folder(&folder)
        && msgbox::ask(
            &title,
            &format!(
                "The target folder \n{}\nis a special folder and is not supposed to be a repository root!\nAre you sure you want to initialize a git repository inside that folder?",
                folder.display()
            ),
            &["Abort", "Proceed"],
        ) != Some(1)
    {
        return Ok(1);
    }
    let dlg = CreateRepoDialog::new()?;
    dlg.set_dialog_title(title.clone().into());
    dlg.set_bare(
        folder
            .file_name()
            .is_some_and(|n| n.to_string_lossy().to_lowercase().ends_with(".git")),
    );
    let accepted = Rc::new(RefCell::new(false));
    let not_empty = folder
        .read_dir()
        .map(|mut d| d.next().is_some())
        .unwrap_or(false);
    let display = folder.display().to_string();
    {
        let accepted = accepted.clone();
        let weak = dlg.as_weak();
        dlg.on_ok(move || {
            let Some(d) = weak.upgrade() else { return };
            if d.get_bare() && not_empty {
                d.set_notice_text(
                    format!("The target folder \n{display}\nis not empty!\nAre you sure you want to initialize a bare repository there?").into(),
                );
                d.set_notice_buttons(crate::refs::string_model(&["Abort".into(), "Proceed".into()]));
                return;
            }
            *accepted.borrow_mut() = true;
            let _ = d.hide();
        });
    }
    {
        let accepted = accepted.clone();
        let weak = dlg.as_weak();
        dlg.on_notice_clicked(move |i| {
            let Some(d) = weak.upgrade() else { return };
            d.set_notice_text("".into());
            if i == 1 {
                *accepted.borrow_mut() = true;
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
    crate::window::prepare(&dlg);
    dlg.run()?;
    if !*accepted.borrow() {
        return Ok(1);
    }
    let parent = folder
        .ancestors()
        .find(|a| a.is_dir())
        .map(Path::to_owned)
        .unwrap_or_else(std::env::temp_dir);
    let out = Git::new(parent).output(fileops::init(&folder.to_string_lossy(), dlg.get_bare()))?;
    if !out.success() {
        msgbox::error(
            &title,
            &format!("Could not initialize a new repository.\n\n{}", out.stderr),
        );
        return Ok(1);
    }
    msgbox::info(
        &title,
        &format!("Initialized empty Git repository in {}.", folder.display()),
    );
    Ok(0)
}

// ---------------------------------------------------------------- Inaccessible

/// `/command:inaccessible`: explains why a folder with `.git` cannot be opened.
pub fn inaccessible(inv: &Invocation) -> DialogResult<i32> {
    let folder = inv.paths()?.into_iter().next().unwrap_or_default();
    let detail = match Repo::discover(&folder) {
        Ok(_) => "The repository could be opened now.".to_owned(),
        Err(e) => e.to_string(),
    };
    let detail = if folder.as_os_str().is_empty() {
        detail
    } else {
        let probe = Git::new(&folder).output(["rev-parse", "--git-dir"]);
        match probe {
            Ok(o) if !o.success() => o.stderr,
            _ => detail,
        }
    };
    msgbox::info(
        &crate::window_title(&folder, "Inaccessible"),
        &format!(
            "The repository in\n{}\nis not accessible.\n\nThis usually means the .git folder is damaged, owned by another user (see git's safe.directory), or uses an unsupported format.\n\nDetails:\n{}",
            folder.display(),
            detail.trim()
        ),
    );
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_filters_and_special_folders() {
        assert!(under("a/b.txt", &["a".into()]));
        assert!(!under("ab/c", &["a".into()]));
        assert!(under("x", &[]));
        assert!(is_special_folder(Path::new("/")));
    }
}
