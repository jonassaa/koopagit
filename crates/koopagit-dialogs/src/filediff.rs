//! `CFileDiffDlg` ("Changed Files"): the files that differ between two
//! revisions (either may be the working tree). Opened by `showcompare` without
//! `/unified`, `prevdiff` on a folder and the log's "Compare revisions".

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use koopagit_core::Repo;
use koopagit_core::extras::{ChangedFile, DiffOptions, is_working_tree};
use koopagit_core::launcher::Invocation;
use koopagit_core::settings::Settings;
use koopagit_core::status::Action;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::diffcmd::{rev_title, start_diff, wt_title};
use crate::menus::filediff::{self as fdm, FileDiffAction, FileDiffMenuInput};
use crate::menus::{ActionId, MenuCache};
use crate::selection::{Mods, Selection};
use crate::{
    Context, DialogResult, FileDiffDialog, FileListBridge, FileRow, MenuBlock, msgbox,
    spawn_launcher,
};

/// Text shown for the working tree in the version boxes.
const WORKING_TREE: &str = "Working Tree";

/// Display name of a row: `new (from old)` for renames and copies.
pub fn row_name(f: &ChangedFile) -> String {
    match &f.old_path {
        Some(old) if matches!(f.action, Action::Renamed | Action::Copied) => {
            format!("{} (from {old})", f.path)
        }
        _ => f.path.clone(),
    }
}

/// `CLogDlgFileFilter` for plain text: case-insensitive substring of the path.
pub fn matches_filter(f: &ChangedFile, filter: &str) -> bool {
    let filter = filter.trim();
    filter.is_empty()
        || f.path.to_lowercase().contains(&filter.to_lowercase())
        || f.old_path
            .as_ref()
            .is_some_and(|o| o.to_lowercase().contains(&filter.to_lowercase()))
}

fn to_row(f: &ChangedFile) -> FileRow {
    let kind = match f.action {
        Action::Added | Action::Copied => 1,
        Action::Deleted | Action::Missing => 2,
        Action::Modified => 3,
        Action::Unmerged => 4,
        Action::Renamed => 5,
        Action::Unversioned | Action::Ignored => 6,
    };
    let stat = |v: Option<u32>| v.map(|n| n.to_string()).unwrap_or_else(|| "-".into());
    FileRow {
        path: row_name(f).into(),
        extension: crate::filelist::extension(&f.path).into(),
        status: f.action.name().into(),
        added: stat(f.added).into(),
        removed: stat(f.removed).into(),
        kind,
        ..Default::default()
    }
}

/// Normalises a launcher revision: all zeros or empty means the working tree.
fn norm(rev: &str) -> String {
    if is_working_tree(rev) {
        String::new()
    } else {
        rev.to_owned()
    }
}

fn shown(rev: &str) -> String {
    if rev.is_empty() {
        WORKING_TREE.to_owned()
    } else {
        rev.to_owned()
    }
}

fn parse_shown(text: &str) -> String {
    let t = text.trim();
    if t.eq_ignore_ascii_case(WORKING_TREE) {
        String::new()
    } else {
        t.to_owned()
    }
}

struct State {
    rev1: String,
    rev2: String,
    files: Vec<ChangedFile>,
    filtered: Vec<usize>,
}

/// Shows the dialog for `rev1` (base) against `rev2`; `filter` is the
/// initial path filter (the path given on the command line).
pub fn run(
    repo_dir: PathBuf,
    settings: Settings,
    rev1: &str,
    rev2: &str,
    filter: &str,
) -> DialogResult<i32> {
    let repo = Repo::discover(&repo_dir)?;
    let dlg = FileDiffDialog::new()?;
    dlg.set_dialog_title(crate::window_title(repo.workdir(), "Changed Files").into());
    let mut refs = vec!["HEAD".to_owned(), WORKING_TREE.to_owned()];
    refs.extend(repo.local_branches().unwrap_or_default());
    refs.extend(
        repo.remote_branches()
            .unwrap_or_default()
            .into_iter()
            .map(|b| format!("remotes/{b}")),
    );
    refs.extend(repo.tags().unwrap_or_default());
    dlg.set_refs(crate::remote_util::strings(&refs));
    dlg.set_filter_text(filter.into());

    let state = Rc::new(RefCell::new(State {
        rev1: norm(rev1),
        rev2: norm(rev2),
        files: Vec::new(),
        filtered: Vec::new(),
    }));
    dlg.set_rev1_text(shown(&state.borrow().rev1).into());
    dlg.set_rev2_text(shown(&state.borrow().rev2).into());
    let repo = Rc::new(repo);
    let rows_model = Rc::new(VecModel::<FileRow>::default());
    dlg.set_rows(ModelRc::from(rows_model.clone()));
    let menu = Rc::new(FileDiffMenu {
        state: state.clone(),
        rows: rows_model.clone(),
        sel: RefCell::new(Selection::default()),
        cache: MenuCache::default(),
        repo: repo.clone(),
        settings: settings.clone(),
        reload: RefCell::new(None),
    });
    menu.install(&dlg.global::<FileListBridge>());

    let apply_filter = {
        let state = state.clone();
        let weak = dlg.as_weak();
        let rows_model = rows_model.clone();
        let menu = menu.clone();
        Rc::new(move || {
            let Some(d) = weak.upgrade() else { return };
            let mut st = state.borrow_mut();
            let filter = d.get_filter_text().to_string();
            st.filtered = (0..st.files.len())
                .filter(|&i| matches_filter(&st.files[i], &filter))
                .collect();
            let rows: Vec<FileRow> = st.filtered.iter().map(|&i| to_row(&st.files[i])).collect();
            d.set_list_text(if st.files.is_empty() && d.get_list_text().is_empty() {
                "No differences found!".into()
            } else {
                d.get_list_text()
            });
            rows_model.set_vec(rows);
            d.set_current_row(-1);
            drop(st);
            menu.reset_selection();
        })
    };

    let reload = {
        let state = state.clone();
        let weak = dlg.as_weak();
        let repo = repo.clone();
        let apply_filter = apply_filter.clone();
        Rc::new(move || {
            let Some(d) = weak.upgrade() else { return };
            let r1 = parse_shown(&d.get_rev1_text());
            let r2 = parse_shown(&d.get_rev2_text());
            for rev in [&r1, &r2] {
                if !rev.is_empty() && repo.resolve_commit(rev).is_none() {
                    // IDS_PROC_REFINVALID
                    d.set_list_text(format!("\"{rev}\" is invalid.").into());
                    d.set_rows(ModelRc::default());
                    return;
                }
            }
            let info = |rev: &str| match repo.rev_summary(rev) {
                Some((short, subject, _)) => format!("{short}: {subject}"),
                None => WORKING_TREE.to_owned(),
            };
            d.set_rev1_info(info(&r1).into());
            d.set_rev2_info(info(&r2).into());
            // "Newer commit time" captions.
            let t1 = repo.rev_summary(&r1).map(|x| x.2);
            let t2 = repo.rev_summary(&r2).map(|x| x.2);
            d.set_rev1_group("Version 1 (Base)".into());
            d.set_rev2_group("Version 2".into());
            if let (Some(a), Some(b)) = (t1, t2) {
                if a > b {
                    d.set_rev1_group("Version 1 (Base, Newer commit time)".into());
                } else if a < b {
                    d.set_rev2_group("Version 2 (Newer commit time)".into());
                }
            }
            d.set_log_enabled(!r1.is_empty() && !r2.is_empty());
            d.set_swap_enabled(!r2.is_empty());
            let opts = DiffOptions {
                ignore_space_at_eol: d.get_opt_eol(),
                ignore_space_change: d.get_opt_change(),
                ignore_all_space: d.get_opt_all(),
                ignore_blank_lines: d.get_opt_blank(),
            };
            d.set_list_text(SharedString::new());
            match repo.changed_files(&r1, &r2, &opts, d.get_opt_ancestor()) {
                Ok(files) => {
                    let mut st = state.borrow_mut();
                    st.files = files;
                    st.rev1 = r1;
                    st.rev2 = r2;
                }
                Err(e) => {
                    state.borrow_mut().files.clear();
                    d.set_list_text(e.to_string().into());
                }
            }
            apply_filter();
            if d.get_patch_visible() {
                d.set_patch_text(patch_text(&repo, &state.borrow(), None).into());
            }
        })
    };
    *menu.reload.borrow_mut() = Some(reload.clone());
    reload();

    {
        let reload = reload.clone();
        dlg.on_rev_entered(move || reload());
    }
    {
        let reload = reload.clone();
        dlg.on_options_changed(move || reload());
    }
    {
        // `ClickRevButton` (FileDiffDlg.cpp:1033): Reference Browser, Log or
        // RefLog picks the version, then the differences are re-read.
        let weak = dlg.as_weak();
        let workdir = repo.workdir().to_owned();
        dlg.on_rev_button(move |which| {
            let Some(d) = weak.upgrade() else { return };
            let (choice, current) = if which == 1 {
                (d.get_rev1_choice(), d.get_rev1_text())
            } else {
                (d.get_rev2_choice(), d.get_rev2_text())
            };
            let base = |command: &str| Invocation::new(command).with_paths([workdir.clone()]);
            let inv = match choice {
                0 => base("refbrowse"),
                1 => {
                    let rev = parse_shown(&current);
                    if is_working_tree(&rev) {
                        base("log")
                    } else {
                        base("log").with("range", rev.clone()).with("rev", rev)
                    }
                }
                _ => base("reflog"),
            };
            let weak = weak.clone();
            crate::pick::launcher_picker(inv, move |picked| {
                if let Some(d) = weak.upgrade() {
                    if which == 1 {
                        d.set_rev1_text(picked.into());
                    } else {
                        d.set_rev2_text(picked.into());
                    }
                    d.invoke_rev_entered();
                }
            });
        });
    }
    {
        let reload = reload.clone();
        let weak = dlg.as_weak();
        dlg.on_swap(move || {
            if let Some(d) = weak.upgrade() {
                let (a, b) = (d.get_rev1_text(), d.get_rev2_text());
                d.set_rev1_text(b);
                d.set_rev2_text(a);
                reload();
            }
        });
    }
    {
        let apply_filter = apply_filter.clone();
        dlg.on_filter_edited(move || apply_filter());
    }
    {
        let state = state.clone();
        let workdir = repo.workdir().to_owned();
        dlg.on_show_log(move || {
            let st = state.borrow();
            spawn_launcher(
                &Invocation::new("log")
                    .with_paths([workdir.clone()])
                    .with("range", format!("{}..{}", st.rev1, st.rev2)),
            );
        });
    }
    {
        let state = state.clone();
        let repo = repo.clone();
        dlg.on_row_activated(move |i| {
            let st = state.borrow();
            let Some(&idx) = st.filtered.get(i as usize) else {
                return;
            };
            let f = st.files[idx].clone();
            if let Err(e) = do_diff(&repo, &settings, &st.rev1, &st.rev2, &f) {
                msgbox::ask_async("KoopaGit", &e.to_string(), &["OK"], |_| {});
            }
        });
    }
    {
        let state = state.clone();
        let repo = repo.clone();
        let weak = dlg.as_weak();
        dlg.on_toggle_patch(move || {
            let Some(d) = weak.upgrade() else { return };
            let show = !d.get_patch_visible();
            d.set_patch_visible(show);
            if show {
                d.set_patch_text(patch_text(&repo, &state.borrow(), None).into());
            }
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

    crate::snapshot::arm(&dlg);
    dlg.run()?;
    Ok(0)
}

/// The unified diff shown in the patch pane.
fn patch_text(repo: &Repo, st: &State, only: Option<&str>) -> String {
    let mut args = vec!["diff".to_owned(), "--no-color".to_owned(), "-M".to_owned()];
    match (st.rev1.is_empty(), st.rev2.is_empty()) {
        (false, false) => args.extend([st.rev1.clone(), st.rev2.clone()]),
        (false, true) => args.push(st.rev1.clone()),
        (true, false) => args.extend(["-R".to_owned(), st.rev2.clone()]),
        (true, true) => {}
    }
    args.push("--".to_owned());
    if let Some(p) = only {
        args.push(p.to_owned());
    }
    repo.git().run(args).unwrap_or_else(|e| e.to_string())
}

/// `CFileDiffDlg::DoDiff`: two-way diff of one row.
fn do_diff(
    repo: &Repo,
    settings: &Settings,
    rev1: &str,
    rev2: &str,
    f: &ChangedFile,
) -> DialogResult {
    let side = |rev: &str, path: &str| -> DialogResult<(PathBuf, String)> {
        if rev.is_empty() {
            Ok((repo.workdir().join(path), wt_title(path)))
        } else {
            Ok((repo.extract_blob(rev, path)?, rev_title(repo, path, rev)))
        }
    };
    // Added files compare against nothing at rev1; deleted ones against nothing
    // at rev2 (extract_blob yields an empty file for a missing path).
    let old_path = match f.action {
        Action::Renamed | Action::Copied => f.old_path.clone().unwrap_or_else(|| f.path.clone()),
        _ => f.path.clone(),
    };
    let (left, left_title) = side(rev1, &old_path)?;
    let (right, right_title) = side(rev2, &f.path)?;
    start_diff(
        settings,
        &left,
        &right,
        &left_title,
        &right_title,
        false,
        None,
    );
    Ok(())
}

/// Entry from the launcher: `/command:showcompare` (without `/unified`) on a
/// folder or the repository root.
pub fn from_context(ctx: &Context, rev1: &str, rev2: &str) -> DialogResult<i32> {
    let filter = ctx
        .paths
        .first()
        .and_then(|p| ctx.repo.relative_path(p))
        .unwrap_or_default();
    run(
        ctx.repo.workdir().to_owned(),
        ctx.settings.clone(),
        rev1,
        rev2,
        &filter,
    )
}

/// Selection and context menu of the Changed Files list (spec-menus §1.7).
struct FileDiffMenu {
    state: Rc<RefCell<State>>,
    rows: Rc<VecModel<FileRow>>,
    sel: RefCell<Selection>,
    cache: MenuCache,
    repo: Rc<Repo>,
    settings: Settings,
    reload: RefCell<Option<Rc<dyn Fn()>>>,
}

impl FileDiffMenu {
    fn install(self: &Rc<Self>, bridge: &FileListBridge<'_>) {
        let me = Rc::downgrade(self);
        {
            let me = me.clone();
            bridge.on_pressed(move |_, row, toggle, range| {
                if let Some(m) = me.upgrade() {
                    m.sel
                        .borrow_mut()
                        .click(row as usize, Mods { toggle, range });
                    m.apply_selection();
                }
            });
        }
        {
            let me = me.clone();
            bridge.on_right_pressed(move |_, row, _| {
                if let Some(m) = me.upgrade() {
                    m.sel.borrow_mut().right_click(row as usize);
                    m.apply_selection();
                }
            });
        }
        {
            let me = me.clone();
            bridge.on_menu(move |_, row| match me.upgrade() {
                Some(m) => m.menu(row),
                None => ModelRc::default(),
            });
        }
        {
            let me = me.clone();
            bridge.on_chosen(move |_, _, id| {
                if let (Some(m), Some(a)) = (me.upgrade(), FileDiffAction::from_id(id)) {
                    m.run(a);
                }
            });
        }
        {
            let me = me.clone();
            bridge.on_key(move |_, code, shift| {
                let Some(m) = me.upgrade() else { return -1 };
                let len = m.rows.row_count();
                let current = {
                    let mut sel = m.sel.borrow_mut();
                    match code {
                        0 => sel.step(-1, len, shift),
                        1 => sel.step(1, len, shift),
                        2 => sel.select_all(len),
                        3 => sel.step(-(len as isize), len, shift),
                        4 => sel.step(len as isize, len, shift),
                        _ => {}
                    }
                    sel.current()
                };
                m.apply_selection();
                current.map_or(-1, |c| c as i32)
            });
        }
        {
            // Enter in the list: diff the selection-mark file, as on double-click.
            let me = me.clone();
            bridge.on_activate(move |_| {
                let Some(m) = me.upgrade() else { return false };
                let current = m.sel.borrow().current();
                let file = current.and_then(|r| {
                    let st = m.state.borrow();
                    st.filtered.get(r).map(|&i| st.files[i].clone())
                });
                if let Some(f) = file {
                    let (rev1, rev2) = {
                        let st = m.state.borrow();
                        (st.rev1.clone(), st.rev2.clone())
                    };
                    if let Err(e) = do_diff(&m.repo, &m.settings, &rev1, &rev2, &f) {
                        msgbox::ask_async("KoopaGit", &e.to_string(), &["OK"], |_| {});
                    }
                }
                true
            });
        }
        // Ctrl+C / Ctrl+Insert: `CopySelectionToClipboard()` (paths), with or without Shift.
        bridge.on_copy(move |_, _| {
            if let Some(m) = me.upgrade() {
                if !m.sel.borrow().is_empty() {
                    m.run(FileDiffAction::CopyPaths);
                }
            }
        });
    }

    /// Called after the rows were rebuilt (reload or filter).
    fn reset_selection(&self) {
        *self.sel.borrow_mut() = Selection::default();
        self.cache.invalidate();
    }

    /// Updates the `selected` flags in place, so a row that owns an open
    /// native menu is never destroyed.
    fn apply_selection(&self) {
        let sel = self.sel.borrow();
        for i in 0..self.rows.row_count() {
            if let Some(mut row) = self.rows.row_data(i) {
                let want = sel.contains(i);
                if row.selected != want {
                    row.selected = want;
                    self.rows.set_row_data(i, row);
                }
            }
        }
        self.cache.invalidate();
    }

    fn selected(&self) -> Vec<ChangedFile> {
        let st = self.state.borrow();
        self.sel
            .borrow()
            .rows()
            .filter_map(|r| st.filtered.get(r).map(|&i| st.files[i].clone()))
            .collect()
    }

    fn menu(&self, row: i32) -> ModelRc<MenuBlock> {
        let Ok(r) = usize::try_from(row) else {
            return ModelRc::default();
        };
        if !self.sel.borrow().contains(r) {
            self.sel.borrow_mut().right_click(r);
            self.apply_selection();
        }
        self.cache.get(row, || {
            let files = self.selected();
            let st = self.state.borrow();
            let first = files.first();
            fdm::build(&FileDiffMenuInput {
                selected: files.len(),
                bare: false,
                rev1: st.rev1.clone(),
                rev2: st.rev2.clone(),
                first_is_dir: first.is_some_and(|f| f.is_submodule),
                first_deleted: first.is_some_and(|f| f.action == Action::Deleted),
            })
        })
    }

    fn full(&self, rel: &str) -> PathBuf {
        self.repo.workdir().join(rel)
    }

    fn after_change(&self) {
        let reload = self.reload.borrow().clone();
        if let Some(r) = reload {
            r();
        }
    }

    fn run(&self, action: FileDiffAction) {
        use FileDiffAction as A;
        let files = self.selected();
        let Some(first) = files.first().cloned() else {
            return;
        };
        let (rev1, rev2) = {
            let st = self.state.borrow();
            (st.rev1.clone(), st.rev2.clone())
        };
        let report = |r: DialogResult| {
            if let Err(e) = r {
                msgbox::ask_async("KoopaGit", &e.to_string(), &["OK"], |_| {});
            }
        };
        match action {
            A::Compare => {
                for f in &files {
                    report(do_diff(&self.repo, &self.settings, &rev1, &rev2, f));
                }
            }
            A::GnuDiff => {
                for f in &files {
                    spawn_launcher(&unified_invocation(&self.full(&f.path), &rev1, &rev2));
                }
            }
            A::Revert1 | A::Revert2 => {
                let rev = if action == A::Revert1 { &rev1 } else { &rev2 };
                let git = self.repo.git();
                let mut errors = Vec::new();
                for f in &files {
                    // Revert to rev1 restores the file under its old name.
                    let path = match (&f.old_path, action) {
                        (Some(old), A::Revert1) => old.clone(),
                        _ => f.path.clone(),
                    };
                    let exists = git
                        .output(["cat-file", "-e", &format!("{rev}:{path}")])
                        .is_ok_and(|o| o.success());
                    let args: Vec<String> = if exists {
                        revert_args(rev, &path)
                    } else {
                        ["rm", "--cached", "--ignore-unmatch", "--", &path]
                            .map(str::to_owned)
                            .to_vec()
                    };
                    if let Err(e) = git.run(&args) {
                        errors.push(e.to_string());
                    }
                }
                if !errors.is_empty() {
                    msgbox::ask_async("KoopaGit", &errors.join("\n"), &["OK"], |_| {});
                }
                self.after_change();
            }
            A::Log => {
                let mut inv = Invocation::new("log").with_paths([self.full(&first.path)]);
                if first.is_submodule {
                    inv = inv.with_switch("submodule");
                }
                if !rev2.is_empty() {
                    inv = inv.with("endrev", rev2.clone()).with("rev", rev2.clone());
                }
                spawn_launcher(&inv);
            }
            A::Blame => {
                let (path, rev) = blame_target(&first, &rev1, &rev2);
                let mut inv = Invocation::new("blame").with_paths([self.full(&path)]);
                if !rev.is_empty() {
                    inv = inv.with("endrev", rev);
                }
                spawn_launcher(&inv);
            }
            A::Export => {
                let Some(dir) =
                    crate::pick::folder("Export selection to", Some(self.repo.workdir()))
                else {
                    return;
                };
                for f in files.iter().filter(|f| !f.is_submodule) {
                    report(self.export_one(f, &rev2, &dir));
                }
            }
            A::LogSubmodule => {
                spawn_launcher(&Invocation::new("log").with_paths([self.full(&first.path)]));
            }
            A::SaveAs => {
                let Some(out) =
                    crate::pick::save_file("Save list of selected files", None, "changes.txt")
                else {
                    return;
                };
                let text = saved_list(&rev1, &rev2, &files);
                report(std::fs::write(out, text).map_err(Into::into));
            }
            A::CopyPaths | A::CopyAll => {
                let rows: Vec<FileRow> = self
                    .sel
                    .borrow()
                    .rows()
                    .filter_map(|r| self.rows.row_data(r))
                    .collect();
                let text = clipboard_text(&rows, action == A::CopyAll);
                if !crate::clipboard::copy(&text) {
                    msgbox::ask_async("Copy to clipboard", &text, &["OK"], |_| {});
                }
            }
        }
    }

    /// Writes one file as of `rev` (working tree when empty) below `dir`.
    fn export_one(&self, f: &ChangedFile, rev: &str, dir: &Path) -> DialogResult {
        let target = dir.join(&f.path);
        if let Some(p) = target.parent() {
            std::fs::create_dir_all(p)?;
        }
        if rev.is_empty() {
            std::fs::copy(self.full(&f.path), &target)?;
        } else {
            let out = self
                .repo
                .git()
                .command(["show", &format!("{rev}:{}", f.path)])
                .output()?;
            if !out.status.success() {
                return Err(String::from_utf8_lossy(&out.stderr).into_owned().into());
            }
            std::fs::write(&target, out.stdout)?;
        }
        Ok(())
    }
}

/// `git checkout <rev> -- <path>` (the old path for renames at rev1).
fn revert_args(rev: &str, path: &str) -> Vec<String> {
    ["checkout", rev, "--", path].map(str::to_owned).to_vec()
}

/// `/command:showcompare /unified` for one file; the working tree as rev1 is
/// shown as `rev2..Working Tree`.
fn unified_invocation(full: &Path, rev1: &str, rev2: &str) -> Invocation {
    let inv = Invocation::new("showcompare")
        .with_paths([full.to_owned()])
        .with_switch("unified");
    match (rev1.is_empty(), rev2.is_empty()) {
        (false, false) => inv.with("revision1", rev1).with("revision2", rev2),
        (false, true) => inv.with("revision1", rev1),
        (true, false) => inv.with("revision1", rev2),
        (true, true) => inv,
    }
}

/// Blame target (`FileDiffDlg.cpp:642-665`): rev2, a deleted file at rev1, a
/// renamed file with rev2 = working tree under its old path at rev1.
fn blame_target(f: &ChangedFile, rev1: &str, rev2: &str) -> (String, String) {
    if f.action == Action::Deleted {
        return (f.path.clone(), rev1.to_owned());
    }
    match (&f.old_path, rev2.is_empty()) {
        (Some(old), true) if f.action == Action::Renamed => (old.clone(), rev1.to_owned()),
        _ => (f.path.clone(), rev2.to_owned()),
    }
}

/// "Save list of selected files to...": a header line, then one path per line.
fn saved_list(rev1: &str, rev2: &str, files: &[ChangedFile]) -> String {
    let mut text = format!(
        "Changed files between {} and {}\n",
        shown(rev1),
        shown(rev2)
    );
    for f in files {
        text.push_str(&f.path);
        text.push('\n');
    }
    text
}

/// "Copy paths" (`<col0>\t\r\n`) / "Copy all information" (`c0\tc1\tc2\tc3\tc4\r\n`).
fn clipboard_text(rows: &[FileRow], all: bool) -> String {
    rows.iter()
        .map(|r| {
            if all {
                format!(
                    "{}\t{}\t{}\t{}\t{}\r\n",
                    r.path, r.extension, r.status, r.added, r.removed
                )
            } else {
                format!("{}\t\r\n", r.path)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cf(path: &str, old: Option<&str>, action: Action) -> ChangedFile {
        ChangedFile {
            path: path.into(),
            old_path: old.map(Into::into),
            action,
            added: Some(1),
            removed: None,
            is_submodule: false,
            has_stat: true,
        }
    }

    #[test]
    fn rows_and_filter() {
        let r = cf("src/new.rs", Some("src/old.rs"), Action::Renamed);
        assert_eq!(row_name(&r), "src/new.rs (from src/old.rs)");
        assert!(matches_filter(&r, "OLD"));
        assert!(!matches_filter(&r, "lib"));
        let row = to_row(&r);
        assert_eq!(row.status.as_str(), "Rename");
        assert_eq!(row.removed.as_str(), "-");
        assert_eq!(norm("0000000000000000000000000000000000000000"), "");
        assert_eq!(parse_shown("Working Tree"), "");
        assert_eq!(shown(""), "Working Tree");
    }

    #[test]
    fn menu_helpers() {
        let renamed = cf("new.rs", Some("old.rs"), Action::Renamed);
        assert_eq!(
            blame_target(&renamed, "A", ""),
            ("old.rs".to_owned(), "A".to_owned())
        );
        assert_eq!(
            blame_target(&renamed, "A", "B"),
            ("new.rs".to_owned(), "B".to_owned())
        );
        let deleted = cf("gone.rs", None, Action::Deleted);
        assert_eq!(blame_target(&deleted, "A", "B").1, "A");
        assert_eq!(
            saved_list("A", "", &[deleted]),
            "Changed files between A and Working Tree\ngone.rs\n"
        );
        let row = to_row(&cf("a.txt", None, Action::Modified));
        assert_eq!(
            clipboard_text(std::slice::from_ref(&row), false),
            "a.txt\t\r\n"
        );
        assert_eq!(
            clipboard_text(&[row], true),
            "a.txt\t.txt\tModified\t1\t-\r\n"
        );
        let inv = unified_invocation(Path::new("/r/a"), "", "B");
        assert_eq!(inv.get("revision1"), Some("B"));
        assert!(inv.has("unified"));
        assert_eq!(revert_args("HEAD", "x").join(" "), "checkout HEAD -- x");
    }
}
