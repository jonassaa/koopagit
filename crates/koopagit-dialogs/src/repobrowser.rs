//! `/command:repobrowser [/rev:<ref>]` — `CRepositoryBrowser`: browse the tree
//! of a revision.

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;

use koopagit_core::Repo;
use koopagit_core::extras::{EntryKind, TreeEntry, children, format_size};
use koopagit_core::launcher::Invocation;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::diffcmd::{rev_title, start_diff, wt_title};
use crate::menus::repobrowser::{MenuState, RepoBrowserAction, SelectionType, menu};
use crate::menus::{ActionId, MenuCache};
use crate::selection::{Mods, Selection};
use crate::{Context, DialogResult, RepoBrowserDialog, RepoEntryRow, TreeNodeRow, spawn_launcher};

/// A node of the folder tree (`""` is the root, shown as `/`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub path: String,
    pub depth: usize,
    pub has_children: bool,
}

/// Folders visible in the tree: the root, then children of expanded folders.
pub fn visible_nodes(entries: &[TreeEntry], expanded: &BTreeSet<String>) -> Vec<Node> {
    fn walk(
        entries: &[TreeEntry],
        expanded: &BTreeSet<String>,
        dir: &str,
        depth: usize,
        out: &mut Vec<Node>,
    ) {
        for e in children(entries, dir) {
            if e.kind != EntryKind::Folder {
                continue;
            }
            let has_children = children(entries, &e.path)
                .iter()
                .any(|c| c.kind == EntryKind::Folder);
            out.push(Node {
                path: e.path.clone(),
                depth,
                has_children,
            });
            if expanded.contains(&e.path) {
                walk(entries, expanded, &e.path, depth + 1, out);
            }
        }
    }
    let mut out = vec![Node {
        path: String::new(),
        depth: 0,
        has_children: entries
            .iter()
            .any(|e| e.kind == EntryKind::Folder && !e.path.contains('/')),
    }];
    if expanded.contains("") {
        walk(entries, expanded, "", 1, &mut out);
    }
    out
}

/// The info label (`UpdateInfoLabel`).
pub fn info_text(dir: &str, items: &[&TreeEntry], selected: &[&TreeEntry]) -> String {
    match selected {
        [] => {
            let files = items.iter().filter(|e| e.kind == EntryKind::File).count();
            let subs = items
                .iter()
                .filter(|e| e.kind == EntryKind::Submodule)
                .count();
            let name = if dir.is_empty() {
                "/"
            } else {
                dir.rsplit('/').next().unwrap_or(dir)
            };
            format!(
                "{name}\nShowing {files} files, {subs} submodules and {} folders, {} items in total",
                items.len() - files - subs,
                items.len()
            )
        }
        [one] => match one.kind {
            EntryKind::Submodule => format!("Submodule \"{}\"\nRevision {}", one.name, one.oid),
            EntryKind::Folder => one.name.clone(),
            EntryKind::File => format!(
                "{}\nSize {}",
                one.name,
                one.size.map(format_size).unwrap_or_default()
            ),
        },
        many => format!("{} items selected", many.len()),
    }
}

fn entry_row(e: &TreeEntry, selected: bool) -> RepoEntryRow {
    let file = e.kind == EntryKind::File;
    RepoEntryRow {
        name: e.name.clone().into(),
        extension: if file {
            crate::filelist::extension(&e.name).into()
        } else {
            SharedString::new()
        },
        size: if file {
            e.size.map(format_size).unwrap_or_default().into()
        } else {
            SharedString::new()
        },
        kind: match e.kind {
            EntryKind::File => 0,
            EntryKind::Folder => 1,
            EntryKind::Submodule => 2,
        },
        selected,
    }
}

struct State {
    rev: String,
    entries: Vec<TreeEntry>,
    expanded: BTreeSet<String>,
    nodes: Vec<Node>,
    dir: String,
    /// Indexes into `entries` of the current folder's children, as listed.
    items: Vec<usize>,
    selection: Selection,
    marked: Option<(String, String)>,
}

impl State {
    fn listed(&self) -> Vec<&TreeEntry> {
        self.items.iter().map(|&i| &self.entries[i]).collect()
    }

    fn selected(&self) -> Vec<&TreeEntry> {
        self.selection
            .rows()
            .filter_map(|r| self.items.get(r).map(|&i| &self.entries[i]))
            .collect()
    }

    fn enter(&mut self, dir: &str) {
        self.dir = dir.to_owned();
        // Expand the path to the folder, like selecting it in the tree.
        let mut p = String::new();
        self.expanded.insert(String::new());
        for part in dir.split('/').filter(|p| !p.is_empty()) {
            if !p.is_empty() {
                p.push('/');
            }
            p.push_str(part);
            self.expanded.insert(p.clone());
        }
        let wanted: Vec<String> = children(&self.entries, dir)
            .iter()
            .map(|e| e.path.clone())
            .collect();
        self.items = wanted
            .iter()
            .filter_map(|p| self.entries.iter().position(|e| &e.path == p))
            .collect();
        self.selection = Selection::default();
        self.nodes = visible_nodes(&self.entries, &self.expanded);
    }
}

/// Pending question in the notice bar.
#[derive(Clone, PartialEq, Eq)]
enum Ask {
    None,
    SubmoduleUpdate(String),
}

pub fn run(ctx: &mut Context) -> DialogResult<i32> {
    let repo = Rc::new(Repo::discover(ctx.repo.workdir())?);
    let settings = Rc::new(ctx.settings.clone());
    let rev = ctx
        .invocation
        .get("rev")
        .filter(|r| !r.is_empty())
        .unwrap_or("HEAD")
        .to_owned();
    let dlg = RepoBrowserDialog::new()?;
    crate::window::apply_theme(&dlg);
    dlg.set_dialog_title(ctx.window_title("Repository Browser").into());
    dlg.set_path_text(repo.workdir().to_string_lossy().as_ref().into());
    dlg.set_revision(rev.clone().into());
    let mut refs = vec!["HEAD".to_owned()];
    refs.extend(repo.local_branches().unwrap_or_default());
    refs.extend(
        repo.remote_branches()
            .unwrap_or_default()
            .into_iter()
            .map(|b| format!("remotes/{b}")),
    );
    refs.extend(repo.tags().unwrap_or_default());
    dlg.set_refs(crate::remote_util::strings(&refs));

    let state = Rc::new(RefCell::new(State {
        rev,
        entries: Vec::new(),
        expanded: BTreeSet::new(),
        nodes: Vec::new(),
        dir: String::new(),
        items: Vec::new(),
        selection: Selection::default(),
        marked: None,
    }));
    let entry_model = Rc::new(VecModel::<RepoEntryRow>::default());
    let tree_model = Rc::new(VecModel::<TreeNodeRow>::default());
    dlg.set_entries(ModelRc::from(entry_model.clone()));
    dlg.set_tree(ModelRc::from(tree_model.clone()));
    let cache = Rc::new(MenuCache::default());
    let tree_cache = Rc::new(MenuCache::default());
    let ask = Rc::new(RefCell::new(Ask::None));
    // Which tree node the tree menu was opened on.
    let tree_menu_node = Rc::new(Cell::new(usize::MAX));

    let render = {
        let state = state.clone();
        let weak = dlg.as_weak();
        let entry_model = entry_model.clone();
        let tree_model = tree_model.clone();
        let cache = cache.clone();
        let tree_cache = tree_cache.clone();
        Rc::new(move || {
            let Some(d) = weak.upgrade() else { return };
            let st = state.borrow();
            tree_model.set_vec(
                st.nodes
                    .iter()
                    .map(|n| TreeNodeRow {
                        name: if n.path.is_empty() {
                            "/".into()
                        } else {
                            n.path.rsplit('/').next().unwrap_or(&n.path).into()
                        },
                        depth: n.depth as i32,
                        expanded: st.expanded.contains(&n.path),
                        has_children: n.has_children,
                        selected: n.path == st.dir,
                    })
                    .collect::<Vec<_>>(),
            );
            entry_model.set_vec(
                st.items
                    .iter()
                    .enumerate()
                    .map(|(r, &i)| entry_row(&st.entries[i], st.selection.contains(r)))
                    .collect::<Vec<_>>(),
            );
            d.set_info(info_text(&st.dir, &st.listed(), &st.selected()).into());
            cache.invalidate();
            tree_cache.invalidate();
        })
    };
    let render_selection = {
        let state = state.clone();
        let weak = dlg.as_weak();
        let entry_model = entry_model.clone();
        let cache = cache.clone();
        Rc::new(move || {
            let Some(d) = weak.upgrade() else { return };
            let st = state.borrow();
            for r in 0..entry_model.row_count() {
                if let Some(mut row) = entry_model.row_data(r) {
                    let sel = st.selection.contains(r);
                    if row.selected != sel {
                        row.selected = sel;
                        entry_model.set_row_data(r, row);
                    }
                }
            }
            d.set_info(info_text(&st.dir, &st.listed(), &st.selected()).into());
            cache.invalidate();
        })
    };
    let load = {
        let state = state.clone();
        let repo = repo.clone();
        let render = render.clone();
        let weak = dlg.as_weak();
        Rc::new(move |rev: &str| {
            let Some(d) = weak.upgrade() else { return };
            match repo.tree_entries(rev) {
                Ok(entries) => {
                    let mut st = state.borrow_mut();
                    st.rev = rev.to_owned();
                    st.entries = entries;
                    st.expanded = BTreeSet::from([String::new()]);
                    st.enter("");
                    d.set_notice_text(SharedString::new());
                }
                Err(e) => {
                    notice(&d, &format!("\"{rev}\" is invalid.\n{e}"), &[], true);
                }
            }
            render();
        })
    };
    let initial = state.borrow().rev.clone();
    load(&initial);

    {
        let weak = dlg.as_weak();
        let load = load.clone();
        dlg.on_revision_changed(move || {
            if let Some(d) = weak.upgrade() {
                let rev = d.get_revision().trim().to_owned();
                if !rev.is_empty() {
                    load(&rev);
                }
            }
        });
    }
    {
        let state = state.clone();
        let render = render.clone();
        dlg.on_tree_clicked(move |i| {
            let path = state.borrow().nodes.get(i as usize).map(|n| n.path.clone());
            if let Some(p) = path {
                state.borrow_mut().enter(&p);
                render();
            }
        });
    }
    {
        let state = state.clone();
        let render = render.clone();
        dlg.on_tree_toggled(move |i| {
            let mut st = state.borrow_mut();
            let Some(n) = st.nodes.get(i as usize).cloned() else {
                return;
            };
            if !st.expanded.remove(&n.path) {
                st.expanded.insert(n.path.clone());
            }
            st.nodes = visible_nodes(&st.entries, &st.expanded);
            drop(st);
            render();
        });
    }
    {
        let state = state.clone();
        let render_selection = render_selection.clone();
        dlg.on_entry_clicked(move |i, toggle, range| {
            state
                .borrow_mut()
                .selection
                .click(i as usize, Mods { toggle, range });
            render_selection();
        });
    }

    // One action handler for list and tree menus.
    let perform = {
        let state = state.clone();
        let repo = repo.clone();
        let settings = settings.clone();
        let weak = dlg.as_weak();
        let render = render.clone();
        let ask = ask.clone();
        Rc::new(move |action: RepoBrowserAction, targets: Vec<TreeEntry>| {
            let Some(d) = weak.upgrade() else { return };
            let rev = state.borrow().rev.clone();
            let abs = |e: &TreeEntry| repo.workdir().join(&e.path);
            let first = targets.first().cloned();
            match action {
                RepoBrowserAction::Open
                | RepoBrowserAction::OpenWith
                | RepoBrowserAction::OpenWithAlternativeEditor => {
                    let Some(e) = first else { return };
                    match e.kind {
                        EntryKind::Folder => {
                            state.borrow_mut().enter(&e.path);
                            render();
                        }
                        EntryKind::Submodule => {
                            let sub = abs(&e);
                            let has = Repo::discover(&sub)
                                .ok()
                                .and_then(|r| r.resolve_commit(&e.oid))
                                .is_some();
                            if has {
                                spawn_launcher(
                                    &Invocation::new("repobrowser")
                                        .with_paths([sub])
                                        .with("rev", e.oid.clone()),
                                );
                            } else {
                                *ask.borrow_mut() = Ask::SubmoduleUpdate(e.path.clone());
                                notice(
                                    &d,
                                    &format!(
                                        "Commit {} not found in \"{}\". Update submodule now?",
                                        e.oid, e.path
                                    ),
                                    &["Yes", "No"],
                                    false,
                                );
                            }
                        }
                        EntryKind::File => match repo.extract_blob(&rev, &e.path) {
                            Ok(file) => {
                                // `CRepositoryBrowser::OpenFile` (RepositoryBrowser.cpp:1304).
                                let res = if action == RepoBrowserAction::OpenWithAlternativeEditor
                                {
                                    crate::editor::launch(&file)
                                } else {
                                    crate::osopen::open(&file)
                                };
                                if let Err(err) = res {
                                    notice(&d, &err.to_string(), &[], true);
                                }
                            }
                            Err(err) => notice(&d, &err.to_string(), &[], true),
                        },
                    }
                }
                RepoBrowserAction::CompareWorkingTree => {
                    let Some(e) = first else { return };
                    match repo.extract_blob(&rev, &e.path) {
                        Ok(left) => start_diff(
                            &settings,
                            &left,
                            &abs(&e),
                            &rev_title(&repo, &e.path, &rev),
                            &wt_title(&e.path),
                            false,
                            None,
                        ),
                        Err(err) => notice(&d, &err.to_string(), &[], true),
                    }
                }
                RepoBrowserAction::ShowLog => {
                    let Some(e) = first else { return };
                    let mut inv = Invocation::new("log")
                        .with_paths([abs(&e)])
                        .with("endrev", rev.clone())
                        .with("rev", rev.clone());
                    if e.kind == EntryKind::Submodule {
                        inv = inv.with_switch("submodule");
                    }
                    spawn_launcher(&inv);
                }
                RepoBrowserAction::ShowLogSubmodule => {
                    let Some(e) = first else { return };
                    spawn_launcher(
                        &Invocation::new("log")
                            .with_paths([abs(&e)])
                            .with("rev", e.oid.clone())
                            .with("endrev", e.oid.clone()),
                    );
                }
                RepoBrowserAction::Blame => {
                    let Some(e) = first else { return };
                    spawn_launcher(
                        &Invocation::new("blame")
                            .with_paths([abs(&e)])
                            .with("endrev", rev.clone()),
                    );
                }
                RepoBrowserAction::SaveAs => {
                    let Some(e) = first else { return };
                    if let Some(dest) = crate::pick::save_file("Save revision to", None, &e.name) {
                        let res = repo
                            .git()
                            .command(["show", &format!("{rev}:{}", e.path)])
                            .output()
                            .map_err(|e| e.to_string())
                            .and_then(|o| {
                                if o.status.success() {
                                    std::fs::write(&dest, o.stdout).map_err(|e| e.to_string())
                                } else {
                                    Err(format!(
                                        "Failed to checkout file \"{}\" of revision {rev} to \"{}\".\n{}",
                                        e.path,
                                        dest.display(),
                                        String::from_utf8_lossy(&o.stderr)
                                    ))
                                }
                            });
                        if let Err(err) = res {
                            notice(&d, &err, &[], true);
                        }
                    }
                }
                RepoBrowserAction::Revert => {
                    let mut count = 0;
                    for e in &targets {
                        match repo.git().output([
                            "checkout",
                            "--end-of-options",
                            &rev,
                            "--",
                            &e.path,
                        ]) {
                            Ok(o) if o.success() => count += 1,
                            Ok(o) => {
                                notice(&d, &o.stderr, &[], true);
                                return;
                            }
                            Err(err) => {
                                notice(&d, &err.to_string(), &[], true);
                                return;
                            }
                        }
                    }
                    notice(&d, &format!("{count} files reverted to {rev}."), &[], false);
                }
                RepoBrowserAction::MarkForComparison => {
                    let Some(e) = first else { return };
                    match repo.resolve_commit(&rev) {
                        Some(hash) => state.borrow_mut().marked = Some((e.path.clone(), hash)),
                        None => notice(
                            &d,
                            &format!("Could not get SHA-1 for \"{rev}^{{}}\"."),
                            &[],
                            true,
                        ),
                    }
                }
                RepoBrowserAction::DiffWithMarked => {
                    let Some(e) = first else { return };
                    let Some((mpath, mrev)) = state.borrow().marked.clone() else {
                        return;
                    };
                    let base = repo.extract_blob(&mrev, &mpath);
                    let mine = repo.extract_blob(&rev, &e.path);
                    match (base, mine) {
                        (Ok(b), Ok(m)) => start_diff(
                            &settings,
                            &b,
                            &m,
                            &rev_title(&repo, &mpath, &mrev),
                            &rev_title(&repo, &e.path, &rev),
                            false,
                            None,
                        ),
                        (Err(err), _) | (_, Err(err)) => notice(&d, &err.to_string(), &[], true),
                    }
                }
                RepoBrowserAction::CopyPaths | RepoBrowserAction::CopyHash => {
                    let text: String = targets
                        .iter()
                        .map(|e| {
                            let v = if action == RepoBrowserAction::CopyPaths {
                                &e.name
                            } else {
                                &e.oid
                            };
                            format!("{v}\r\n")
                        })
                        .collect();
                    if !crate::clipboard::copy(&text) {
                        notice(&d, &text, &[], false);
                    }
                }
            }
        })
    };

    {
        let state = state.clone();
        let perform = perform.clone();
        dlg.on_entry_activated(move |i| {
            let e = {
                let st = state.borrow();
                st.items.get(i as usize).map(|&x| st.entries[x].clone())
            };
            if let Some(e) = e {
                perform(RepoBrowserAction::Open, vec![e]);
            }
        });
    }
    {
        let state = state.clone();
        let render_selection = render_selection.clone();
        dlg.on_entry_key(move |delta, extend| {
            {
                let mut st = state.borrow_mut();
                let len = st.items.len();
                if delta == 0 {
                    st.selection.select_all(len);
                } else {
                    st.selection.step(delta as isize, len, extend);
                }
            }
            render_selection();
        });
    }
    {
        // Enter in the file list (`OnOK`): folders are entered, files opened,
        // only with exactly one item selected.
        let state = state.clone();
        let perform = perform.clone();
        dlg.on_entry_enter(move || {
            let targets: Vec<TreeEntry> = state.borrow().selected().into_iter().cloned().collect();
            if targets.len() == 1 {
                perform(RepoBrowserAction::Open, targets);
            }
        });
    }
    {
        // F5: re-read the revision and return to the folder that was shown.
        let state = state.clone();
        let load = load.clone();
        let render = render.clone();
        dlg.on_refresh(move || {
            let (rev, dir) = {
                let st = state.borrow();
                (st.rev.clone(), st.dir.clone())
            };
            load(&rev);
            let mut st = state.borrow_mut();
            if st
                .entries
                .iter()
                .any(|e| e.path == dir && e.kind == EntryKind::Folder)
            {
                st.enter(&dir);
                drop(st);
                render();
            }
        });
    }
    {
        let state = state.clone();
        let cache = cache.clone();
        let render_selection = render_selection.clone();
        let repo = repo.clone();
        dlg.on_menu_for(move |row| {
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
                let sel = st.selected();
                let kinds: Vec<(bool, bool)> = sel
                    .iter()
                    .map(|e| (e.kind == EntryKind::Folder, e.kind == EntryKind::Submodule))
                    .collect();
                menu(
                    SelectionType::of(&kinds),
                    &MenuState {
                        count: sel.len(),
                        first_is_submodule: sel
                            .first()
                            .is_some_and(|e| e.kind == EntryKind::Submodule),
                        has_wc: !repo.workdir().as_os_str().is_empty(),
                        marked: st.marked.as_ref().map(|(p, h)| {
                            let short: String = h.chars().take(8).collect();
                            format!("{p}:{short}")
                        }),
                    },
                )
            })
        });
    }
    {
        let state = state.clone();
        let perform = perform.clone();
        dlg.on_menu_action(move |id| {
            let Some(action) = RepoBrowserAction::from_id(id) else {
                return;
            };
            let targets: Vec<TreeEntry> = state.borrow().selected().into_iter().cloned().collect();
            perform(action, targets);
        });
    }
    {
        let state = state.clone();
        let tree_cache = tree_cache.clone();
        let tree_menu_node = tree_menu_node.clone();
        dlg.on_tree_menu_for(move |node| {
            tree_menu_node.set(node as usize);
            tree_cache.get(node, || {
                let st = state.borrow();
                // OnContextMenu_RepoTree: the clicked folder, ONLY_FOLDERS.
                if st.nodes.get(node as usize).is_none() {
                    return Vec::new();
                }
                menu(
                    SelectionType::OnlyFolders,
                    &MenuState {
                        count: 1,
                        has_wc: true,
                        ..Default::default()
                    },
                )
            })
        });
    }
    {
        let state = state.clone();
        let perform = perform.clone();
        dlg.on_tree_menu_action(move |node, id| {
            let Some(action) = RepoBrowserAction::from_id(id) else {
                return;
            };
            let target = {
                let st = state.borrow();
                st.nodes.get(node as usize).map(|n| {
                    st.entries
                        .iter()
                        .find(|e| e.path == n.path)
                        .cloned()
                        .unwrap_or(TreeEntry {
                            path: String::new(),
                            name: "/".to_owned(),
                            kind: EntryKind::Folder,
                            oid: String::new(),
                            size: None,
                            executable: false,
                            symlink: false,
                        })
                })
            };
            if let Some(t) = target {
                perform(action, vec![t]);
            }
        });
    }
    {
        let weak = dlg.as_weak();
        let ask = ask.clone();
        let workdir = repo.workdir().to_owned();
        dlg.on_notice_clicked(move |i| {
            let Some(d) = weak.upgrade() else { return };
            notice(&d, "", &[], false);
            if let Ask::SubmoduleUpdate(path) = std::mem::replace(&mut *ask.borrow_mut(), Ask::None)
            {
                if i == 0 {
                    spawn_launcher(
                        &Invocation::new("subupdate")
                            .with("bkpath", workdir.to_string_lossy())
                            .with("selectedpath", path),
                    );
                }
            }
        });
    }
    for ok in [true, false] {
        let weak = dlg.as_weak();
        let h = move || {
            if let Some(d) = weak.upgrade() {
                let _ = d.hide();
            }
        };
        if ok {
            dlg.on_ok(h);
        } else {
            dlg.on_cancel(h);
        }
    }

    crate::window::prepare(&dlg);
    dlg.run()?;
    Ok(0)
}

fn notice(d: &RepoBrowserDialog, text: &str, buttons: &[&str], error: bool) {
    d.set_notice_text(text.into());
    let labels: Vec<SharedString> = buttons.iter().map(|b| (*b).into()).collect();
    d.set_notice_buttons(ModelRc::new(VecModel::from(labels)));
    d.set_notice_error(error);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(path: &str, kind: EntryKind) -> TreeEntry {
        TreeEntry {
            path: path.into(),
            name: path.rsplit('/').next().unwrap().into(),
            kind,
            oid: "abc".into(),
            size: (kind == EntryKind::File).then_some(10),
            executable: false,
            symlink: false,
        }
    }

    #[test]
    fn tree_nodes_and_info() {
        let entries = vec![
            e("a.txt", EntryKind::File),
            e("src", EntryKind::Folder),
            e("src/lib", EntryKind::Folder),
            e("src/lib/x.rs", EntryKind::File),
            e("vendor", EntryKind::Submodule),
        ];
        let mut expanded = BTreeSet::from([String::new()]);
        let nodes = visible_nodes(&entries, &expanded);
        assert_eq!(
            nodes
                .iter()
                .map(|n| (n.path.as_str(), n.depth, n.has_children))
                .collect::<Vec<_>>(),
            [("", 0, true), ("src", 1, true)]
        );
        expanded.insert("src".into());
        assert_eq!(visible_nodes(&entries, &expanded).len(), 3);

        let items = children(&entries, "");
        assert_eq!(
            info_text("", &items, &[]),
            "/\nShowing 1 files, 1 submodules and 1 folders, 3 items in total"
        );
        assert_eq!(
            info_text("", &items, &[&entries[0]]),
            "a.txt\nSize 10 bytes"
        );
        assert_eq!(
            info_text("", &items, &[&entries[4]]),
            "Submodule \"vendor\"\nRevision abc"
        );
        assert_eq!(
            info_text("", &items, &[&entries[0], &entries[1]]),
            "2 items selected"
        );
    }
}
