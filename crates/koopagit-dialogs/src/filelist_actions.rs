//! The file list's selection, context menu and menu actions
//! (`CGitStatusListCtrl`, spec-menus §1.4–1.6).
//!
//! A dialog wires a [`FileList`](crate::FileRow) like this:
//!
//! ```ignore
//! let menu = FileListMenu::new(env, FileListHost { items, model, refresh });
//! FileListMenu::install(&dlg.global::<FileListBridge>(), &[menu.clone()]);
//! // after every model rebuild:
//! menu.after_rebuild();
//! ```
//!
//! and sets `list-id` / `menu-enabled: true` on the `FileList` in Slint.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};

use koopagit_core::Repo;
use koopagit_core::fileops;
use koopagit_core::git_cli::Git;
use koopagit_core::launcher::{CloseOnEnd, Invocation};
use koopagit_core::listops::{self, IndexFlag};
use koopagit_core::settings::Settings;
use koopagit_core::status::Action as StatusAction;
use slint::{Model, ModelRc, VecModel};

use crate::filelist::Item;
use crate::filelist_menu::{self as fm, FileAction as A, ListInfo, PopMask, RowInfo, st};
use crate::menus::{ActionId, MenuCache};
use crate::progress::{PostAction, ProgressJob};
use crate::selection::{Mods, Selection};
use crate::{FileListBridge, FileRow, MenuBlock, keep_alive, msgbox, spawn_launcher};

/// Copies a "Restore after commit" backup back over `dest`, touches it so the
/// change is noticed (`CopyFile` + `CPathUtils::Touch`) and removes the backup.
pub fn restore_backup(backup: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::copy(backup, dest)?;
    std::fs::File::options()
        .write(true)
        .open(dest)?
        .set_modified(std::time::SystemTime::now())?;
    let _ = std::fs::remove_file(backup);
    Ok(())
}

/// Restores every backup into `workdir` (`CCommitDlg::RestoreFiles`).
pub fn restore_all(workdir: &Path, backups: &HashMap<String, PathBuf>) {
    for (rel, backup) in backups {
        let _ = restore_backup(backup, &workdir.join(rel));
    }
}

/// A refresh function bound after the menu is created (the dialog's reload
/// closure usually needs the menu itself).
pub type LateRefresh = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

/// The owning dialog's data, as closures over its own state.
pub struct FileListHost {
    /// The rows behind the list, in model order.
    pub items: Box<dyn Fn() -> Vec<Item>>,
    pub model: Rc<VecModel<FileRow>>,
    /// Reloads the list (the dialog's F5 path); must end with `after_rebuild()`.
    pub refresh: Box<dyn Fn()>,
}

/// Dialog-level settings of one list.
#[derive(Debug, Clone)]
pub struct FileListEnv {
    /// Matches the `FileList`'s `list-id` in Slint.
    pub list_id: i32,
    pub workdir: PathBuf,
    pub mask: PopMask,
    pub has_wc: bool,
    /// Commit dialog amending: diffs and reverts use `HEAD~1`.
    pub amend: bool,
    /// The revision shown; `None` = working tree.
    pub current_version: Option<String>,
    /// SyncDlg's `m_Rev1`/`m_Rev2`.
    pub revisions: Option<(String, String)>,
    /// Window title used for progress dialogs started from the menu.
    pub title: String,
}

impl FileListEnv {
    pub fn working_tree(list_id: i32, workdir: &Path, mask: PopMask, title: &str) -> Self {
        FileListEnv {
            list_id,
            workdir: workdir.to_owned(),
            mask,
            has_wc: true,
            amend: false,
            current_version: None,
            revisions: None,
            title: title.to_owned(),
        }
    }
}

/// A file remembered by "Mark for comparison".
#[derive(Debug, Clone)]
struct Marked {
    full: PathBuf,
    rel: String,
    rev: Option<String>,
}

/// A dialog's handler for a row (Enter) and for a row's new check state (Space).
type RowHook = Rc<dyn Fn(usize)>;
type ToggleHook = Rc<dyn Fn(usize, bool)>;

pub struct FileListMenu {
    env: RefCell<FileListEnv>,
    host: FileListHost,
    selection: RefCell<Selection>,
    cache: MenuCache,
    shift: Cell<bool>,
    backups: RefCell<HashMap<String, PathBuf>>,
    marked: RefCell<Option<Marked>>,
    /// The dialog's per-row activation (Enter) and check-mark change (Space).
    on_activate: RefCell<Option<RowHook>>,
    on_toggle: RefCell<Option<ToggleHook>>,
    me: RefCell<Weak<FileListMenu>>,
}

impl FileListMenu {
    pub fn new(env: FileListEnv, host: FileListHost) -> Rc<Self> {
        let menu = Rc::new(FileListMenu {
            env: RefCell::new(env),
            host,
            selection: RefCell::new(Selection::default()),
            cache: MenuCache::default(),
            shift: Cell::new(false),
            backups: RefCell::new(HashMap::new()),
            marked: RefCell::new(None),
            on_activate: RefCell::new(None),
            on_toggle: RefCell::new(None),
            me: RefCell::new(Weak::new()),
        });
        *menu.me.borrow_mut() = Rc::downgrade(&menu);
        menu
    }

    /// Routes the window's `FileListBridge` events to `lists` by `list-id`.
    pub fn install(bridge: &FileListBridge<'_>, lists: &[Rc<FileListMenu>]) {
        let find = {
            let lists: Vec<Weak<FileListMenu>> = lists.iter().map(Rc::downgrade).collect();
            Rc::new(move |id: i32| {
                lists
                    .iter()
                    .filter_map(Weak::upgrade)
                    .find(|l| l.env.borrow().list_id == id)
            })
        };
        {
            let find = find.clone();
            bridge.on_pressed(move |list, row, toggle, range| {
                if let Some(l) = find(list) {
                    l.click(row as usize, Mods { toggle, range });
                }
            });
        }
        {
            let find = find.clone();
            bridge.on_right_pressed(move |list, row, shift| {
                if let Some(l) = find(list) {
                    l.shift.set(shift);
                    l.right_click(row as usize);
                }
            });
        }
        {
            let find = find.clone();
            bridge.on_menu(move |list, row| match find(list) {
                Some(l) => l.menu(row),
                None => ModelRc::default(),
            });
        }
        {
            let find = find.clone();
            bridge.on_chosen(move |list, _row, id| {
                if let Some(l) = find(list) {
                    l.run(id);
                }
            });
        }
        {
            let find = find.clone();
            bridge.on_key(move |list, code, shift| match find(list) {
                Some(l) => l.key(code, shift),
                None => -1,
            });
        }
        {
            let find = find.clone();
            bridge.on_activate(move |list| find(list).is_some_and(|l| l.activate()));
        }
        {
            let find = find.clone();
            bridge.on_space(move |list| {
                if let Some(l) = find(list) {
                    l.space();
                }
            });
        }
        {
            let find = find.clone();
            bridge.on_copy(move |list, with_status| {
                if let Some(l) = find(list) {
                    l.copy(with_status);
                }
            });
        }
        bridge.on_delete(move |list, permanent| {
            if let Some(l) = find(list) {
                l.delete_key(permanent);
            }
        });
    }

    /// Enter runs `f` on every selected row (`OnNMReturn`). Without it the list
    /// activates only its current row.
    pub fn set_on_activate(&self, f: impl Fn(usize) + 'static) {
        *self.on_activate.borrow_mut() = Some(Rc::new(f));
    }

    /// Space changes check marks through `f` (row, checked), as a click on the
    /// row's checkbox does. Without it Space does nothing.
    pub fn set_on_toggle(&self, f: impl Fn(usize, bool) + 'static) {
        *self.on_toggle.borrow_mut() = Some(Rc::new(f));
    }

    /// Re-applies the selection after the dialog rebuilt its model.
    pub fn after_rebuild(&self) {
        let len = self.host.model.row_count();
        self.selection.borrow_mut().clamp(len);
        self.apply_selection();
        self.cache.invalidate();
    }

    /// Changes the shown revision (Log changed-files pane); clears the selection.
    pub fn set_current_version(&self, rev: Option<String>) {
        self.env.borrow_mut().current_version = rev;
        *self.selection.borrow_mut() = Selection::default();
        self.cache.invalidate();
    }

    /// SyncDlg's two revisions for "Compare revisions".
    pub fn set_revisions(&self, revs: Option<(String, String)>) {
        self.env.borrow_mut().revisions = revs;
        self.cache.invalidate();
    }

    pub fn set_amend(&self, amend: bool) {
        self.env.borrow_mut().amend = amend;
        self.cache.invalidate();
    }

    /// Rows currently selected (model order).
    pub fn selected_rows(&self) -> Vec<usize> {
        self.selection.borrow().to_vec()
    }

    /// "Restore after commit" backups: repository path → backup file.
    pub fn backups(&self) -> HashMap<String, PathBuf> {
        self.backups.borrow().clone()
    }

    /// Replaces the backups, e.g. with those of a previous Commit dialog round.
    pub fn set_backups(&self, backups: HashMap<String, PathBuf>) {
        *self.backups.borrow_mut() = backups;
    }

    /// Takes the backups out, leaving none.
    pub fn take_backups(&self) -> HashMap<String, PathBuf> {
        std::mem::take(&mut *self.backups.borrow_mut())
    }

    fn click(&self, row: usize, mods: Mods) {
        self.selection.borrow_mut().click(row, mods);
        self.apply_selection();
        self.cache.invalidate();
    }

    fn right_click(&self, row: usize) {
        self.selection.borrow_mut().right_click(row);
        self.apply_selection();
        self.cache.invalidate();
    }

    /// Navigation keys; returns the new current row (-1 = none).
    fn key(&self, code: i32, shift: bool) -> i32 {
        let len = self.host.model.row_count();
        let current = {
            let mut sel = self.selection.borrow_mut();
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
        self.apply_selection();
        self.cache.invalidate();
        current.map_or(-1, |c| c as i32)
    }

    /// Enter: activates every selected row; `false` when the dialog has no hook.
    fn activate(&self) -> bool {
        let Some(f) = self.on_activate.borrow().clone() else {
            return false;
        };
        let rows = self.selection.borrow().to_vec();
        for row in rows {
            f(row);
        }
        true
    }

    /// Space: flips the current row's check mark and gives every selected row
    /// the same state when the current row is part of the selection.
    fn space(&self) {
        let Some(f) = self.on_toggle.borrow().clone() else {
            return;
        };
        let (current, rows) = {
            let sel = self.selection.borrow();
            let Some(current) = sel.current() else { return };
            let rows = if sel.contains(current) {
                sel.to_vec()
            } else {
                vec![current]
            };
            (current, rows)
        };
        let model = &self.host.model;
        let Some(checked) = model.row_data(current).map(|r| !r.checked) else {
            return;
        };
        for row in rows {
            if let Some(mut data) = model.row_data(row) {
                if data.checked != checked {
                    data.checked = checked;
                    model.set_row_data(row, data);
                }
                f(row, checked);
            }
        }
    }

    /// Ctrl+C copies the selected relative paths; Ctrl+Shift+C adds the status column.
    fn copy(&self, with_status: bool) {
        let text = self.copy_text(with_status);
        if text.is_empty() {
            return;
        }
        if !crate::clipboard::copy(&text) {
            msgbox::ask_async("Copy to clipboard", &text, &["OK"], |_| {});
        }
    }

    /// `CopySelectedEntriesToClipboard(GITSLC_COLFILENAME [| GITSLC_COLSTATUS])`:
    /// relative paths, or a title line and then path and status per row.
    fn copy_text(&self, with_status: bool) -> String {
        let items = (self.host.items)();
        let model = &self.host.model;
        let sel = self.selection.borrow();
        if sel.is_empty() {
            return String::new();
        }
        let mut lines = Vec::new();
        if with_status {
            lines.push("Path\tStatus".to_owned());
        }
        for i in sel.rows() {
            let Some(item) = items.get(i) else { continue };
            if with_status {
                let status = model.row_data(i).map(|r| r.status).unwrap_or_default();
                lines.push(format!("{}\t{status}", item.entry.path));
            } else {
                lines.push(item.entry.path.clone());
            }
        }
        join_lines(lines.into_iter())
    }

    /// Delete / Shift+Delete: only when the list allows "Delete" and the current
    /// row is unversioned or ignored; then deletes every selected row.
    fn delete_key(&self, permanent: bool) {
        if !self.env.borrow().mask.allows(A::Delete) {
            return;
        }
        let items = (self.host.items)();
        let current = self.selection.borrow().current();
        let deletable = current.and_then(|c| items.get(c)).is_some_and(|i| {
            matches!(
                i.entry.action(),
                StatusAction::Unversioned | StatusAction::Ignored
            )
        });
        let selected = self.selected_items();
        if !deletable || selected.is_empty() {
            return;
        }
        if permanent {
            let text = match selected.as_slice() {
                [one] => format!(
                    "Are you sure you want to permanently delete \"{}\"?",
                    one.entry.path
                ),
                _ => format!(
                    "Are you sure you want to permanently delete these {} items?",
                    selected.len()
                ),
            };
            self.confirm(&text, move |m| m.report(m.delete_files(&selected, true)));
        } else {
            self.report(self.delete_files(&selected, false));
        }
    }

    fn report(&self, r: crate::DialogResult) {
        if let Err(e) = r {
            msgbox::ask_async("KoopaGit", &e.to_string(), &["OK"], |_| {});
        }
    }

    fn apply_selection(&self) {
        let sel = self.selection.borrow();
        let model = &self.host.model;
        for i in 0..model.row_count() {
            if let Some(mut row) = model.row_data(i) {
                let want = sel.contains(i);
                if row.selected != want {
                    row.selected = want;
                    model.set_row_data(i, row);
                }
            }
        }
    }

    fn menu(&self, row: i32) -> ModelRc<MenuBlock> {
        let Ok(r) = usize::try_from(row) else {
            return ModelRc::default();
        };
        if !self.selection.borrow().contains(r) {
            self.right_click(r);
        }
        self.cache.get(row, || self.build(r))
    }

    fn selected_items(&self) -> Vec<Item> {
        let items = (self.host.items)();
        self.selection
            .borrow()
            .rows()
            .filter_map(|i| items.get(i).cloned())
            .collect()
    }

    fn row_info(&self, repo: Option<&Repo>, item: &Item) -> RowInfo {
        let env = self.env.borrow();
        let mut flags = fm::status_flags(item.entry.action());
        if env.current_version.is_none() && flags & (st::UNV | st::IGN) == 0 {
            if let Some(repo) = repo {
                let (assume, skip) = repo.index_flags(env.workdir.join(&item.entry.path));
                if assume {
                    flags |= st::ASSUMEVALID;
                }
                if skip {
                    flags |= st::SKIPWORKTREE;
                }
            }
        }
        RowInfo {
            path: item.entry.path.clone(),
            old_path: item.entry.old_path.clone(),
            flags,
            is_dir: item.is_dir,
            has_backup: self.backups.borrow().contains_key(&item.entry.path),
            lfs_locked: None,
        }
    }

    fn list_info(&self, repo: Option<&Repo>) -> ListInfo {
        let env = self.env.borrow();
        let state = repo.map(Repo::state).unwrap_or_default();
        let parent_title = env.current_version.as_ref().and_then(|cv| {
            let out = Git::new(&env.workdir)
                .run(["log", "-1", "--format=%s%x00%h", &format!("{cv}^"), "--"])
                .ok()?;
            let (subject, short) = out.trim_end().split_once('\0')?;
            Some((subject.to_owned(), short.to_owned()))
        });
        ListInfo {
            current_version: env.current_version.clone(),
            has_wc: env.has_wc,
            amend: env.amend,
            has_two_revisions: env.revisions.is_some(),
            initial_repo: state.head.is_none(),
            rebase_active: state.rebase_active,
            uses_lfs: state.uses_lfs,
            shift: self.shift.get(),
            parent_title,
            marked_label: self.marked_label(),
        }
    }

    fn marked_label(&self) -> Option<String> {
        if let Some(m) = self.marked.borrow().as_ref() {
            let short = |r: &str| r.chars().take(8).collect::<String>();
            return Some(match &m.rev {
                Some(rev) => format!("{}:{}", m.rel, short(rev)),
                None => m.rel.clone(),
            });
        }
        let stored = Settings::load_default().string(DIFF_LATER, "");
        (!stored.is_empty()).then_some(stored)
    }

    fn build(&self, row: usize) -> Vec<crate::menus::Entry> {
        let workdir = self.env.borrow().workdir.clone();
        let repo = Repo::discover(&workdir).ok();
        let items = (self.host.items)();
        let sel: Vec<RowInfo> = self
            .selection
            .borrow()
            .rows()
            .filter_map(|i| items.get(i))
            .map(|it| self.row_info(repo.as_ref(), it))
            .collect();
        let Some(focused) = items.get(row).map(|it| self.row_info(repo.as_ref(), it)) else {
            return Vec::new();
        };
        let mask = self.env.borrow().mask;
        fm::build(&sel, &focused, mask, &self.list_info(repo.as_ref()))
    }

    // ------------------------------------------------------------ actions

    fn run(&self, id: i32) {
        let Some(action) = A::from_id(id) else { return };
        let items = self.selected_items();
        if items.is_empty() {
            return;
        }
        if let Err(e) = self.dispatch(action, &items) {
            msgbox::ask_async("KoopaGit", &e.to_string(), &["OK"], |_| {});
        }
        self.cache.invalidate();
    }

    fn refresh_later(&self) -> impl Fn() + 'static {
        let me = self.me.borrow().clone();
        move || {
            if let Some(m) = me.upgrade() {
                (m.host.refresh)();
            }
        }
    }

    fn git(&self) -> Git {
        Git::new(&self.env.borrow().workdir)
    }

    fn full(&self, rel: &str) -> PathBuf {
        self.env.borrow().workdir.join(rel)
    }

    /// Runs git command lines in a progress window, refreshing the list after.
    fn progress(&self, title: &str, commands: Vec<Vec<String>>, post: Vec<PostAction>) {
        let job = ProgressJob {
            title: format!("{} - {title}", self.env.borrow().title),
            git: self.git(),
            commands,
            post_actions: post,
            close_on_end: CloseOnEnd::IfNoOptions,
        };
        let refresh = self.refresh_later();
        match crate::progress::start(job, move |_| refresh()) {
            Ok(d) => keep_alive(d),
            Err(e) => eprintln!("koopagit: {e}"),
        }
    }

    /// Asks a yes/no question without blocking; runs `then` on "Yes".
    fn confirm(&self, text: &str, then: impl FnOnce(&FileListMenu) + 'static) {
        let me = self.me.borrow().clone();
        msgbox::ask_async("KoopaGit", text, &["Yes", "No"], move |a| {
            if a == Some(0) {
                if let Some(m) = me.upgrade() {
                    then(&m);
                }
            }
        });
    }

    fn dispatch(&self, action: A, items: &[Item]) -> crate::DialogResult {
        let rels: Vec<String> = items.iter().map(|i| i.entry.path.clone()).collect();
        let fulls: Vec<PathBuf> = rels.iter().map(|r| self.full(r)).collect();
        let env = self.env.borrow().clone();
        let base = if env.amend { "HEAD~1" } else { "HEAD" };
        let first = &items[0];
        let first_full = fulls[0].clone();
        match action {
            A::EditConflict => {
                spawn_launcher(&Invocation::new("conflicteditor").with_paths([first_full]))
            }
            A::ResolveConflict | A::ResolveTheirs | A::ResolveMine => {
                let rels = rels.clone();
                self.confirm(
                    "Are you sure you want to mark the conflicted file(s) as resolved?",
                    move |m| {
                        let cmds = match action {
                            A::ResolveTheirs => listops::resolve_side(true, &rels),
                            A::ResolveMine => listops::resolve_side(false, &rels),
                            _ => vec![fileops::resolve(&rels)],
                        };
                        m.progress("Resolve", cmds, vec![]);
                    },
                );
            }
            A::Add | A::AddExe | A::AddLink => {
                let cmds = match action {
                    A::AddExe => vec![listops::add_executable(&rels)],
                    A::AddLink => listops::add_as_symlink(&self.git(), &rels)?,
                    _ => vec![fileops::add(&rels)],
                };
                let mut post = Vec::new();
                if env.mask.has(PopMask::COMMIT) {
                    post.push(PostAction::on_success(
                        "Commit...",
                        Invocation::new("commit").with_paths(fulls.clone()),
                    ));
                }
                self.progress("Add", cmds, post);
            }
            A::Commit => spawn_launcher(&Invocation::new("commit").with_paths(fulls)),
            A::Compare => {
                for full in &fulls {
                    let inv = Invocation::new("diff").with_paths([full.clone()]);
                    let inv = match &env.current_version {
                        None if env.amend => inv.with("startrev", "HEAD~1"),
                        None => inv,
                        Some(cv) => inv
                            .with("startrev", format!("{cv}~1"))
                            .with("endrev", cv.clone()),
                    };
                    spawn_launcher(&inv);
                }
            }
            A::GnuDiff1 => {
                for full in &fulls {
                    let inv = Invocation::new("diff")
                        .with_paths([full.clone()])
                        .with_switch("unified");
                    let inv = match &env.current_version {
                        None if env.amend => inv.with("startrev", "HEAD~1"),
                        None => inv,
                        Some(cv) => inv
                            .with("startrev", format!("{cv}~1"))
                            .with("endrev", cv.clone()),
                    };
                    spawn_launcher(&inv);
                }
            }
            A::CompareWc | A::CompareParentWc => {
                if let Some(cv) = &env.current_version {
                    let rev = if action == A::CompareParentWc {
                        format!("{cv}^")
                    } else {
                        cv.clone()
                    };
                    for full in &fulls {
                        spawn_launcher(
                            &Invocation::new("diff")
                                .with_paths([full.clone()])
                                .with("startrev", rev.clone()),
                        );
                    }
                }
            }
            A::CompareTwoRevisions | A::GnuDiff2Revisions => {
                if let Some((r1, r2)) = &env.revisions {
                    for full in &fulls {
                        let mut inv = Invocation::new("diff").with_paths([full.clone()]);
                        inv = if action == A::GnuDiff2Revisions {
                            inv.with("startrev", r2.clone())
                                .with("endrev", r1.clone())
                                .with_switch("unified")
                        } else {
                            inv.with("startrev", r1.clone()).with("endrev", r2.clone())
                        };
                        spawn_launcher(&inv);
                    }
                }
            }
            A::CompareTwoFiles => {
                if fulls.len() == 2 {
                    let mut inv = Invocation::new("diff")
                        .with("path", fulls[0].to_string_lossy())
                        .with("path2", fulls[1].to_string_lossy());
                    if let Some(cv) = &env.current_version {
                        inv = inv.with("startrev", cv.clone()).with("endrev", cv.clone());
                    }
                    spawn_launcher(&inv);
                }
            }
            A::Revert => {
                let entries: Vec<_> = items.iter().map(|i| i.entry.clone()).collect();
                let modified = items.iter().any(|i| {
                    !i.is_dir
                        && matches!(
                            i.entry.action(),
                            StatusAction::Modified | StatusAction::Renamed
                        )
                });
                let go = move |m: &FileListMenu| {
                    if Settings::load_default().bool("RevertWithRecycleBin", true) {
                        let backup: Vec<PathBuf> = entries
                            .iter()
                            .filter(|e| e.action() != StatusAction::Added)
                            .map(|e| m.full(&e.path))
                            .filter(|p| p.is_file())
                            .collect();
                        // Keep the files: trash copies, the checkout restores them.
                        trash_copies(&backup);
                    }
                    // Amending reverts to the amended commit's parent.
                    let cmds = if base == "HEAD" {
                        fileops::revert(&entries)
                    } else {
                        let (added, other): (Vec<_>, Vec<_>) = entries
                            .iter()
                            .partition(|e| e.action() == StatusAction::Added);
                        let added: Vec<String> = added.iter().map(|e| e.path.clone()).collect();
                        let other: Vec<String> = other.iter().map(|e| e.path.clone()).collect();
                        let mut c = Vec::new();
                        if !added.is_empty() {
                            c.push(listops::unstage_if_present(&added));
                        }
                        if !other.is_empty() {
                            c.push(listops::checkout_from(base, &other));
                        }
                        c
                    };
                    m.progress("Revert", cmds, vec![]);
                };
                if modified {
                    self.confirm(
                        &format!(
                            "Are you sure you want to revert {} item(s)? You will lose ALL changes since the last update!",
                            items.len()
                        ),
                        go,
                    );
                } else {
                    go(self);
                }
            }
            A::SkipWorktree | A::AssumeValid | A::UnsetIgnoreLocalChanges => {
                let rels = rels.clone();
                let text = match action {
                    A::SkipWorktree => {
                        "Do you really want to mark the selected files as \"skip-worktree\"? Local changes to them will be ignored."
                    }
                    A::AssumeValid => {
                        "Do you really want to mark the selected files as \"assume-unchanged\"? Local changes to them will be ignored."
                    }
                    _ => {
                        "Do you really want to remove the \"skip-worktree\" and \"assume-unchanged\" flags from the selected files?"
                    }
                };
                self.confirm(text, move |m| {
                    let cmds = match action {
                        A::SkipWorktree => vec![listops::set_index_flag(
                            IndexFlag::SkipWorktree,
                            true,
                            &rels,
                        )],
                        A::AssumeValid => vec![listops::set_index_flag(
                            IndexFlag::AssumeUnchanged,
                            true,
                            &rels,
                        )],
                        _ => listops::clear_index_flags(&rels),
                    };
                    let git = m.git();
                    for c in cmds {
                        if let Err(e) = git.run(c) {
                            msgbox::ask_async("KoopaGit", &e.to_string(), &["OK"], |_| {});
                            break;
                        }
                    }
                    (m.host.refresh)();
                });
            }
            A::CreateRestore => {
                let dir = std::env::temp_dir()
                    .join("koopagit")
                    .join(format!("restore-{}", std::process::id()));
                for (rel, full) in rels.iter().zip(&fulls) {
                    if !full.is_file() {
                        continue;
                    }
                    let backup = dir.join(rel);
                    if let Some(p) = backup.parent() {
                        std::fs::create_dir_all(p)?;
                    }
                    std::fs::copy(full, &backup)?;
                    self.backups.borrow_mut().insert(rel.clone(), backup);
                }
            }
            A::RestorePath => {
                let rels = rels.clone();
                self.confirm(
                    "Do you really want to restore the copy? All changes made since \"Restore after commit\" was selected will be lost.",
                    move |m| {
                        for rel in &rels {
                            let backup = m.backups.borrow_mut().remove(rel);
                            if let Some(b) = backup {
                                let _ = restore_backup(&b, &m.full(rel));
                            }
                        }
                        (m.host.refresh)();
                    },
                );
            }
            A::LfsLock | A::LfsUnlock => {
                let lock = action == A::LfsLock;
                self.progress(
                    if lock { "LFS Lock" } else { "LFS Unlock" },
                    listops::lfs_lock(lock, &rels),
                    vec![],
                );
            }
            A::RevertToRev | A::RevertToParent => {
                let Some(cv) = env.current_version.clone() else {
                    return Ok(());
                };
                let rev = if action == A::RevertToParent {
                    format!("{cv}~1")
                } else {
                    cv
                };
                if Settings::load_default().bool("RevertWithRecycleBin", true) {
                    trash_copies(
                        &fulls
                            .iter()
                            .filter(|p| p.is_file())
                            .cloned()
                            .collect::<Vec<_>>(),
                    );
                }
                let git = self.git();
                let mut done = 0;
                for it in items {
                    let res = if action == A::RevertToParent
                        && it.entry.action() == StatusAction::Added
                    {
                        let _ = std::fs::remove_file(self.full(&it.entry.path));
                        git.run(listops::unstage_if_present(std::slice::from_ref(
                            &it.entry.path,
                        )))
                    } else {
                        let path = if action == A::RevertToParent {
                            it.entry.old_path.clone().unwrap_or(it.entry.path.clone())
                        } else {
                            it.entry.path.clone()
                        };
                        git.run(listops::checkout_from(&rev, &[path]))
                    };
                    match res {
                        Ok(_) => done += 1,
                        Err(e) => msgbox::ask_async("KoopaGit", &e.to_string(), &["OK"], |_| {}),
                    }
                }
                msgbox::ask_async(
                    "KoopaGit",
                    &format!("{done} files reverted to {rev}."),
                    &["OK"],
                    |_| {},
                );
            }
            A::Log | A::LogSubmodule | A::LogOldName => {
                let path = match action {
                    A::LogOldName => match &first.entry.old_path {
                        Some(old) => self.full(old),
                        None => return Ok(()),
                    },
                    _ => first_full,
                };
                let mut inv = Invocation::new("log").with_paths([path]);
                if action == A::Log {
                    if first.is_dir {
                        inv = inv.with_switch("submodule");
                    }
                    if let Some(cv) = &env.current_version {
                        inv = inv.with("endrev", cv.clone()).with("rev", cv.clone());
                    }
                }
                spawn_launcher(&inv);
            }
            A::Blame => {
                let mut inv = Invocation::new("blame").with_paths([first_full]);
                if let Some(cv) = &env.current_version {
                    inv = inv.with("endrev", cv.clone());
                }
                spawn_launcher(&inv);
            }
            A::Export => {
                let Some(dir) = crate::pick::folder("Export selection to", Some(&env.workdir))
                else {
                    return Ok(());
                };
                for it in items.iter().filter(|i| !i.is_dir) {
                    let target = dir.join(&it.entry.path);
                    if let Some(p) = target.parent() {
                        std::fs::create_dir_all(p)?;
                    }
                    std::fs::write(&target, self.contents(&env, &it.entry.path)?)?;
                }
            }
            A::SaveAs => {
                let name = save_as_name(&first.entry.path, env.current_version.as_deref());
                let start = first_full.parent().map(Path::to_owned);
                if let Some(dest) =
                    crate::pick::save_file("Save revision to", start.as_deref(), &name)
                {
                    std::fs::write(dest, self.contents(&env, &first.entry.path)?)?;
                }
            }
            A::ViewRev | A::Open | A::OpenWith => {
                let file = match &env.current_version {
                    None => first_full,
                    Some(cv) => {
                        Repo::discover(&env.workdir)?.extract_blob(cv, &first.entry.path)?
                    }
                };
                if action == A::OpenWith {
                    open_with(&file);
                } else {
                    open_path(&file);
                }
            }
            A::Explore => explore_to(&first_full),
            A::PrepareDiff => {
                *self.marked.borrow_mut() = Some(Marked {
                    full: first_full.clone(),
                    rel: first.entry.path.clone(),
                    rev: env.current_version.clone(),
                });
                let mut s = Settings::load_default();
                s.set(DIFF_LATER, first_full.to_string_lossy());
                let _ = s.save();
            }
            A::PrepareDiffCompare => {
                let marked = self.marked.borrow_mut().take();
                let (left, right) = match marked {
                    Some(m) => {
                        let repo = Repo::discover(&env.workdir)?;
                        let left = match &m.rev {
                            Some(r) => repo.extract_blob(r, &m.rel)?,
                            None => m.full,
                        };
                        let right = match &env.current_version {
                            Some(cv) => repo.extract_blob(cv, &first.entry.path)?,
                            None => first_full,
                        };
                        (left, right)
                    }
                    None => {
                        let stored = Settings::load_default().string(DIFF_LATER, "");
                        if stored.is_empty() {
                            return Ok(());
                        }
                        (PathBuf::from(stored), first_full)
                    }
                };
                let mut s = Settings::load_default();
                s.remove(DIFF_LATER);
                let _ = s.save();
                spawn_launcher(
                    &Invocation::new("diff")
                        .with("path", left.to_string_lossy())
                        .with("path2", right.to_string_lossy()),
                );
            }
            A::Delete => self.delete_files(items, false)?,
            A::Ignore | A::IgnoreMask => {
                let mut inv = Invocation::new("ignore").with_paths(fulls);
                if action == A::IgnoreMask {
                    inv = inv.with_switch("onlymask");
                }
                spawn_launcher(&inv);
            }
            A::IgnoreFolder => {
                if let Some(dir) = first_full.parent() {
                    spawn_launcher(&Invocation::new("ignore").with_paths([dir.to_owned()]));
                }
            }
            A::CopyFull | A::CopyRelPaths | A::CopyFileNames | A::CopyExt => {
                let text = match action {
                    A::CopyFull => {
                        join_lines(fulls.iter().map(|p| p.to_string_lossy().into_owned()))
                    }
                    A::CopyRelPaths => join_lines(rels.iter().cloned()),
                    A::CopyFileNames => join_lines(
                        rels.iter()
                            .map(|r| r.rsplit('/').next().unwrap_or(r).to_owned()),
                    ),
                    _ => self.copy_all_information(),
                };
                if !crate::clipboard::copy(&text) {
                    msgbox::ask_async("Copy to clipboard", &text, &["OK"], |_| {});
                }
            }
            A::Changelists | A::RestoreBit => {}
        }
        Ok(())
    }

    /// `DeleteSelectedFiles`: to the trash, or removed for good when `permanent`
    /// (Shift+Delete).
    fn delete_files(&self, items: &[Item], permanent: bool) -> crate::DialogResult {
        let missing: Vec<String> = items
            .iter()
            .filter(|i| i.entry.action() == StatusAction::Missing)
            .map(|i| i.entry.path.clone())
            .collect();
        if !missing.is_empty() {
            self.git().run(listops::unstage_if_present(&missing))?;
        }
        let on_disk: Vec<PathBuf> = items
            .iter()
            .map(|i| self.full(&i.entry.path))
            .filter(|p| p.exists())
            .collect();
        let failed = if permanent {
            on_disk
                .into_iter()
                .filter(|p| {
                    if p.is_dir() {
                        std::fs::remove_dir_all(p).is_err()
                    } else {
                        std::fs::remove_file(p).is_err()
                    }
                })
                .collect()
        } else {
            crate::trashcan::move_to_trash(&on_disk)
        };
        if !failed.is_empty() {
            msgbox::ask_async(
                "KoopaGit",
                &format!(
                    "Could not {}:\n{}",
                    if permanent {
                        "delete"
                    } else {
                        "move to the trash"
                    },
                    failed
                        .iter()
                        .map(|p| p.to_string_lossy().into_owned())
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
                &["OK"],
                |_| {},
            );
        }
        (self.host.refresh)();
        Ok(())
    }

    /// The file at the shown revision (working tree or `git show rev:path`).
    fn contents(&self, env: &FileListEnv, rel: &str) -> crate::DialogResult<Vec<u8>> {
        Ok(match &env.current_version {
            None => std::fs::read(env.workdir.join(rel))?,
            Some(cv) => listops::blob_at(&Git::new(&env.workdir), cv, rel)?,
        })
    }

    /// `COPYEXT`: header line, then every visible column tab-separated (CRLF).
    fn copy_all_information(&self) -> String {
        let model = &self.host.model;
        let mut lines = vec!["Path\tExtension\tStatus\tLines added\tLines removed".to_owned()];
        for i in self.selection.borrow().rows() {
            if let Some(r) = model.row_data(i) {
                lines.push(format!(
                    "{}\t{}\t{}\t{}\t{}",
                    r.path, r.extension, r.status, r.added, r.removed
                ));
            }
        }
        lines.join("\r\n")
    }
}

/// Setting shared with the shell's "Diff later".
const DIFF_LATER: &str = "DiffLater";

fn join_lines(lines: impl Iterator<Item = String>) -> String {
    lines.collect::<Vec<_>>().join("\r\n")
}

/// `<base>-<shorthash><ext>` (or the file name for the working tree).
pub fn save_as_name(rel: &str, rev: Option<&str>) -> String {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    let Some(rev) = rev else {
        return name.to_owned();
    };
    let short: String = rev.chars().take(8).collect();
    match name.rfind('.') {
        Some(i) if i > 0 => format!("{}-{short}{}", &name[..i], &name[i..]),
        _ => format!("{name}-{short}"),
    }
}

/// `RevertWithRecycleBin`: puts copies of the files in the trash before they
/// are overwritten, so the old content can be recovered from there.
fn trash_copies(files: &[PathBuf]) {
    let dir = std::env::temp_dir().join("koopagit").join(format!(
        "revert-{}-{}",
        std::process::id(),
        crate::format::now()
    ));
    let mut copies = Vec::new();
    for f in files {
        let Some(name) = f.file_name() else { continue };
        let target = dir.join(copies.len().to_string()).join(name);
        if let Some(p) = target.parent() {
            if std::fs::create_dir_all(p).is_err() {
                continue;
            }
        }
        if std::fs::copy(f, &target).is_ok() {
            copies.push(target);
        }
    }
    let _ = crate::trashcan::move_to_trash(&copies);
    let _ = std::fs::remove_dir_all(&dir);
}

fn open_path(p: &Path) {
    let r = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(p).spawn()
    } else if cfg!(windows) {
        std::process::Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(p)
            .spawn()
    } else {
        std::process::Command::new("xdg-open").arg(p).spawn()
    };
    if let Err(e) = r {
        eprintln!("koopagit: open {}: {e}", p.display());
    }
}

fn explore_to(p: &Path) {
    let r = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg("-R").arg(p).spawn()
    } else if cfg!(windows) {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", p.display()))
            .spawn()
    } else {
        std::process::Command::new("xdg-open")
            .arg(p.parent().unwrap_or(p))
            .spawn()
    };
    if let Err(e) = r {
        eprintln!("koopagit: explore {}: {e}", p.display());
    }
}

/// "Open with...": the Windows "Open with" dialog; elsewhere an application picker.
fn open_with(p: &Path) {
    if cfg!(windows) {
        let _ = std::process::Command::new("rundll32")
            .arg("shell32.dll,OpenAs_RunDLL")
            .arg(p)
            .spawn();
        return;
    }
    let apps = if cfg!(target_os = "macos") {
        Some(PathBuf::from("/Applications"))
    } else {
        Some(PathBuf::from("/usr/bin"))
    };
    let Some(app) = crate::pick::open_file("Open with", apps.as_deref(), &[]) else {
        return;
    };
    let r = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
            .arg("-a")
            .arg(&app)
            .arg(p)
            .spawn()
    } else {
        std::process::Command::new(&app).arg(p).spawn()
    };
    if let Err(e) = r {
        eprintln!("koopagit: open with {}: {e}", app.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use koopagit_core::status::StatusEntry;

    #[test]
    fn save_as_names() {
        assert_eq!(
            save_as_name("src/a.rs", Some("0123456789abcdef")),
            "a-01234567.rs"
        );
        assert_eq!(
            save_as_name("Makefile", Some("abcdef0123")),
            "Makefile-abcdef01"
        );
        assert_eq!(save_as_name("dir/x.txt", None), "x.txt");
    }

    fn item(path: &str, a: StatusAction) -> Item {
        Item {
            entry: StatusEntry {
                path: path.into(),
                old_path: None,
                staged: None,
                unstaged: Some(a),
            },
            checked: false,
            is_dir: false,
        }
    }

    #[test]
    fn selection_drives_menu_and_model() {
        let dir = tempfile::tempdir().unwrap();
        let items = vec![
            item("a.txt", StatusAction::Modified),
            item("b.txt", StatusAction::Unversioned),
            item("c.txt", StatusAction::Modified),
        ];
        let model = Rc::new(VecModel::from(
            items
                .iter()
                .map(|i| crate::filelist::to_row(i, &HashMap::new()))
                .collect::<Vec<_>>(),
        ));
        let refreshed = Rc::new(Cell::new(0));
        let r2 = refreshed.clone();
        let it2 = items.clone();
        let menu = FileListMenu::new(
            FileListEnv::working_tree(0, dir.path(), PopMask::CHANGED_DLG, "Working Tree"),
            FileListHost {
                items: Box::new(move || it2.clone()),
                model: model.clone(),
                refresh: Box::new(move || r2.set(r2.get() + 1)),
            },
        );
        menu.click(0, Mods::default());
        menu.click(
            2,
            Mods {
                toggle: true,
                range: false,
            },
        );
        assert_eq!(menu.selected_rows(), [0, 2]);
        assert!(model.row_data(0).unwrap().selected);
        assert!(!model.row_data(1).unwrap().selected);
        // Right-click on an unselected row selects only it.
        let blocks = menu.menu(1);
        assert_eq!(menu.selected_rows(), [1]);
        assert!(blocks.row_count() > 0);
        // The unversioned row's menu starts with "Add".
        let first = blocks.row_data(0).unwrap().items.row_data(0).unwrap();
        assert_eq!(first.title.as_str(), "Add");
        menu.key(2, false);
        assert_eq!(menu.selected_rows(), [0, 1, 2]);
        assert_eq!(menu.copy_all_information().lines().count(), 4);
        // Shrinking the model clamps the selection.
        model.remove(2);
        menu.after_rebuild();
        assert_eq!(menu.selected_rows(), [0, 1]);
        assert_eq!(refreshed.get(), 0);
    }

    #[test]
    fn list_keys() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "a").unwrap();
        let items = vec![
            item("a.txt", StatusAction::Modified),
            item("b.txt", StatusAction::Unversioned),
            item("c.txt", StatusAction::Added),
        ];
        let model = Rc::new(VecModel::from(
            items
                .iter()
                .map(|i| crate::filelist::to_row(i, &HashMap::new()))
                .collect::<Vec<_>>(),
        ));
        let refreshed = Rc::new(Cell::new(0));
        let r2 = refreshed.clone();
        let it2 = items.clone();
        let menu = FileListMenu::new(
            FileListEnv::working_tree(0, dir.path(), PopMask::COMMIT_DLG, "Commit"),
            FileListHost {
                items: Box::new(move || it2.clone()),
                model: model.clone(),
                refresh: Box::new(move || r2.set(r2.get() + 1)),
            },
        );
        // Without hooks the dialog's own handling applies.
        assert!(!menu.activate());
        let activated = Rc::new(RefCell::new(Vec::new()));
        let toggled = Rc::new(RefCell::new(Vec::new()));
        {
            let a = activated.clone();
            menu.set_on_activate(move |r| a.borrow_mut().push(r));
            let t = toggled.clone();
            menu.set_on_toggle(move |r, c| t.borrow_mut().push((r, c)));
        }
        // Down from nothing focuses the first row; Shift+End extends to the last.
        assert_eq!(menu.key(1, false), 0);
        assert_eq!(menu.key(4, true), 2);
        assert_eq!(menu.selected_rows(), [0, 1, 2]);
        assert!(menu.activate());
        assert_eq!(*activated.borrow(), [0, 1, 2]);
        // Space on a selected row applies its new state to the whole selection.
        menu.space();
        assert_eq!(*toggled.borrow(), [(0, true), (1, true), (2, true)]);
        assert!((0..3).all(|i| model.row_data(i).unwrap().checked));
        assert_eq!(menu.copy_text(false), "a.txt\r\nb.txt\r\nc.txt");
        assert_eq!(
            menu.copy_text(true),
            "Path\tStatus\r\na.txt\tModified\r\nb.txt\tUnknown\r\nc.txt\tAdded"
        );
        // Delete needs the current row to be unversioned: c.txt (Added) blocks it.
        menu.delete_key(false);
        assert!(dir.path().join("a.txt").exists());
        assert_eq!(refreshed.get(), 0);
        // A list without "Delete" in its mask ignores the key altogether.
        menu.env.borrow_mut().mask = PopMask(PopMask::ALL.0 ^ PopMask::DELETE);
        assert_eq!(menu.key(0, false), 1);
        menu.delete_key(false);
        assert_eq!(refreshed.get(), 0);
        // With "Delete" allowed and b.txt current, the selection goes (b.txt is not on disk).
        menu.env.borrow_mut().mask = PopMask::COMMIT_DLG;
        menu.delete_key(false);
        assert_eq!(refreshed.get(), 1);
        // Shift+Delete removes for good (after its confirmation).
        std::fs::write(dir.path().join("b.txt"), "b").unwrap();
        menu.delete_files(&items[1..2], true).unwrap();
        assert!(!dir.path().join("b.txt").exists());
    }

    #[test]
    fn restore_all_copies_back_touches_and_drops_backups() {
        use std::time::{Duration, SystemTime};
        let work = tempfile::tempdir().unwrap();
        let store = tempfile::tempdir().unwrap();
        std::fs::create_dir(work.path().join("sub")).unwrap();
        std::fs::write(work.path().join("sub/a.txt"), "edited\n").unwrap();
        let backup = store.path().join("a.txt");
        std::fs::write(&backup, "original\n").unwrap();
        let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        std::fs::File::options()
            .write(true)
            .open(&backup)
            .unwrap()
            .set_modified(old)
            .unwrap();

        let backups = HashMap::from([("sub/a.txt".to_owned(), backup.clone())]);
        restore_all(work.path(), &backups);

        let dest = work.path().join("sub/a.txt");
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "original\n");
        assert!(std::fs::metadata(&dest).unwrap().modified().unwrap() > old);
        assert!(!backup.exists(), "backup is removed after restoring");
    }
}
