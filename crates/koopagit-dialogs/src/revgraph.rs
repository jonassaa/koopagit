//! `/command:revisiongraph` — the Revision Graph window (`CRevisionGraphDlg`).

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use koopagit_core::Repo;
use koopagit_core::launcher::{CloseOnEnd, Invocation};
use koopagit_core::log::{RefKind, RefLabel};
use koopagit_core::revgraph::{self, RgNode, RgOptions};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::menu_actions;
use crate::menus::{Entry, Menu, MenuCache};
use crate::progress::ProgressJob;
use crate::{
    Context, DialogResult, GraphNode, MessageBox, RevisionGraphDialog, keep_alive, spawn_launcher,
};

const NODE_W: f32 = 170.0;
const NODE_H: f32 = 46.0;
const COL_GAP: f32 = 40.0;
const ROW_GAP: f32 = 26.0;
/// `git diff` "working tree" pseudo revision (`GitRev::GetWorkingCopyRef`).
const WORKING_TREE: &str = "0000000000000000000000000000000000000000";

menu_actions! {
    /// Node context menu (`CRevisionGraphWnd::OnContextMenu`).
    pub enum RgAction {
        ShowLog = 1,
        BrowseRepo = 2,
        Switch = 3,
        SwitchToRev = 4,
        CopyRefs = 5,
        Delete = 6,
        CompareHeads = 7,
        UnidiffHeads = 8,
        CompareWorkingTree = 9,
        CompareRevs = 10,
        UnidiffRevs = 11,
    }
}

/// Raw ids for per-ref submenu entries.
const SWITCH_BASE: i32 = 100;
const DELETE_BASE: i32 = 200;
const DELETE_ALL: i32 = 299;

/// Branch names offered by "Switch/Checkout to" (local branches except the current one).
fn switch_names(labels: &[RefLabel]) -> Vec<String> {
    labels
        .iter()
        .filter(|l| l.kind == RefKind::LocalBranch)
        .map(|l| l.name.clone())
        .collect()
}

/// The first remote branch or tag, for "Switch/Checkout to this...".
fn switch_rev(labels: &[RefLabel]) -> Option<String> {
    labels
        .iter()
        .find(|l| matches!(l.kind, RefKind::RemoteBranch | RefKind::Tag))
        .map(|l| match l.kind {
            RefKind::RemoteBranch => format!("remotes/{}", l.name),
            _ => l.name.clone(),
        })
}

fn deletable(labels: &[RefLabel]) -> Vec<RefLabel> {
    labels
        .iter()
        .filter(|l| {
            matches!(
                l.kind,
                RefKind::LocalBranch | RefKind::RemoteBranch | RefKind::Tag
            )
        })
        .cloned()
        .collect()
}

/// Builds the node menu for the current selection (`selected` = node indices,
/// at most two, the clicked node first).
pub fn node_menu(nodes: &[RgNode], selected: &[usize]) -> Vec<Entry> {
    let mut m = Menu::new();
    m.item(RgAction::ShowLog, "Show &log");
    match selected {
        [one] => {
            let labels = &nodes[*one].labels;
            m.item(RgAction::BrowseRepo, "&Browse repository");
            let names = switch_names(labels);
            match names.as_slice() {
                [] => {
                    if switch_rev(labels).is_some() {
                        m.item(RgAction::SwitchToRev, "Sw&itch/Checkout to this...");
                    }
                }
                [b] => {
                    m.item(RgAction::Switch, format!("&Switch/Checkout to \"{b}\""));
                }
                many => {
                    let children = many
                        .iter()
                        .enumerate()
                        .map(|(i, b)| crate::menus::Leaf {
                            id: SWITCH_BASE + i as i32,
                            title: b.clone(),
                            enabled: true,
                        })
                        .collect();
                    m.sub("&Switch/Checkout to", children);
                }
            }
            m.item(RgAction::CopyRefs, "Copy ref names");
            let del = deletable(labels);
            match del.as_slice() {
                [] => {}
                [r] => {
                    m.item(RgAction::Delete, format!("&Delete {}", r.name));
                }
                many => {
                    let mut children: Vec<crate::menus::Leaf> = many
                        .iter()
                        .enumerate()
                        .map(|(i, r)| crate::menus::Leaf {
                            id: DELETE_BASE + i as i32,
                            title: r.name.clone(),
                            enabled: true,
                        })
                        .collect();
                    children.push(crate::menus::Leaf {
                        id: DELETE_ALL,
                        title: "All".to_owned(),
                        enabled: true,
                    });
                    m.sub("&Delete branch/tag", children);
                }
            }
            m.item(RgAction::CompareHeads, "Compare &HEAD revisions");
            m.item(RgAction::UnidiffHeads, "Unified &diff of HEAD revisions");
            m.item(RgAction::CompareWorkingTree, "Compare with &working tree");
        }
        [_, _] => {
            m.item(RgAction::CompareRevs, "&Compare revisions");
            m.item(RgAction::UnidiffRevs, "&Unified diff");
        }
        _ => {}
    }
    m.finish()
}

fn node_kind(n: &RgNode, child_count: usize) -> i32 {
    let has = |k: RefKind| n.labels.iter().any(|l| l.kind == k);
    if has(RefKind::CurrentBranch) {
        1
    } else if has(RefKind::LocalBranch) {
        2
    } else if has(RefKind::RemoteBranch) {
        3
    } else if has(RefKind::Tag) {
        4
    } else if n.parents.len() > 1 || child_count > 1 {
        5
    } else {
        0
    }
}

/// Graph model for Slint: nodes, edge path commands, size.
pub fn graph_model(nodes: &[RgNode], selected: &[usize]) -> (Vec<GraphNode>, String, f32, f32) {
    let layout = revgraph::layout(nodes);
    let mut children = vec![0usize; nodes.len()];
    for &(_, p) in &layout.edges {
        children[p] += 1;
    }
    let pos = |i: usize| {
        let (row, col) = layout.cells[i];
        (
            col as f32 * (NODE_W + COL_GAP),
            row as f32 * (NODE_H + ROW_GAP),
        )
    };
    let g: Vec<GraphNode> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let (x, y) = pos(i);
            GraphNode {
                x,
                y,
                title: n.id.chars().take(8).collect::<String>().into(),
                labels: n
                    .labels
                    .iter()
                    .map(|l| l.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
                    .into(),
                subject: n.subject.as_str().into(),
                kind: node_kind(n, children[i]),
                selected: selected.contains(&i),
            }
        })
        .collect();
    let mut edges = String::new();
    for &(c, p) in &layout.edges {
        let (cx, cy) = pos(c);
        let (px, py) = pos(p);
        edges.push_str(&format!(
            "M {} {} L {} {} ",
            cx + NODE_W / 2.0,
            cy + NODE_H,
            px + NODE_W / 2.0,
            py
        ));
    }
    if edges.is_empty() {
        edges.push_str("M 0 0");
    }
    let width = layout.columns as f32 * (NODE_W + COL_GAP) - COL_GAP;
    let height = (nodes.len().max(1)) as f32 * (NODE_H + ROW_GAP) - ROW_GAP;
    (g, edges, width.max(NODE_W), height.max(NODE_H))
}

fn delete_args(l: &RefLabel) -> Vec<String> {
    match l.kind {
        RefKind::Tag => vec!["tag".into(), "-d".into(), "--".into(), l.name.clone()],
        RefKind::RemoteBranch => {
            let (remote, branch) = l.name.split_once('/').unwrap_or((&l.name, ""));
            vec![
                "push".into(),
                remote.to_owned(),
                "--delete".into(),
                format!("refs/heads/{branch}"),
            ]
        }
        _ => vec!["branch".into(), "-D".into(), "--".into(), l.name.clone()],
    }
}

struct State {
    workdir: PathBuf,
    options: RgOptions,
    nodes: Vec<RgNode>,
    selected: Vec<usize>,
    menu: MenuCache,
}

impl State {
    fn load(&mut self) {
        self.nodes = Repo::discover(&self.workdir)
            .and_then(|r| r.revision_graph(&self.options))
            .unwrap_or_default();
        self.selected.clear();
        self.menu.invalidate();
    }

    fn rev(&self, i: usize) -> String {
        self.nodes[i].id.clone()
    }

    fn launch(&self, cmd: &str) -> Invocation {
        Invocation::new(cmd).with_paths([self.workdir.clone()])
    }
}

/// `/command:revisiongraph`.
pub fn run(ctx: &mut Context) -> DialogResult<i32> {
    let dlg = RevisionGraphDialog::new()?;
    dlg.set_dialog_title(ctx.window_title("Revision Graph").into());
    let state = Rc::new(RefCell::new(State {
        workdir: ctx.repo.workdir().to_owned(),
        options: RgOptions::default(),
        nodes: Vec::new(),
        selected: Vec::new(),
        menu: MenuCache::default(),
    }));
    state.borrow_mut().load();

    let refresh: Rc<dyn Fn()> = {
        let state = state.clone();
        let weak = dlg.as_weak();
        Rc::new(move || {
            let Some(d) = weak.upgrade() else { return };
            let st = state.borrow();
            let (nodes, edges, w, h) = graph_model(&st.nodes, &st.selected);
            d.set_nodes(ModelRc::new(VecModel::from(nodes)));
            d.set_edges(edges.into());
            d.set_graph_width(w);
            d.set_graph_height(h);
            d.set_branchings_and_merges(st.options.branchings_and_merges);
            d.set_all_tags(st.options.all_tags);
            d.set_one_selected(st.selected.len() == 1);
            d.set_two_selected(st.selected.len() == 2);
            d.set_status_text(
                format!(
                    "{} node(s){}",
                    st.nodes.len(),
                    match st.selected.as_slice() {
                        [] => String::new(),
                        s => format!(", {} selected", s.len()),
                    }
                )
                .into(),
            );
        })
    };
    refresh();

    {
        let state = state.clone();
        let refresh = refresh.clone();
        dlg.on_node_clicked(move |i, add| {
            {
                let mut st = state.borrow_mut();
                let i = i as usize;
                if add && !st.selected.contains(&i) {
                    if st.selected.len() >= 2 {
                        st.selected.remove(0);
                    }
                    st.selected.push(i);
                } else {
                    st.selected = vec![i];
                }
                st.menu.invalidate();
            }
            refresh();
        });
    }
    {
        let state = state.clone();
        dlg.on_node_double_clicked(move |i| {
            let st = state.borrow();
            let id = st.rev(i as usize);
            spawn_launcher(&st.launch("log").with("endrev", id.clone()).with("rev", id));
        });
    }
    {
        let state = state.clone();
        dlg.on_menu_for(move |_| {
            let st = state.borrow();
            st.menu.get(st.selected.len() as i32, || {
                node_menu(&st.nodes, &st.selected)
            })
        });
    }
    {
        let state = state.clone();
        let refresh = refresh.clone();
        let weak = dlg.as_weak();
        dlg.on_menu_chosen(move |_, id| {
            let Some(d) = weak.upgrade() else { return };
            handle(&state, &refresh, &d, id);
        });
    }
    {
        let state = state.clone();
        let refresh = refresh.clone();
        let weak = dlg.as_weak();
        dlg.on_git_action(move |a| {
            let Some(d) = weak.upgrade() else { return };
            let id = match a {
                1 => RgAction::CompareRevs,
                2 => RgAction::CompareHeads,
                3 => RgAction::UnidiffRevs,
                _ => RgAction::UnidiffHeads,
            };
            handle(&state, &refresh, &d, id as i32);
        });
    }
    for (toggle, which) in [(0, "b"), (1, "t")] {
        let state = state.clone();
        let refresh = refresh.clone();
        let f = move || {
            {
                let mut st = state.borrow_mut();
                if which == "b" {
                    st.options.branchings_and_merges = !st.options.branchings_and_merges;
                } else {
                    st.options.all_tags = !st.options.all_tags;
                }
                st.load();
            }
            refresh();
        };
        if toggle == 0 {
            dlg.on_toggle_branchings(f);
        } else {
            dlg.on_toggle_all_tags(f);
        }
    }
    {
        let state = state.clone();
        let refresh = refresh.clone();
        dlg.on_reload(move || {
            state.borrow_mut().load();
            refresh();
        });
    }
    {
        let weak = dlg.as_weak();
        dlg.on_toggle_arrows(move || {
            if let Some(d) = weak.upgrade() {
                d.set_arrows_to_merges(!d.get_arrows_to_merges());
            }
        });
    }
    {
        let weak = dlg.as_weak();
        dlg.on_zoom_fit(move |how| {
            let Some(d) = weak.upgrade() else { return };
            let size = d.window().size().to_logical(d.window().scale_factor());
            let zw = (size.width - 60.0) / (d.get_graph_width() + 40.0);
            let zh = (size.height - 80.0) / (d.get_graph_height() + 40.0);
            let z = match how {
                1 => zh,
                2 => zw,
                _ => zw.min(zh),
            };
            d.set_zoom(z.clamp(0.2, 4.0));
        });
    }
    {
        let weak = dlg.as_weak();
        let workdir = ctx.repo.workdir().to_owned();
        dlg.on_save_graph(move || {
            let Some(d) = weak.upgrade() else { return };
            let Some(path) =
                crate::pick::save_file("Save graph as", Some(&workdir), "revisiongraph.png")
            else {
                return;
            };
            match d.window().take_snapshot() {
                Ok(buf) => {
                    let png =
                        crate::snapshot::encode_png(buf.width(), buf.height(), buf.as_bytes());
                    if let Err(e) = std::fs::write(&path, png) {
                        d.set_status_text(format!("{}: {e}", path.display()).into());
                    }
                }
                Err(e) => d.set_status_text(e.to_string().into()),
            }
        });
    }
    {
        let weak = dlg.as_weak();
        dlg.on_exit(move || {
            if let Some(d) = weak.upgrade() {
                let _ = d.hide();
            }
        });
    }
    crate::window::prepare(&dlg);
    dlg.run()?;
    Ok(0)
}

fn compare(st: &State, a: &str, b: &str, unified: bool) {
    let mut inv = st
        .launch("showcompare")
        .with("revision1", a)
        .with("revision2", b);
    if unified {
        inv = inv.with_switch("unified");
    }
    spawn_launcher(&inv);
}

fn run_git_then_reload(
    state: &Rc<RefCell<State>>,
    refresh: &Rc<dyn Fn()>,
    title: &str,
    cmds: Vec<Vec<String>>,
) {
    let workdir = state.borrow().workdir.clone();
    let job = ProgressJob {
        title: crate::window_title(&workdir, title),
        git: koopagit_core::git_cli::Git::new(&workdir),
        commands: cmds,
        post_actions: Vec::new(),
        close_on_end: CloseOnEnd::IfNoErrors,
    };
    let state = state.clone();
    let refresh = refresh.clone();
    match crate::progress::start(job, move |_| {
        state.borrow_mut().load();
        refresh();
    }) {
        Ok(p) => keep_alive(p),
        Err(e) => eprintln!("revision graph: {e}"),
    }
}

/// Asks before deleting refs (`CAppUtils::DeleteRef`), without a nested loop.
fn confirm_delete(state: &Rc<RefCell<State>>, refresh: &Rc<dyn Fn()>, refs: Vec<RefLabel>) {
    let Ok(mb) = MessageBox::new() else { return };
    crate::window::apply_theme(&mb);
    let names: Vec<&str> = refs.iter().map(|r| r.name.as_str()).collect();
    mb.set_dialog_title("KoopaGit".into());
    mb.set_text(format!("Do you really want to delete {}?", names.join(", ")).into());
    let buttons: Vec<SharedString> = vec!["Delete".into(), "Cancel".into()];
    mb.set_buttons(ModelRc::new(VecModel::from(buttons)));
    let weak = mb.as_weak();
    let state = state.clone();
    let refresh = refresh.clone();
    mb.on_clicked(move |i| {
        if let Some(m) = weak.upgrade() {
            let _ = m.hide();
        }
        if i == 0 {
            let cmds = refs.iter().map(delete_args).collect();
            run_git_then_reload(&state, &refresh, "Delete", cmds);
        }
    });
    if mb.show().is_ok() {
        keep_alive(mb);
    }
}

fn handle(state: &Rc<RefCell<State>>, refresh: &Rc<dyn Fn()>, d: &RevisionGraphDialog, id: i32) {
    let (sel, nodes) = {
        let st = state.borrow();
        (st.selected.clone(), st.nodes.clone())
    };
    let Some(&first) = sel.first() else { return };
    let labels = nodes[first].labels.clone();
    let st = state.borrow();
    match id {
        SWITCH_BASE..DELETE_BASE => {
            if let Some(b) = switch_names(&labels).get((id - SWITCH_BASE) as usize) {
                let b = b.clone();
                drop(st);
                switch_to(state, refresh, &b);
            }
            return;
        }
        DELETE_BASE..DELETE_ALL => {
            if let Some(r) = deletable(&labels).get((id - DELETE_BASE) as usize).cloned() {
                drop(st);
                confirm_delete(state, refresh, vec![r]);
            }
            return;
        }
        DELETE_ALL => {
            drop(st);
            confirm_delete(state, refresh, deletable(&labels));
            return;
        }
        _ => {}
    }
    use crate::menus::ActionId;
    let Some(action) = RgAction::from_id(id) else {
        return;
    };
    match action {
        RgAction::ShowLog => {
            let inv = match sel.as_slice() {
                [a, b] => st
                    .launch("log")
                    .with("startrev", st.rev(*a))
                    .with("endrev", st.rev(*b)),
                _ => st
                    .launch("log")
                    .with("endrev", st.rev(first))
                    .with("rev", st.rev(first)),
            };
            spawn_launcher(&inv);
        }
        RgAction::BrowseRepo => {
            let rev = labels
                .first()
                .map(|l| l.name.clone())
                .unwrap_or_else(|| st.rev(first));
            spawn_launcher(&st.launch("repobrowser").with("rev", rev));
        }
        RgAction::Switch => {
            if let Some(b) = switch_names(&labels).first().cloned() {
                drop(st);
                switch_to(state, refresh, &b);
            }
        }
        RgAction::SwitchToRev => {
            if let Some(r) = switch_rev(&labels) {
                spawn_launcher(&st.launch("switch").with("rev", r));
            }
        }
        RgAction::CopyRefs => {
            let text = if labels.is_empty() {
                st.rev(first)
            } else {
                labels
                    .iter()
                    .map(|l| l.name.as_str())
                    .collect::<Vec<_>>()
                    .join("\r\n")
            };
            if !crate::clipboard::copy(&text) {
                d.set_status_text(text.into());
            }
        }
        RgAction::Delete => {
            let refs = deletable(&labels);
            drop(st);
            confirm_delete(state, refresh, refs);
        }
        RgAction::CompareHeads | RgAction::UnidiffHeads => compare(
            &st,
            &st.rev(first),
            "HEAD",
            action == RgAction::UnidiffHeads,
        ),
        RgAction::CompareWorkingTree => compare(&st, &st.rev(first), WORKING_TREE, false),
        RgAction::CompareRevs | RgAction::UnidiffRevs => {
            if let [a, b] = sel.as_slice() {
                compare(
                    &st,
                    &st.rev(*a),
                    &st.rev(*b),
                    action == RgAction::UnidiffRevs,
                );
            }
        }
    }
}

fn switch_to(state: &Rc<RefCell<State>>, refresh: &Rc<dyn Fn()>, branch: &str) {
    run_git_then_reload(
        state,
        refresh,
        "Switch/Checkout",
        vec![vec![
            "checkout".to_owned(),
            "--end-of-options".to_owned(),
            branch.to_owned(),
            "--".to_owned(),
        ]],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menus::Entry;

    fn titles(e: &[Entry]) -> Vec<String> {
        e.iter()
            .map(|e| match e {
                Entry::Item(l) => l.title.replace('&', ""),
                Entry::Sub {
                    title, children, ..
                } => format!(
                    "[{}: {}]",
                    title.replace('&', ""),
                    children
                        .iter()
                        .map(|c| c.title.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                ),
                Entry::Separator => "---".into(),
            })
            .collect()
    }

    fn node(id: &str, labels: &[(RefKind, &str)]) -> RgNode {
        RgNode {
            id: id.into(),
            parents: vec![],
            subject: String::new(),
            labels: labels
                .iter()
                .map(|(k, n)| RefLabel {
                    kind: *k,
                    name: (*n).into(),
                })
                .collect(),
        }
    }

    #[test]
    fn menu_follows_upstream_order_and_conditions() {
        let nodes = vec![
            node("a", &[(RefKind::LocalBranch, "dev")]),
            node(
                "b",
                &[
                    (RefKind::LocalBranch, "x"),
                    (RefKind::LocalBranch, "y"),
                    (RefKind::Tag, "v1"),
                ],
            ),
            node("c", &[(RefKind::RemoteBranch, "origin/z")]),
            node("d", &[]),
        ];
        assert_eq!(
            titles(&node_menu(&nodes, &[0])),
            [
                "Show log",
                "Browse repository",
                "Switch/Checkout to \"dev\"",
                "Copy ref names",
                "Delete dev",
                "Compare HEAD revisions",
                "Unified diff of HEAD revisions",
                "Compare with working tree"
            ]
        );
        let b = titles(&node_menu(&nodes, &[1]));
        assert!(b.contains(&"[Switch/Checkout to: x,y]".to_owned()));
        assert!(b.contains(&"[Delete branch/tag: x,y,v1,All]".to_owned()));
        assert!(
            titles(&node_menu(&nodes, &[2])).contains(&"Switch/Checkout to this...".to_owned())
        );
        assert_eq!(
            titles(&node_menu(&nodes, &[0, 3])),
            ["Show log", "Compare revisions", "Unified diff"]
        );
    }

    #[test]
    fn model_positions_and_edges() {
        let mut a = node("a", &[(RefKind::CurrentBranch, "main")]);
        a.parents = vec!["b".into()];
        let b = node("b", &[]);
        let (g, edges, w, h) = graph_model(&[a, b], &[1]);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].kind, 1);
        assert!(g[1].selected && !g[0].selected);
        assert!(g[1].y > g[0].y);
        assert!(edges.starts_with("M 85 46 L 85 72"));
        assert!(w >= NODE_W && h > NODE_H);
    }
}
