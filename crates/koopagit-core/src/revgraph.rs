//! Revision graph data (`CRevisionGraphWnd::FetchRevisionData`): labeled
//! commits, merges and fork points, with straight single-parent chains folded.

use std::collections::{HashMap, HashSet};

use crate::log::{RefKind, RefLabel};
use crate::{Repo, Result};

/// One node of the revision graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgNode {
    pub id: String,
    pub parents: Vec<String>,
    pub subject: String,
    pub labels: Vec<RefLabel>,
}

/// Which history the graph shows (filter dialog).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RgScope {
    #[default]
    AllBranches,
    LocalBranches,
    CurrentBranch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RgOptions {
    pub scope: RgScope,
    /// "Show branchings and merges" (`--sparse`).
    pub branchings_and_merges: bool,
    /// "Show all tags": tag-only commits stay visible.
    pub all_tags: bool,
}

impl Default for RgOptions {
    fn default() -> Self {
        RgOptions {
            scope: RgScope::AllBranches,
            branchings_and_merges: false,
            all_tags: true,
        }
    }
}

/// `git log` arguments for the graph's input.
pub fn log_args(o: &RgOptions) -> Vec<String> {
    let mut a: Vec<String> = [
        "log",
        "--simplify-by-decoration",
        // Rewrite parents to the simplified history (otherwise %P names
        // commits that are not in the graph).
        "--parents",
        "--topo-order",
        "-z",
        "--format=%H%x1f%P%x1f%s",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    if o.branchings_and_merges {
        a.push("--sparse".to_owned());
    }
    match o.scope {
        RgScope::AllBranches => a.push("--all".to_owned()),
        RgScope::LocalBranches => a.push("--branches".to_owned()),
        RgScope::CurrentBranch => a.push("HEAD".to_owned()),
    }
    a
}

/// Parses the output of [`log_args`].
pub fn parse_log(out: &str, labels: &HashMap<String, Vec<RefLabel>>) -> Vec<RgNode> {
    out.split('\0')
        .map(|r| r.trim_start_matches('\n'))
        .filter(|r| !r.is_empty())
        .filter_map(|r| {
            let mut f = r.splitn(3, '\x1f');
            let id = f.next()?.to_owned();
            let parents = f
                .next()
                .unwrap_or_default()
                .split_whitespace()
                .map(str::to_owned)
                .collect();
            let subject = f.next().unwrap_or_default().to_owned();
            Some(RgNode {
                labels: labels.get(&id).cloned().unwrap_or_default(),
                id,
                parents,
                subject,
            })
        })
        .collect()
}

/// Folds commits that carry no (relevant) label, have one parent and one
/// child, whose child has one parent; the child is re-linked to the folded
/// commit's parent. Applied when tags are hidden or "branchings and merges" is
/// on (`RevisionGraphDlgFunc.cpp:245-310`).
pub fn simplify(mut nodes: Vec<RgNode>, o: &RgOptions) -> Vec<RgNode> {
    if o.all_tags && !o.branchings_and_merges {
        return nodes;
    }
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    for n in &nodes {
        for p in &n.parents {
            children.entry(p.clone()).or_default().push(n.id.clone());
        }
    }
    let index: HashMap<String, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.clone(), i))
        .collect();
    let mut skip = HashSet::new();
    for i in 0..nodes.len() {
        let n = &nodes[i];
        let keep_label = n
            .labels
            .iter()
            .any(|l| o.all_tags || l.kind != RefKind::Tag);
        if keep_label || n.parents.len() != 1 {
            continue;
        }
        let Some(kids) = children.get(&n.id) else {
            continue;
        };
        if kids.len() != 1 {
            continue;
        }
        let child_id = kids[0].clone();
        let Some(&ci) = index.get(&child_id) else {
            continue;
        };
        if nodes[ci].parents.len() != 1 {
            continue;
        }
        let parent = n.parents[0].clone();
        let id = n.id.clone();
        skip.insert(id.clone());
        nodes[ci].parents[0] = parent.clone();
        if let Some(pc) = children.get_mut(&parent) {
            if let Some(slot) = pc.iter_mut().find(|c| **c == id) {
                *slot = child_id.clone();
            }
        }
        children.remove(&id);
    }
    nodes.retain(|n| !skip.contains(&n.id));
    nodes
}

/// Node placement: `(row, column)` per node (row 0 = newest), edges as
/// `(child, parent)` node indices.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RgLayout {
    pub cells: Vec<(usize, usize)>,
    pub edges: Vec<(usize, usize)>,
    pub columns: usize,
}

pub fn layout(nodes: &[RgNode]) -> RgLayout {
    let rows = crate::history::graph(nodes.iter().map(|n| (n.id.as_str(), n.parents.as_slice())));
    let index: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let cells: Vec<(usize, usize)> = rows.iter().enumerate().map(|(r, g)| (r, g.node)).collect();
    let edges = nodes
        .iter()
        .enumerate()
        .flat_map(|(i, n)| {
            n.parents
                .iter()
                .filter_map(|p| index.get(p.as_str()).map(|&pi| (i, pi)))
                .collect::<Vec<_>>()
        })
        .collect();
    RgLayout {
        columns: rows.iter().map(|r| r.width).max().unwrap_or(1).max(1),
        cells,
        edges,
    }
}

impl Repo {
    /// Loads and simplifies the revision graph.
    pub fn revision_graph(&self, o: &RgOptions) -> Result<Vec<RgNode>> {
        let out = self.git().run(log_args(o))?;
        Ok(simplify(parse_log(&out, &self.ref_labels()?), o))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, parents: &[&str], labels: &[(RefKind, &str)]) -> RgNode {
        RgNode {
            id: id.into(),
            parents: parents.iter().map(|p| (*p).into()).collect(),
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
    fn folds_unlabeled_chains_but_keeps_forks_and_tags() {
        // d(main) -> c -> b(tag v1) -> a ; e(feature) -> b
        let nodes = vec![
            node("d", &["c"], &[(RefKind::LocalBranch, "main")]),
            node("e", &["b"], &[(RefKind::LocalBranch, "feature")]),
            node("c", &["b"], &[]),
            node("b", &["a"], &[(RefKind::Tag, "v1")]),
            node("a", &[], &[]),
        ];
        let all = simplify(nodes.clone(), &RgOptions::default());
        assert_eq!(all.len(), 5, "all tags without sparse folds nothing");
        let hide_tags = RgOptions {
            all_tags: false,
            ..Default::default()
        };
        let s = simplify(nodes, &hide_tags);
        let ids: Vec<&str> = s.iter().map(|n| n.id.as_str()).collect();
        // c folds (one parent, one child with one parent); b is a fork point.
        assert_eq!(ids, ["d", "e", "b", "a"]);
        assert_eq!(s[0].parents, ["b"]);
        let l = layout(&s);
        assert_eq!(l.cells.len(), 4);
        assert!(l.edges.contains(&(0, 2)) && l.edges.contains(&(1, 2)));
        assert_eq!(l.columns, 2);
    }

    #[test]
    fn real_repository() {
        let (dir, repo) = crate::repo::tests::fixture();
        let git = repo.git();
        for i in 0..3 {
            std::fs::write(dir.path().join("a.txt"), format!("{i}\n")).unwrap();
            git.run(["commit", "-qam", &format!("c{i}")]).unwrap();
        }
        git.run(["tag", "v1", "HEAD~1"]).unwrap();
        let g = repo.revision_graph(&RgOptions::default()).unwrap();
        assert!(g.iter().any(|n| n.labels.iter().any(|l| l.name == "v1")));
        assert!(g.len() <= 3, "{g:?}");
        let sparse = RgOptions {
            branchings_and_merges: true,
            ..Default::default()
        };
        assert!(log_args(&sparse).contains(&"--sparse".to_owned()));

        // A merge whose first parent is undecorated: its parents must be
        // rewritten to commits that are in the graph.
        git.run(["checkout", "-qb", "side"]).unwrap();
        std::fs::write(dir.path().join("s.txt"), "s\n").unwrap();
        git.run(["add", "s.txt"]).unwrap();
        git.run(["commit", "-qm", "side"]).unwrap();
        git.run(["checkout", "-q", "main"]).unwrap();
        std::fs::write(dir.path().join("m.txt"), "m\n").unwrap();
        git.run(["add", "m.txt"]).unwrap();
        git.run(["commit", "-qm", "main work"]).unwrap();
        git.run(["merge", "-q", "--no-ff", "side", "-m", "merge"])
            .unwrap();
        let g = repo.revision_graph(&RgOptions::default()).unwrap();
        let ids: Vec<&str> = g.iter().map(|n| n.id.as_str()).collect();
        for n in &g {
            for p in &n.parents {
                assert!(ids.contains(&p.as_str()), "dangling parent {p} in {g:?}");
            }
        }
        assert_eq!(g[0].labels[0].name, "main", "{g:?}");
    }
}
