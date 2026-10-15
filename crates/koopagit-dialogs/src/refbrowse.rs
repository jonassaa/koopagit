//! `/command:refbrowse` — `CBrowseRefsDlg` ("Browse References"): a ref tree
//! (heads / remotes / tags) and a filterable, multi-select list of the refs
//! under the selected node, with the upstream tree and list menus.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::rc::Rc;

use koopagit_core::Repo;
use koopagit_core::git_cli::Git;
use koopagit_core::history::{RefInfo, RefType};
use koopagit_core::launcher::{CloseOnEnd, Invocation};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::menus::refbrowse::{
    self as menu, RefAction as A, RefLeaf, RefMenuInput, decode, strip_ref_name,
};
use crate::progress::ProgressJob;
use crate::rowsel::ListSel;
use crate::{
    BrowseRefsDialog, Context, DeleteRemoteTagDialog, DialogResult, RefCheckRow, RefRow,
    RefTreeRow, keep_alive, spawn_launcher,
};

/// Case-insensitive substring filter on the ref name.
pub fn ref_matches(r: &RefInfo, text: &str) -> bool {
    text.is_empty() || r.short_name.to_lowercase().contains(&text.to_lowercase())
}

/// A folder of the ref tree, e.g. `refs/remotes/origin`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeNode {
    pub path: String,
    pub label: String,
    pub depth: usize,
    pub has_children: bool,
}

/// Every folder of the ref tree in display order: `refs`, then `heads`,
/// `remotes`, `tags` and the folders inside them (refs themselves are leaves
/// and only appear in the list).
pub fn tree_nodes(refs: &[String]) -> Vec<TreeNode> {
    let mut folders: BTreeSet<String> = ["refs/heads", "refs/remotes", "refs/tags"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    for r in refs {
        let parts: Vec<&str> = r.split('/').collect();
        for k in 2..parts.len() {
            folders.insert(parts[..k].join("/"));
        }
    }
    folders.insert("refs".to_owned());
    // Order: refs, then heads, remotes, tags (upstream order), each subtree sorted.
    let rank = |p: &str| {
        if p == "refs" {
            0
        } else if p.starts_with("refs/heads") {
            1
        } else if p.starts_with("refs/remotes") {
            2
        } else if p.starts_with("refs/tags") {
            3
        } else {
            4
        }
    };
    let mut list: Vec<String> = folders.into_iter().collect();
    list.sort_by(|a, b| rank(a).cmp(&rank(b)).then(a.cmp(b)));
    list.iter()
        .map(|p| TreeNode {
            label: p.rsplit('/').next().unwrap_or(p).to_owned(),
            depth: p.matches('/').count(),
            has_children: list.iter().any(|q| q.starts_with(&format!("{p}/"))),
            path: p.clone(),
        })
        .collect()
}

/// The nodes visible with the given folders expanded.
pub fn visible_nodes<'a>(nodes: &'a [TreeNode], expanded: &BTreeSet<String>) -> Vec<&'a TreeNode> {
    nodes
        .iter()
        .filter(|n| {
            // Every ancestor must be expanded.
            let mut p = n.path.as_str();
            while let Some((parent, _)) = p.rsplit_once('/') {
                if !expanded.contains(parent) {
                    return false;
                }
                p = parent;
            }
            true
        })
        .collect()
}

/// Whether ref `full` is listed under tree node `node` (recursively).
pub fn under(full: &str, node: &str) -> bool {
    full.starts_with(&format!("{node}/"))
}

/// Ref kinds a picker lists (`gPickRef_*`, BrowseRefsDlg.h:28), passed as
/// `/pickkind:<bits>`.
pub const PICK_HEAD: u32 = 1;
pub const PICK_TAG: u32 = 2;
pub const PICK_REMOTE: u32 = 4;
pub const PICK_ALL: u32 = PICK_HEAD | PICK_TAG | PICK_REMOTE;
pub const PICK_NO_TAG: u32 = PICK_ALL & !PICK_TAG;

/// Whether `full` is listed for the picker kinds `kind` (BrowseRefsDlg.cpp:462).
pub fn pick_kind_allows(kind: u32, full: &str) -> bool {
    let blocked = (full.starts_with("refs/heads/") && kind & PICK_HEAD == 0)
        || (full.starts_with("refs/tags/") && kind & PICK_TAG == 0)
        || (full.starts_with("refs/remotes/") && kind & PICK_REMOTE == 0)
        // Only remote branches: no refs/stash either.
        || (kind == PICK_REMOTE && !full.starts_with("refs/remotes/"));
    !blocked
}

/// The tree node and list ref selected when the dialog opens (`Refresh` /
/// `SelectRef`, BrowseRefsDlg.cpp:432, 513): `HEAD` is the current branch,
/// a short name resolves like git's DWIM, and a node path selects that node.
pub fn initial_selection(
    refs: &[String],
    wanted: &str,
    current: Option<&str>,
) -> (String, Option<String>) {
    let fallback = || ("refs/heads".to_owned(), None);
    let wanted = match wanted {
        "" | "HEAD" => match current {
            Some(c) => format!("refs/heads/{c}"),
            None => return fallback(),
        },
        w => w.to_owned(),
    };
    let full = if wanted.starts_with("refs") {
        wanted
    } else {
        let Some(f) = ["refs/", "refs/tags/", "refs/heads/", "refs/remotes/"]
            .iter()
            .map(|p| format!("{p}{wanted}"))
            .find(|f| refs.contains(f))
        else {
            return fallback();
        };
        f
    };
    if refs.contains(&full) {
        let node = full.rsplit_once('/').map_or("refs", |(n, _)| n).to_owned();
        (node, Some(full))
    } else if refs.iter().any(|r| under(r, &full)) {
        (full, None)
    } else {
        fallback()
    }
}

#[derive(Clone)]
enum Pending {
    None,
    Delete(Vec<RefLeaf>),
}

#[derive(Clone)]
enum Input {
    None,
    Rename(String),
    Description(String),
    Upstream(String),
}

struct State {
    refs: Vec<RefInfo>,
    nodes: Vec<TreeNode>,
    expanded: BTreeSet<String>,
    visible_nodes: Vec<String>,
    node: String,
    rows: Vec<usize>,
    current: Option<String>,
    remotes: Vec<String>,
    pending: Pending,
    input: Input,
}

pub fn run(ctx: &mut Context) -> DialogResult<i32> {
    let dlg = BrowseRefsDialog::new()?;
    let workdir = ctx.repo.workdir().to_owned();
    dlg.set_dialog_title(crate::window_title(&workdir, "Browse References").into());
    // Picker mode (`CBrowseRefsDlg::PickRef`): `/outfile:<file>` receives the
    // chosen full ref name; `/rev:` is the initial ref, `/pickkind:` the kinds.
    let outfile = ctx.invocation.get("outfile").map(PathBuf::from);
    let want_pick = outfile.is_some();
    let kind = ctx
        .invocation
        .get("pickkind")
        .and_then(|k| k.parse().ok())
        .unwrap_or(PICK_ALL);
    let initial = ctx.invocation.get("rev").unwrap_or("HEAD").to_owned();
    let state = Rc::new(RefCell::new(State {
        refs: Vec::new(),
        nodes: Vec::new(),
        expanded: ["refs", "refs/heads", "refs/remotes", "refs/tags"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        visible_nodes: Vec::new(),
        node: "refs/heads".to_owned(),
        rows: Vec::new(),
        current: ctx.repo.current_branch()?,
        remotes: ctx.repo.remotes(),
        pending: Pending::None,
        input: Input::None,
    }));
    let sel = Rc::new(ListSel::default());
    let tree_model = Rc::new(VecModel::<RefTreeRow>::default());
    dlg.set_tree(ModelRc::from(tree_model.clone()));
    let model = Rc::new(VecModel::<RefRow>::default());
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
    let paint_tree: Rc<dyn Fn()> = {
        let state = state.clone();
        let tree_model = tree_model.clone();
        Rc::new(move || {
            let mut st = state.borrow_mut();
            let visible: Vec<TreeNode> = visible_nodes(&st.nodes, &st.expanded)
                .into_iter()
                .cloned()
                .collect();
            tree_model.set_vec(
                visible
                    .iter()
                    .map(|n| RefTreeRow {
                        label: n.label.clone().into(),
                        depth: n.depth as i32,
                        has_children: n.has_children,
                        expanded: st.expanded.contains(&n.path),
                        selected: n.path == st.node,
                    })
                    .collect::<Vec<_>>(),
            );
            st.visible_nodes = visible.into_iter().map(|n| n.path).collect();
        })
    };
    let refilter: Rc<dyn Fn()> = {
        let state = state.clone();
        let weak = dlg.as_weak();
        let sel = sel.clone();
        let paint = paint.clone();
        let model = model.clone();
        Rc::new(move || {
            let Some(d) = weak.upgrade() else { return };
            let mut st = state.borrow_mut();
            let text = d.get_filter_text().to_string();
            let rows: Vec<usize> = st
                .refs
                .iter()
                .enumerate()
                .filter(|(_, r)| under(&r.full_name, &st.node) && ref_matches(r, &text))
                .map(|(i, _)| i)
                .collect();
            let data: Vec<RefRow> = rows
                .iter()
                .map(|&i| {
                    let r = &st.refs[i];
                    RefRow {
                        name: r.short_name.clone().into(),
                        kind: r.kind.label().into(),
                        date: crate::format::short_datetime(r.time, r.offset).into(),
                        author: r.author.clone().into(),
                        sha: r.id[..r.id.len().min(8)].to_string().into(),
                        message: r.subject.clone().into(),
                        upstream: r.upstream.clone().into(),
                        is_current: r.kind == RefType::Branch
                            && st.current.as_deref() == Some(r.short_name.as_str()),
                        selected: false,
                    }
                })
                .collect();
            model.set_vec(data);
            sel.reset(rows.len(), false);
            st.rows = rows;
            drop(st);
            paint();
        })
    };
    let reload: Rc<dyn Fn()> = {
        let state = state.clone();
        let refilter = refilter.clone();
        let paint_tree = paint_tree.clone();
        let workdir = workdir.clone();
        Rc::new(move || {
            if let Ok(repo) = Repo::discover(&workdir) {
                let mut st = state.borrow_mut();
                st.refs = repo.ref_infos().unwrap_or_default();
                st.refs.retain(|r| pick_kind_allows(kind, &r.full_name));
                st.current = repo.current_branch().ok().flatten();
                st.remotes = repo.remotes();
                let names: Vec<String> = st.refs.iter().map(|r| r.full_name.clone()).collect();
                st.nodes = tree_nodes(&names);
                if !st.nodes.iter().any(|n| n.path == st.node) {
                    st.node = "refs/heads".to_owned();
                }
            }
            paint_tree();
            refilter();
        })
    };
    reload();
    {
        // `Refresh(m_initialRef)`: select the initial ref and its tree node.
        let (node, leaf) = {
            let st = state.borrow();
            let names: Vec<String> = st.refs.iter().map(|r| r.full_name.clone()).collect();
            initial_selection(&names, &initial, st.current.as_deref())
        };
        {
            let mut st = state.borrow_mut();
            let mut path = node.as_str();
            while let Some((parent, _)) = path.rsplit_once('/') {
                st.expanded.insert(parent.to_owned());
                path = parent;
            }
            st.node = node;
        }
        paint_tree();
        refilter();
        let row = leaf.and_then(|leaf| {
            let st = state.borrow();
            st.rows.iter().position(|&i| st.refs[i].full_name == leaf)
        });
        if let Some(row) = row {
            sel.click(row, false, false);
            paint();
        }
    }

    let actions = Rc::new(Actions {
        outfile,
        want_pick,
        workdir: workdir.clone(),
        has_wc: true,
        state: state.clone(),
        sel: sel.clone(),
        tree_cache: crate::menus::MenuCache::default(),
        weak: dlg.as_weak(),
        reload: reload.clone(),
    });
    {
        let refilter = refilter.clone();
        dlg.on_filter_changed(move || refilter());
    }
    {
        let reload = reload.clone();
        dlg.on_refresh(move || reload());
    }
    {
        let (state, paint_tree, refilter) = (state.clone(), paint_tree.clone(), refilter.clone());
        let a = actions.clone();
        dlg.on_tree_clicked(move |i| {
            let path = state.borrow().visible_nodes.get(i as usize).cloned();
            if let Some(p) = path {
                state.borrow_mut().node = p;
                a.tree_cache.invalidate();
                paint_tree();
                refilter();
            }
        });
    }
    {
        let (state, paint_tree) = (state.clone(), paint_tree.clone());
        dlg.on_tree_toggled(move |i| {
            let path = state.borrow().visible_nodes.get(i as usize).cloned();
            if let Some(p) = path {
                let mut st = state.borrow_mut();
                if !st.expanded.remove(&p) {
                    st.expanded.insert(p);
                }
                drop(st);
                paint_tree();
            }
        });
    }
    {
        let (sel, paint) = (sel.clone(), paint.clone());
        // A picker selects one ref (LVS_SINGLESEL).
        dlg.on_row_clicked(move |row, toggle, range| {
            sel.click(row as usize, toggle && !want_pick, range && !want_pick);
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
            if want_pick && delta == 0 {
                return;
            }
            sel.key(delta, extend && !want_pick, model.row_count());
            paint();
        });
    }
    {
        let a = actions.clone();
        dlg.on_menu_for(move |row| {
            let a2 = a.clone();
            a.sel.cache.get(row, move || a2.list_menu(row as usize))
        });
    }
    {
        let a = actions.clone();
        dlg.on_menu_action(move |row, id| a.list_action(row as usize, id));
    }
    {
        let a = actions.clone();
        dlg.on_tree_menu_for(move |row| {
            let a2 = a.clone();
            a.tree_cache.get(row, move || a2.tree_menu(row as usize))
        });
    }
    {
        let a = actions.clone();
        dlg.on_tree_menu_action(move |row, id| a.tree_action(row as usize, id));
    }
    {
        let a = actions.clone();
        // Double-click: pick in a picker, else "Show log" for commits
        // (OnNMDblclkListRefLeafs).
        dlg.on_row_activated(move |row| {
            let action = if a.want_pick { A::Select } else { A::ViewLog };
            a.list_action(row as usize, crate::menus::ActionId::id(action))
        });
    }
    {
        let a = actions.clone();
        dlg.on_notice_clicked(move |i| a.notice_clicked(i));
    }
    {
        let a = actions.clone();
        dlg.on_input_accepted(move || a.input_accepted());
    }
    {
        let a = actions.clone();
        dlg.on_input_cancelled(move || a.close_input());
    }
    {
        let a = actions.clone();
        dlg.on_rename_selected(move || a.rename_selected());
    }
    {
        let a = actions.clone();
        // OK picks the selected ref, if exactly one (GetSelectedRef(true, false)).
        dlg.on_ok(move || match a.selected_ref() {
            Some(r) => a.pick(&r),
            None => a.close(),
        });
    }
    {
        let a = actions.clone();
        dlg.on_cancel(move || a.close());
    }
    {
        let a = actions.clone();
        dlg.on_current_branch(move || {
            let current = a.state.borrow().current.clone();
            a.pick(&current.map_or_else(|| "HEAD".to_owned(), |c| format!("refs/heads/{c}")));
        });
    }
    crate::window::prepare(&dlg);
    dlg.run()?;
    Ok(0)
}

struct Actions {
    /// Picker mode: where the chosen ref is written.
    outfile: Option<PathBuf>,
    want_pick: bool,
    workdir: PathBuf,
    has_wc: bool,
    state: Rc<RefCell<State>>,
    sel: Rc<ListSel>,
    tree_cache: crate::menus::MenuCache,
    weak: slint::Weak<BrowseRefsDialog>,
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

    fn close(&self) {
        if let Some(d) = self.weak.upgrade() {
            let _ = d.hide();
        }
    }

    /// Closes with `full` as the picked ref (`m_pickedRef`), written to
    /// `/outfile` in picker mode.
    fn pick(&self, full: &str) {
        if let Some(file) = &self.outfile {
            if let Err(e) = std::fs::write(file, format!("{full}\n")) {
                eprintln!("koopagit: {e}");
            }
        }
        self.close();
    }

    /// The single selected ref.
    fn selected_ref(&self) -> Option<String> {
        let s = self.sel.sel.borrow();
        if s.len() != 1 {
            return None;
        }
        let row = s.rows().next()?;
        let st = self.state.borrow();
        st.rows.get(row).map(|&i| st.refs[i].full_name.clone())
    }

    fn git(&self) -> Git {
        Git::new(&self.workdir)
    }

    /// Selected refs for a menu opened on `row`; the last-clicked ref is last.
    fn leaves(&self, row: usize) -> Vec<RefInfo> {
        let rows = self.sel.effective(row);
        let current = self.sel.sel.borrow().current().unwrap_or(row);
        let st = self.state.borrow();
        let mut out: Vec<(usize, RefInfo)> = rows
            .iter()
            .filter_map(|&r| st.rows.get(r).map(|&i| (r, st.refs[i].clone())))
            .collect();
        out.sort_by_key(|(r, _)| *r == current);
        out.into_iter().map(|(_, r)| r).collect()
    }

    fn leaf_of(r: &RefInfo) -> RefLeaf {
        RefLeaf {
            full: r.full_name.clone(),
            is_commit: r.is_commit,
            has_upstream: !r.upstream.is_empty(),
        }
    }

    fn input(&self, row: usize) -> RefMenuInput {
        let st = self.state.borrow();
        RefMenuInput {
            leaves: Vec::new(),
            node: st.visible_nodes.get(row).cloned(),
            has_wc: self.has_wc,
            current_branch: st.current.clone().unwrap_or_default(),
            remotes: st.remotes.clone(),
            want_pick: false,
        }
    }

    fn list_menu(&self, row: usize) -> Vec<crate::menus::Entry> {
        let leaves: Vec<RefLeaf> = self.leaves(row).iter().map(Self::leaf_of).collect();
        let st = self.state.borrow();
        menu::build(&RefMenuInput {
            leaves,
            node: Some(st.node.clone()),
            has_wc: self.has_wc,
            current_branch: st.current.clone().unwrap_or_default(),
            remotes: st.remotes.clone(),
            want_pick: self.want_pick,
        })
    }

    fn tree_menu(&self, row: usize) -> Vec<crate::menus::Entry> {
        menu::build(&self.input(row))
    }

    fn job(&self, title: &str, commands: Vec<Vec<String>>) {
        let job = ProgressJob {
            title: crate::window_title(&self.workdir, title),
            git: self.git(),
            commands,
            post_actions: Vec::new(),
            close_on_end: CloseOnEnd::IfNoErrors,
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

    /// The name used for dialogs: `remotes/<r>/<b>` for remote branches.
    fn dialog_name(r: &RefInfo) -> String {
        match r.kind {
            RefType::Remote => format!("remotes/{}", r.short_name),
            _ => r.short_name.clone(),
        }
    }

    fn ask_delete(&self, leaves: Vec<RefLeaf>) {
        if leaves.is_empty() {
            return;
        }
        let unmerged = leaves.len() == 1
            && !Repo::discover(&self.workdir).is_ok_and(|r| r.is_ancestor(&leaves[0].full, "HEAD"));
        let text = menu::confirm_delete_text(&leaves, unmerged);
        self.state.borrow_mut().pending = Pending::Delete(leaves);
        self.notice(&text, &["Yes", "No"], false);
    }

    fn list_action(&self, row: usize, id: i32) {
        let Some((action, _)) = decode(id) else {
            return;
        };
        self.sel.right(row);
        let refs = self.leaves(row);
        let Some(r) = refs.last().cloned() else {
            return;
        };
        let name = Self::dialog_name(&r);
        let st_remotes = self.state.borrow().remotes.clone();
        match action {
            A::Select => self.pick(&r.full_name),
            A::ViewLog => {
                if r.is_commit {
                    spawn_launcher(&self.launch("log").with("range", name))
                }
            }
            A::RepoBrowser => {
                spawn_launcher(&self.launch("repobrowser").with("rev", r.full_name.clone()))
            }
            A::ShowReflog => {
                spawn_launcher(&self.launch("reflog").with("ref", r.full_name.clone()))
            }
            A::DiffWc => spawn_launcher(
                &self
                    .launch("showcompare")
                    .with("revision1", r.full_name.clone())
                    .with("revision2", ""),
            ),
            A::Fetch => {
                if let Some((remote, _)) = menu::split_remote(&r.full_name, &st_remotes) {
                    spawn_launcher(&self.launch("fetch").with("remote", remote));
                }
            }
            A::Merge => spawn_launcher(&self.launch("merge").with("rev", name)),
            A::Switch => spawn_launcher(&self.launch("switch").with("rev", name)),
            A::CreateBranch => spawn_launcher(&self.launch("branch").with("rev", r.id.clone())),
            A::EditBranchDescription => {
                let current = self
                    .git()
                    .run([
                        "config",
                        "--get",
                        &format!("branch.{}.description", r.short_name),
                    ])
                    .unwrap_or_default();
                self.open_input(
                    &format!("Description of \"{}\":", r.short_name),
                    current.trim(),
                    Input::Description(r.short_name.clone()),
                );
            }
            A::Rename => self.open_input(
                &format!("Rename \"{}\" to:", r.short_name),
                &r.short_name,
                Input::Rename(r.short_name.clone()),
            ),
            A::UpstreamDrop => self.job(
                "Unset tracked branch",
                vec![vec![
                    "branch".into(),
                    "--unset-upstream".into(),
                    r.short_name.clone(),
                ]],
            ),
            A::UpstreamSet => {
                let remote = st_remotes
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "origin".into());
                let proposal = if r.upstream.is_empty() {
                    format!("{remote}/{}", r.short_name)
                } else {
                    r.upstream.clone()
                };
                self.open_input(
                    &format!("Tracked branch of \"{}\":", r.short_name),
                    &proposal,
                    Input::Upstream(r.short_name.clone()),
                );
            }
            A::Diff | A::UnifiedDiff => {
                if let [a, b] = refs.as_slice() {
                    let mut inv = self
                        .launch("showcompare")
                        .with("revision1", a.full_name.clone())
                        .with("revision2", b.full_name.clone());
                    if action == A::UnifiedDiff {
                        inv = inv.with_switch("unified");
                    }
                    spawn_launcher(&inv);
                }
            }
            A::ViewLogRange | A::ViewLogRangeReachableFromOnlyOne => {
                if let [a, b] = refs.as_slice() {
                    let dots = if action == A::ViewLogRange {
                        ".."
                    } else {
                        "..."
                    };
                    let range = format!(
                        "{}{dots}{}",
                        strip_ref_name(&a.full_name),
                        strip_ref_name(&b.full_name)
                    );
                    spawn_launcher(&self.launch("log").with("range", range));
                }
            }
            A::DeleteRemoteBranch | A::DeleteBranch | A::DeleteTag => {
                self.ask_delete(refs.iter().map(Self::leaf_of).collect())
            }
            A::Copy => self.copy(
                &refs
                    .iter()
                    .map(|r| strip_ref_name(&r.full_name).to_owned())
                    .collect::<Vec<_>>()
                    .join("\r\n"),
            ),
            // Tree-only entries can appear in the list menu when nothing is selected.
            _ => self.node_action(&self.state.borrow().node.clone(), id),
        }
    }

    fn tree_action(&self, row: usize, id: i32) {
        let node = self.state.borrow().visible_nodes.get(row).cloned();
        if let Some(node) = node {
            self.node_action(&node, id);
        }
    }

    fn node_action(&self, node: &str, id: i32) {
        let Some((action, idx)) = decode(id) else {
            return;
        };
        let remotes = self.state.borrow().remotes.clone();
        match action {
            A::ManageRemotes => spawn_launcher(&self.launch("settings").with("page", "gitremote")),
            A::Fetch => {
                let remote = remotes
                    .get(idx)
                    .cloned()
                    .or_else(|| node.strip_prefix("refs/remotes/").map(str::to_owned));
                if let Some(r) = remote {
                    spawn_launcher(&self.launch("fetch").with("remote", r));
                }
            }
            A::DeleteRemoteTag => {
                if let Some(r) = remotes.get(idx) {
                    delete_remote_tags(self.workdir.clone(), r);
                }
            }
            A::CreateBranch => spawn_launcher(&self.launch("branch")),
            A::CreateTag => spawn_launcher(&self.launch("tag")),
            A::DeleteAllTags => {
                let tags: Vec<RefLeaf> = self
                    .state
                    .borrow()
                    .refs
                    .iter()
                    .filter(|r| r.kind == RefType::Tag)
                    .map(Self::leaf_of)
                    .collect();
                self.ask_delete(tags);
            }
            _ => {}
        }
    }

    fn open_input(&self, label: &str, value: &str, input: Input) {
        self.state.borrow_mut().input = input;
        if let Some(d) = self.weak.upgrade() {
            d.set_input_text(value.into());
            d.set_input_label(label.into());
        }
    }

    /// F2 (`EditLabel` on the first selected ref): only branches can be renamed.
    fn rename_selected(&self) {
        let first = self.sel.sel.borrow().rows().next();
        let r = {
            let st = self.state.borrow();
            first.and_then(|row| st.rows.get(row).map(|&i| st.refs[i].clone()))
        };
        if let Some(r) = r.filter(|r| r.kind == RefType::Branch) {
            self.open_input(
                &format!("Rename \"{}\" to:", r.short_name),
                &r.short_name,
                Input::Rename(r.short_name.clone()),
            );
        }
    }

    fn close_input(&self) {
        self.state.borrow_mut().input = Input::None;
        if let Some(d) = self.weak.upgrade() {
            d.set_input_label("".into());
        }
    }

    fn input_accepted(&self) {
        let Some(d) = self.weak.upgrade() else { return };
        let text = d.get_input_text().trim().to_owned();
        let input = self.state.borrow().input.clone();
        let cmd: Vec<String> = match input {
            Input::None => return,
            Input::Rename(old) => {
                if text.is_empty() || text == old {
                    self.close_input();
                    return;
                }
                vec!["branch".into(), "-m".into(), old, "--".into(), text]
            }
            Input::Description(b) => {
                let key = format!("branch.{b}.description");
                if text.is_empty() {
                    vec!["config".into(), "--unset".into(), key]
                } else {
                    vec!["config".into(), key, text]
                }
            }
            Input::Upstream(b) => {
                if text.is_empty() {
                    return;
                }
                vec![
                    "branch".into(),
                    format!("--set-upstream-to={text}"),
                    "--".into(),
                    b,
                ]
            }
        };
        match self.git().output(&cmd) {
            Ok(o) if o.success() || cmd.get(1).is_some_and(|a| a == "--unset") => {
                self.close_input();
                (self.reload)();
            }
            Ok(o) => self.notice(o.stderr.trim(), &[], true),
            Err(e) => self.notice(&e.to_string(), &[], true),
        }
    }

    fn notice_clicked(&self, i: i32) {
        let pending = std::mem::replace(&mut self.state.borrow_mut().pending, Pending::None);
        self.notice("", &[], false);
        if let (Pending::Delete(leaves), 0) = (pending, i) {
            let remotes = self.state.borrow().remotes.clone();
            self.job("Delete", menu::delete_commands(&leaves, &remotes));
        }
    }
}

/// `git ls-remote --tags <remote>` tag names (peeled `^{}` entries dropped).
pub fn parse_remote_tags(out: &str) -> Vec<String> {
    out.lines()
        .filter_map(|l| l.split_whitespace().nth(1))
        .filter_map(|r| r.strip_prefix("refs/tags/"))
        .filter(|t| !t.ends_with("^{}"))
        .map(str::to_owned)
        .collect()
}

/// `CDeleteRemoteTagDlg`: lists the remote's tags; OK pushes their deletion.
fn delete_remote_tags(workdir: PathBuf, remote: &str) {
    let Ok(dlg) = DeleteRemoteTagDialog::new() else {
        return;
    };
    crate::window::apply_theme(&dlg);
    dlg.set_dialog_title(crate::window_title(&workdir, "Delete remote tags").into());
    dlg.set_remote(remote.into());
    let model = Rc::new(VecModel::<RefCheckRow>::default());
    dlg.set_tags(ModelRc::from(model.clone()));
    // Lists the remote's tags; F5 runs it again.
    let load = {
        let weak = dlg.as_weak();
        let model = model.clone();
        let git = Git::new(&workdir);
        let remote = remote.to_owned();
        move || {
            let Some(d) = weak.upgrade() else { return };
            let out = git.output(["ls-remote", "--tags", "--", remote.as_str()]);
            let tags = match &out {
                Ok(o) if o.success() => parse_remote_tags(&o.stdout),
                _ => Vec::new(),
            };
            d.set_status_text(match &out {
                Ok(o) if !o.success() => o.stderr.trim().into(),
                Err(e) => e.to_string().into(),
                _ => format!("{} tag(s)", tags.len()).into(),
            });
            model.set_vec(
                tags.iter()
                    .map(|t| RefCheckRow {
                        text: t.as_str().into(),
                        checked: false,
                    })
                    .collect::<Vec<_>>(),
            );
        }
    };
    load();
    dlg.on_refresh(load);
    {
        let model = model.clone();
        dlg.on_toggled(move |i, c| {
            if let Some(mut r) = model.row_data(i as usize) {
                r.checked = c;
                model.set_row_data(i as usize, r);
            }
        });
    }
    {
        let model = model.clone();
        dlg.on_select_all(move |c| {
            for i in 0..model.row_count() {
                if let Some(mut r) = model.row_data(i) {
                    r.checked = c;
                    model.set_row_data(i, r);
                }
            }
        });
    }
    {
        let weak = dlg.as_weak();
        let remote = remote.to_owned();
        dlg.on_ok(move || {
            let mut cmd = vec!["push".to_owned(), "--".to_owned(), remote.clone()];
            cmd.extend(
                (0..model.row_count())
                    .filter_map(|i| model.row_data(i))
                    .filter(|r| r.checked)
                    .map(|r| format!(":refs/tags/{}", r.text)),
            );
            if let Some(d) = weak.upgrade() {
                let _ = d.hide();
            }
            if cmd.len() > 3 {
                let job = ProgressJob {
                    title: crate::window_title(&workdir, "Delete remote tags"),
                    git: Git::new(&workdir),
                    commands: vec![cmd],
                    post_actions: Vec::new(),
                    close_on_end: CloseOnEnd::Manual,
                };
                if let Ok(p) = crate::progress::start(job, |_| {}) {
                    keep_alive(p);
                }
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
    if dlg.show().is_ok() {
        keep_alive(dlg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(full: &str, kind: RefType) -> RefInfo {
        RefInfo {
            full_name: full.into(),
            short_name: full.rsplit('/').next().unwrap().into(),
            kind,
            id: "0".repeat(40),
            subject: String::new(),
            time: 0,
            offset: 0,
            author: String::new(),
            upstream: String::new(),
            is_commit: true,
        }
    }

    #[test]
    fn picker_kinds_and_initial_selection() {
        assert!(pick_kind_allows(PICK_ALL, "refs/stash"));
        assert!(!pick_kind_allows(PICK_NO_TAG, "refs/tags/v1"));
        assert!(pick_kind_allows(PICK_NO_TAG, "refs/remotes/origin/main"));
        assert!(!pick_kind_allows(PICK_REMOTE, "refs/heads/main"));
        assert!(!pick_kind_allows(PICK_REMOTE, "refs/stash"));
        assert!(pick_kind_allows(PICK_REMOTE, "refs/remotes/origin/main"));

        let refs: Vec<String> = [
            "refs/heads/main",
            "refs/heads/feature/a",
            "refs/remotes/origin/main",
            "refs/tags/v1",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let pick = |w: &str, c: Option<&str>| initial_selection(&refs, w, c);
        assert_eq!(
            pick("HEAD", Some("feature/a")),
            (
                "refs/heads/feature".into(),
                Some("refs/heads/feature/a".into())
            )
        );
        assert_eq!(pick("HEAD", None), ("refs/heads".into(), None));
        assert_eq!(
            pick("v1", None),
            ("refs/tags".into(), Some("refs/tags/v1".into()))
        );
        assert_eq!(
            pick("origin/main", None),
            (
                "refs/remotes/origin".into(),
                Some("refs/remotes/origin/main".into())
            )
        );
        assert_eq!(pick("refs/remotes", None), ("refs/remotes".into(), None));
        assert_eq!(pick("nope", None), ("refs/heads".into(), None));
    }

    #[test]
    fn filter() {
        let refs = [
            info("refs/heads/main", RefType::Branch),
            info("refs/remotes/origin/Feature", RefType::Remote),
        ];
        assert_eq!(refs.iter().filter(|r| ref_matches(r, "FEAT")).count(), 1);
        assert_eq!(refs.iter().filter(|r| ref_matches(r, "")).count(), 2);
        assert!(under("refs/remotes/origin/x", "refs/remotes"));
        assert!(!under("refs/remotes/origin/x", "refs/remotes/orig"));
    }

    #[test]
    fn tree_folders_and_visibility() {
        let refs: Vec<String> = [
            "refs/heads/main",
            "refs/heads/feature/a",
            "refs/remotes/origin/main",
            "refs/tags/v1",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let nodes = tree_nodes(&refs);
        let paths: Vec<&str> = nodes.iter().map(|n| n.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "refs",
                "refs/heads",
                "refs/heads/feature",
                "refs/remotes",
                "refs/remotes/origin",
                "refs/tags"
            ]
        );
        assert!(nodes[1].has_children && !nodes[2].has_children);
        assert_eq!(nodes[4].label, "origin");
        assert_eq!(nodes[4].depth, 2);
        let expanded: BTreeSet<String> = ["refs", "refs/heads"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        let vis: Vec<&str> = visible_nodes(&nodes, &expanded)
            .iter()
            .map(|n| n.path.as_str())
            .collect();
        assert_eq!(
            vis,
            [
                "refs",
                "refs/heads",
                "refs/heads/feature",
                "refs/remotes",
                "refs/tags"
            ]
        );
    }

    #[test]
    fn remote_tags_and_git_commands() {
        assert_eq!(
            parse_remote_tags("aaa\trefs/tags/v1\nbbb\trefs/tags/v1^{}\nccc\trefs/tags/v2\n"),
            ["v1", "v2"]
        );
        // The rename and upstream command lines git accepts.
        let dir = tempfile::tempdir().unwrap();
        let git = Git::new(dir.path());
        git.run(["init", "-q", "-b", "main"]).unwrap();
        git.run([
            "-c",
            "user.name=T",
            "-c",
            "user.email=t@e",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "a",
        ])
        .unwrap();
        git.run(["branch", "old"]).unwrap();
        git.run(["branch", "-m", "old", "--", "new"]).unwrap();
        assert!(git.run(["rev-parse", "--verify", "new"]).is_ok());
        git.run(["branch", "--set-upstream-to=main", "--", "new"])
            .unwrap();
        git.run(["branch", "--unset-upstream", "new"]).unwrap();
    }
}
