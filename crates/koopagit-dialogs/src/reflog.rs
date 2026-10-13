//! `/command:reflog [/ref:<ref>]` — `CRefLogDlg` ("RefLog"). The shell's
//! "Stash List" opens it with `/ref:refs/stash`.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use koopagit_core::Repo;
use koopagit_core::commands;
use koopagit_core::git_cli::Git;
use koopagit_core::history::ReflogEntry;
use koopagit_core::launcher::{CloseOnEnd, Invocation};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::menus::ActionId;
use crate::menus::reflog::{self as menu, ClipRow, ReflogAction as A, ReflogMenuInput};
use crate::progress::{PostAction, ProgressJob};
use crate::rowsel::ListSel;
use crate::{Context, DialogResult, RefLogDialog, ReflogRow, keep_alive, spawn_launcher};

pub const STASH_REF: &str = "refs/stash";

/// Display form of a reflog selector: `refs/stash@{1}` -> `stash@{1}`,
/// `refs/heads/main@{0}` -> `main@{0}`.
pub fn short_selector(selector: &str) -> String {
    selector
        .strip_prefix("refs/heads/")
        .or_else(|| selector.strip_prefix("refs/"))
        .unwrap_or(selector)
        .to_owned()
}

/// The refs offered in the "Ref:" box: HEAD, the stash (if any), local branches.
pub fn ref_choices(repo: &Repo) -> Vec<String> {
    let mut refs = vec!["HEAD".to_owned()];
    if repo.has_stash() {
        refs.push(STASH_REF.to_owned());
    }
    refs.extend(
        repo.local_branches()
            .unwrap_or_default()
            .into_iter()
            .map(|b| format!("refs/heads/{b}")),
    );
    refs
}

/// The reflog delete command for one selector (`stash drop` for stash entries).
pub fn delete_args(selector: &str) -> Vec<String> {
    let short = short_selector(selector);
    if short.starts_with("stash@{") || short.starts_with("stash{") {
        commands::stash_drop(&short)
    } else {
        vec![
            "reflog".into(),
            "delete".into(),
            "--".into(),
            selector.to_owned(),
        ]
    }
}

#[derive(Clone, PartialEq, Eq)]
enum Pending {
    None,
    Delete(Vec<usize>),
    Revert(Vec<usize>),
    ClearStash,
}

struct State {
    refs: Vec<String>,
    refname: String,
    entries: Vec<ReflogEntry>,
    head: Option<String>,
    current_branch: String,
    merge_active: bool,
    pending: Pending,
}

pub fn run(ctx: &mut Context) -> DialogResult<i32> {
    let dlg = RefLogDialog::new()?;
    let workdir = ctx.repo.workdir().to_owned();
    dlg.set_dialog_title(crate::window_title(&workdir, "RefLog").into());
    let mut refs = ref_choices(&ctx.repo);
    let wanted = ctx.invocation.get("ref").unwrap_or("HEAD").to_owned();
    let index = match refs.iter().position(|r| *r == wanted) {
        Some(i) => i,
        None => {
            refs.push(wanted);
            refs.len() - 1
        }
    };
    let labels: Vec<SharedString> = refs.iter().map(|r| r.as_str().into()).collect();
    dlg.set_refs(ModelRc::new(VecModel::from(labels)));
    dlg.set_ref_index(index as i32);

    let state = Rc::new(RefCell::new(State {
        refs,
        refname: String::new(),
        entries: Vec::new(),
        head: None,
        current_branch: String::new(),
        merge_active: false,
        pending: Pending::None,
    }));
    let sel = Rc::new(ListSel::default());
    let model = Rc::new(VecModel::<ReflogRow>::default());
    dlg.set_rows(ModelRc::from(model.clone()));

    let paint: Rc<dyn Fn()> = {
        let model = model.clone();
        let sel = sel.clone();
        Rc::new(move || {
            for i in 0..model.row_count() {
                if let Some(mut r) = model.row_data(i) {
                    let s = sel.is_selected(i);
                    if r.selected != s {
                        r.selected = s;
                        model.set_row_data(i, r);
                    }
                }
            }
        })
    };

    let reload: Rc<dyn Fn()> = {
        let state = state.clone();
        let weak = dlg.as_weak();
        let workdir = workdir.clone();
        let sel = sel.clone();
        let paint = paint.clone();
        let model = model.clone();
        Rc::new(move || {
            let Some(d) = weak.upgrade() else { return };
            let refname = state
                .borrow()
                .refs
                .get(d.get_ref_index() as usize)
                .cloned()
                .unwrap_or_else(|| "HEAD".to_owned());
            let repo = Repo::discover(&workdir).ok();
            let entries = repo
                .as_ref()
                .and_then(|r| r.reflog(&refname).ok())
                .unwrap_or_default();
            let rows: Vec<ReflogRow> = entries
                .iter()
                .map(|e| ReflogRow {
                    sha: e.id[..e.id.len().min(8)].to_string().into(),
                    selector: short_selector(&e.selector).into(),
                    action: e.action.clone().into(),
                    message: e.message.clone().into(),
                    date: crate::format::short_datetime(e.time, e.offset).into(),
                    selected: false,
                })
                .collect();
            model.set_vec(rows);
            d.set_is_stash(refname == STASH_REF);
            sel.reset(entries.len(), true);
            let mut st = state.borrow_mut();
            if let Some(repo) = &repo {
                let rs = repo.state();
                st.head = rs.head;
                st.current_branch = rs.branch.unwrap_or_default();
                st.merge_active = rs.merge_active;
            }
            st.entries = entries;
            st.refname = refname;
            drop(st);
            paint();
        })
    };
    reload();

    let actions = Rc::new(Actions {
        workdir: workdir.clone(),
        has_wc: true,
        state: state.clone(),
        sel: sel.clone(),
        weak: dlg.as_weak(),
        reload: reload.clone(),
    });
    {
        let reload = reload.clone();
        dlg.on_ref_changed(move || reload());
    }
    {
        let reload = reload.clone();
        dlg.on_refresh(move || reload());
    }
    {
        let (sel, paint) = (sel.clone(), paint.clone());
        dlg.on_row_clicked(move |row, toggle, range| {
            sel.click(row as usize, toggle, range);
            paint();
        });
    }
    {
        let (sel, paint) = (sel.clone(), paint.clone());
        dlg.on_row_right(move |row| {
            sel.right(row as usize);
            paint();
        });
    }
    {
        let (sel, paint, model) = (sel.clone(), paint.clone(), model.clone());
        dlg.on_key_step(move |delta, extend| {
            sel.key(delta, extend, model.row_count());
            paint();
        });
    }
    {
        let a = actions.clone();
        dlg.on_menu_for(move |row| {
            let a2 = a.clone();
            a.sel.cache.get(row, move || a2.menu(row as usize))
        });
    }
    {
        let a = actions.clone();
        dlg.on_menu_action(move |row, id| a.run(row as usize, id));
    }
    {
        let a = actions.clone();
        // The reflog's default (double-click) item is "Show log...".
        dlg.on_row_activated(move |row| a.run(row as usize, A::ShowLog.id()));
    }
    {
        let a = actions.clone();
        dlg.on_clear_stash(move || a.ask_clear());
    }
    {
        let a = actions.clone();
        dlg.on_notice_clicked(move |i| a.notice_clicked(i));
    }
    {
        let weak = dlg.as_weak();
        dlg.on_ok(move || {
            if let Some(d) = weak.upgrade() {
                let _ = d.hide();
            }
        });
    }
    crate::snapshot::arm(&dlg);
    dlg.run()?;
    Ok(0)
}

struct Actions {
    workdir: PathBuf,
    has_wc: bool,
    state: Rc<RefCell<State>>,
    sel: Rc<ListSel>,
    weak: slint::Weak<RefLogDialog>,
    reload: Rc<dyn Fn()>,
}

impl Actions {
    fn notice(&self, text: &str, buttons: &[&str], error: bool) {
        if let Some(d) = self.weak.upgrade() {
            d.set_notice_text(text.into());
            let labels: Vec<SharedString> = buttons.iter().map(|b| (*b).into()).collect();
            d.set_notice_buttons(ModelRc::new(VecModel::from(labels)));
            d.set_notice_error(error);
        }
    }

    fn launch(&self, command: &str) -> Invocation {
        Invocation::new(command).with_paths([self.workdir.clone()])
    }

    fn entries(&self, rows: &[usize]) -> Vec<ReflogEntry> {
        let st = self.state.borrow();
        rows.iter()
            .filter_map(|&r| st.entries.get(r).cloned())
            .collect()
    }

    fn menu(&self, row: usize) -> Vec<crate::menus::Entry> {
        let rows = self.sel.effective(row);
        let st = self.state.borrow();
        let first = rows.first().and_then(|&r| st.entries.get(r));
        let last = rows.last().and_then(|&r| st.entries.get(r));
        let short = |e: Option<&ReflogEntry>| {
            e.map(|e| e.id[..e.id.len().min(8)].to_owned())
                .unwrap_or_default()
        };
        menu::build(&ReflogMenuInput {
            count: rows.len(),
            is_stash: st.refname == STASH_REF,
            is_head: first.is_some_and(|e| st.head.as_deref() == Some(e.id.as_str())),
            merge_active: st.merge_active,
            has_wc: self.has_wc,
            current_branch: st.current_branch.clone(),
            contiguous: ListSel::contiguous(&rows),
            first_short: short(first),
            last_short: short(last),
        })
    }

    fn git_job(&self, title: &str, commands: Vec<Vec<String>>, post: Vec<PostAction>) {
        let job = ProgressJob {
            title: crate::window_title(&self.workdir, title),
            git: Git::new(&self.workdir),
            commands,
            post_actions: post,
            close_on_end: CloseOnEnd::Manual,
        };
        let reload = self.reload.clone();
        match crate::progress::start(job, move |_| reload()) {
            Ok(p) => keep_alive(p),
            Err(e) => self.notice(&e.to_string(), &[], true),
        }
    }

    fn copy(&self, text: &str) {
        if !crate::clipboard::copy(text) {
            self.notice(text, &["OK"], false);
        }
    }

    fn run(&self, row: usize, id: i32) {
        let Some(action) = A::from_id(id) else { return };
        // The menu acts on the rows that were selected when it opened.
        self.sel.right(row);
        let rows = self.sel.effective(row);
        let sel = self.entries(&rows);
        let Some(e) = sel.first().cloned() else {
            return;
        };
        let last = sel.last().cloned().unwrap_or_else(|| e.clone());
        let id = e.id.clone();
        match action {
            A::CompareWithWorkingTree => spawn_launcher(
                &self
                    .launch("showcompare")
                    .with("revision1", id)
                    .with("revision2", ""),
            ),
            A::UnifiedDiff => spawn_launcher(
                &self
                    .launch("showcompare")
                    .with("revision1", format!("{id}^"))
                    .with("revision2", id)
                    .with_switch("unified"),
            ),
            A::CompareWithPrevious => spawn_launcher(
                &self
                    .launch("showcompare")
                    .with("revision1", format!("{id}^"))
                    .with("revision2", id),
            ),
            A::ShowLog => spawn_launcher(
                &self
                    .launch("log")
                    .with("endrev", id.clone())
                    .with("rev", id),
            ),
            A::BrowseRepository => spawn_launcher(&self.launch("repobrowser").with("rev", id)),
            A::Merge => spawn_launcher(&self.launch("merge").with("rev", id)),
            A::Reset => {
                let reload = self.reload.clone();
                if let Err(err) = crate::reset::show(self.workdir.clone(), &id, move |_| reload()) {
                    self.notice(&err.to_string(), &[], true);
                }
            }
            A::SwitchToRev => spawn_launcher(&self.launch("switch").with("rev", id)),
            A::CreateBranch => spawn_launcher(&self.launch("branch").with("rev", id)),
            A::CreateTag => spawn_launcher(&self.launch("tag").with("rev", id)),
            A::RebaseOnto => spawn_launcher(&self.launch("rebase").with("upstream", id)),
            A::Export => spawn_launcher(&self.launch("export").with("rev", id)),
            A::Revert => {
                self.state.borrow_mut().pending = Pending::Revert(rows.clone());
                let text = if rows.len() == 1 {
                    "Do you really want to revert the changes of the selected commit?".to_owned()
                } else {
                    format!(
                        "Do you really want to revert the changes of the {} selected commits?",
                        rows.len()
                    )
                };
                self.notice(&text, &["Revert", "Abort"], false);
            }
            A::EditNotes => self.edit_notes(&id),
            A::DeleteRef => {
                // Re-read the reflog: refuse to delete from a stale view.
                let refname = self.state.borrow().refname.clone();
                let fresh = Repo::discover(&self.workdir)
                    .and_then(|r| r.reflog(&refname))
                    .unwrap_or_default();
                let stale = {
                    let st = self.state.borrow();
                    fresh.len() != st.entries.len()
                        || fresh.first().map(|e| &e.id) != st.entries.first().map(|e| &e.id)
                        || fresh.last().map(|e| &e.id) != st.entries.last().map(|e| &e.id)
                };
                if stale {
                    self.notice(
                        "The current view seems to be out of date. Please refresh, e.g. by pressing F5 and recheck the selection.",
                        &[],
                        true,
                    );
                    return;
                }
                let text = if rows.len() == 1 {
                    format!(
                        "\"{}\" will be permanently deleted. It can NOT be recovered!\n\nDo you really want to continue?",
                        short_selector(&e.selector)
                    )
                } else {
                    format!(
                        "Do you really want to permanently delete the {} selected refs? It can NOT be recovered!",
                        rows.len()
                    )
                };
                self.state.borrow_mut().pending = Pending::Delete(rows.clone());
                self.notice(&text, &["Delete", "Abort"], false);
            }
            A::StashApply => self.git_job(
                "Stash Apply",
                vec![commands::stash_apply(&short_selector(&e.selector))],
                Vec::new(),
            ),
            A::CompareRevisions => spawn_launcher(
                &self
                    .launch("showcompare")
                    .with("revision1", last.id.clone())
                    .with("revision2", id),
            ),
            A::UnifiedDiff2 => spawn_launcher(
                &self
                    .launch("showcompare")
                    .with("revision1", last.id.clone())
                    .with("revision2", id)
                    .with_switch("unified"),
            ),
            A::LogRange => spawn_launcher(
                &self
                    .launch("log")
                    .with("range", format!("{}..{id}", last.id)),
            ),
            A::LogRangeReverse => spawn_launcher(
                &self
                    .launch("log")
                    .with("range", format!("{id}..{}", last.id)),
            ),
            A::LogRangeReachableFromOnlyOne => spawn_launcher(
                &self
                    .launch("log")
                    .with("range", format!("{}...{id}", last.id)),
            ),
            A::CherryPick => {
                // Oldest first: the reflog lists newest first.
                let revs: Vec<String> = sel.iter().rev().map(|e| e.id.clone()).collect();
                spawn_launcher(
                    &self
                        .launch("rebase")
                        .with_switch("cherrypick")
                        .with("revs", revs.join(",")),
                );
            }
            A::FormatPatch => {
                let inv = if sel.len() > 1 {
                    self.launch("formatpatch")
                        .with("startrev", format!("{}~1", last.id))
                        .with("endrev", id)
                } else {
                    self.launch("formatpatch").with("startrev", id)
                };
                spawn_launcher(&inv);
            }
            A::CopyFull | A::CopySha | A::CopyMessages => {
                let dates: Vec<String> = sel
                    .iter()
                    .map(|e| crate::format::short_datetime(e.time, e.offset))
                    .collect();
                let clip: Vec<ClipRow<'_>> = sel
                    .iter()
                    .zip(&dates)
                    .map(|(e, d)| ClipRow {
                        hash: &e.id,
                        date: d,
                        action: &e.action,
                        message: &e.message,
                    })
                    .collect();
                let text = match action {
                    A::CopyFull => menu::copy_full(&clip),
                    A::CopySha => menu::copy_sha(&clip),
                    _ => menu::copy_messages(&clip),
                };
                self.copy(&text);
            }
            A::ShowBranches => spawn_launcher(&self.launch("commitisonrefs").with("rev", id)),
        }
    }

    fn edit_notes(&self, id: &str) {
        let current = Git::new(&self.workdir)
            .run(["notes", "show", id])
            .unwrap_or_default();
        let workdir = self.workdir.clone();
        let id = id.to_owned();
        let reload = self.reload.clone();
        crate::prompt::text(
            "Edit Notes",
            "Notes:",
            current.trim_end(),
            move |text: &str| {
                let args = if text.trim().is_empty() {
                    vec![
                        "notes".into(),
                        "remove".into(),
                        "--ignore-missing".into(),
                        id.clone(),
                    ]
                } else {
                    vec![
                        "notes".into(),
                        "add".into(),
                        "-f".into(),
                        "-m".into(),
                        text.to_owned(),
                        id.clone(),
                    ]
                };
                let out = Git::new(&workdir).output(args);
                reload();
                match out {
                    Ok(o) if o.success() => None,
                    Ok(o) => Some(o.stderr),
                    Err(e) => Some(e.to_string()),
                }
            },
        );
    }

    fn ask_clear(&self) {
        let n = self.state.borrow().entries.len();
        self.state.borrow_mut().pending = Pending::ClearStash;
        self.notice(
            &format!("Do you really want to delete ALL {n} stash?"),
            &["Yes", "No"],
            false,
        );
    }

    fn notice_clicked(&self, i: i32) {
        let pending = std::mem::replace(&mut self.state.borrow_mut().pending, Pending::None);
        self.notice("", &[], false);
        if i != 0 {
            return;
        }
        match pending {
            Pending::ClearStash => self.git_job(
                "Clear stash",
                vec![vec!["stash".into(), "clear".into()]],
                Vec::new(),
            ),
            Pending::Delete(rows) => {
                // Last to first, so stash indices stay valid.
                let cmds = self
                    .entries(&rows)
                    .iter()
                    .rev()
                    .map(|e| delete_args(&e.selector))
                    .collect();
                self.git_job("Delete", cmds, Vec::new());
            }
            Pending::Revert(rows) => {
                let git = Git::new(&self.workdir);
                let cmds = self
                    .entries(&rows)
                    .iter()
                    .map(|e| {
                        let merge = git
                            .run(["rev-list", "--parents", "-n", "1", &e.id])
                            .map(|o| o.split_whitespace().count() > 2)
                            .unwrap_or(false);
                        let mut c = vec!["revert".into(), "--no-edit".into(), "--no-commit".into()];
                        if merge {
                            c.extend(["-m".into(), "1".into()]);
                        }
                        c.push(e.id.clone());
                        c
                    })
                    .collect();
                let post = vec![PostAction::on_success("Commit", self.launch("commit"))];
                self.git_job("Revert", cmds, post);
            }
            Pending::None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selectors() {
        assert_eq!(short_selector("refs/stash@{1}"), "stash@{1}");
        assert_eq!(short_selector("refs/heads/main@{0}"), "main@{0}");
        assert_eq!(short_selector("HEAD@{2}"), "HEAD@{2}");
        assert_eq!(
            delete_args("refs/stash@{1}").join(" "),
            "stash drop -- stash@{1}"
        );
        assert_eq!(
            delete_args("refs/heads/main@{0}").join(" "),
            "reflog delete -- refs/heads/main@{0}"
        );
    }
}
