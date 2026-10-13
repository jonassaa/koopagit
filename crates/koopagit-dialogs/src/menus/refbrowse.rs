//! Browse References tree and list menus (`CBrowseRefsDlg::ShowContextMenu`,
//! spec-menus.md §4.2).

use super::{ActionId, Entry, Menu};

menu_actions! {
    pub enum RefAction {
        ViewLog = 1,
        RepoBrowser = 2,
        ShowReflog = 3,
        DiffWc = 4,
        Fetch = 5,
        Merge = 6,
        Switch = 7,
        CreateBranch = 8,
        EditBranchDescription = 9,
        Rename = 10,
        UpstreamDrop = 11,
        UpstreamSet = 12,
        Diff = 13,
        UnifiedDiff = 14,
        ViewLogRange = 15,
        ViewLogRangeReachableFromOnlyOne = 16,
        DeleteRemoteBranch = 17,
        DeleteBranch = 18,
        DeleteTag = 19,
        ManageRemotes = 20,
        /// `| remote_index << 16`.
        DeleteRemoteTag = 21,
        CreateTag = 22,
        DeleteAllTags = 23,
        Copy = 24,
    }
}

/// Encodes a remote index into an action id (upstream's `eCmd | idx << 16`).
pub fn with_index(a: RefAction, idx: usize) -> i32 {
    a.id() | ((idx as i32) << 16)
}

/// Splits an encoded id into the action and the index.
pub fn decode(id: i32) -> Option<(RefAction, usize)> {
    Some((RefAction::from_id(id & 0xFFFF)?, (id >> 16) as usize))
}

/// One selected ref row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RefLeaf {
    /// Full ref name, e.g. `refs/heads/main`.
    pub full: String,
    /// The ref points at a commit (an annotated tag does not).
    pub is_commit: bool,
    /// A tracked upstream is configured (local branches).
    pub has_upstream: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RefMenuInput {
    /// Selected list rows; the last one is the last-clicked ref.
    pub leaves: Vec<RefLeaf>,
    /// The tree node, e.g. `refs/remotes/origin` (used when no leaf is selected).
    pub node: Option<String>,
    pub has_wc: bool,
    pub current_branch: String,
    /// Configured remotes.
    pub remotes: Vec<String>,
}

/// `refs/remotes/<remote>/<branch>` split on a configured remote (longest match).
pub fn split_remote<'a>(full: &'a str, remotes: &[String]) -> Option<(&'a str, &'a str)> {
    let rest = full.strip_prefix("refs/remotes/")?;
    remotes
        .iter()
        .filter_map(|r| {
            let b = rest.strip_prefix(r.as_str())?.strip_prefix('/')?;
            Some((&rest[..r.len()], b))
        })
        .max_by_key(|(r, _)| r.len())
}

/// Short display name (`StripRefName`): drops `refs/heads/`, else `refs/`.
pub fn strip_ref_name(full: &str) -> &str {
    full.strip_prefix("refs/heads/")
        .or_else(|| full.strip_prefix("refs/"))
        .unwrap_or(full)
}

pub fn build(i: &RefMenuInput) -> Vec<Entry> {
    use RefAction as A;
    let mut m = Menu::new();
    let leaves = &i.leaves;
    if leaves.len() == 1 {
        let l = &leaves[0];
        let is_head = l.full.starts_with("refs/heads/");
        let is_remote = l.full.starts_with("refs/remotes/");
        m.item_if(l.is_commit, A::ViewLog, "Show &log");
        m.item(A::RepoBrowser, "&Browse repository");
        m.item_if(is_head || is_remote, A::ShowReflog, "Show &RefLog");
        if i.has_wc && l.is_commit {
            m.sep();
            m.item(A::DiffWc, "Compare with &working tree");
        }
        m.sep();
        if let Some((remote, _)) = split_remote(&l.full, &i.remotes) {
            m.item(A::Fetch, format!("Fetch from \"{remote}\""));
            m.sep();
        }
        m.item_if(
            i.has_wc && l.is_commit && l.full != format!("refs/heads/{}", i.current_branch),
            A::Merge,
            format!("&Merge to \"{}\"...", i.current_branch),
        );
        if i.has_wc && l.is_commit {
            m.item(A::Switch, "Sw&itch/Checkout to this...");
            m.sep();
        }
        m.item_if(is_remote, A::CreateBranch, "Create &Branch...");
        m.item_if(
            is_head && l.is_commit,
            A::EditBranchDescription,
            "Edit description",
        );
        m.item_if(is_head, A::Rename, "Rename");
        if i.has_wc && l.is_commit && is_head {
            m.sep();
            m.item_if(l.has_upstream, A::UpstreamDrop, "Unset tracked branch");
            m.item(A::UpstreamSet, "Select tracked branch");
        }
    } else if leaves.len() == 2 {
        let a = strip_ref_name(&leaves[0].full);
        let b = strip_ref_name(&leaves[1].full);
        m.item(A::Diff, "Compare selected refs");
        m.item(A::UnifiedDiff, "Show changes as &unified diff");
        m.item(A::ViewLogRange, format!("Show log of {a}..{b}"));
        m.item(
            A::ViewLogRangeReachableFromOnlyOne,
            format!("Show log of {a}...{b}"),
        );
    }
    if !leaves.is_empty() {
        let all = |p: &str| leaves.iter().all(|l| l.full.starts_with(p));
        let n = leaves.len();
        m.sep();
        if all("refs/remotes/") {
            m.item(
                A::DeleteRemoteBranch,
                if n == 1 {
                    "Delete remote branch".to_owned()
                } else {
                    format!("Delete {n} remote branches")
                },
            );
        } else if all("refs/heads/") {
            m.item(
                A::DeleteBranch,
                if n == 1 {
                    "Delete branch".to_owned()
                } else {
                    format!("Delete {n} branches")
                },
            );
        } else if all("refs/tags/") {
            m.item(
                A::DeleteTag,
                if n == 1 {
                    "Delete tag".to_owned()
                } else {
                    format!("Delete {n} tags")
                },
            );
        }
    } else if let Some(node) = &i.node {
        let remote_idx = node
            .strip_prefix("refs/remotes/")
            .and_then(|r| i.remotes.iter().position(|x| x == r));
        if node == "refs/remotes" || node.starts_with("refs/remotes/") {
            m.item(A::ManageRemotes, "Manage Remotes");
        }
        if let Some(idx) = remote_idx {
            let r = &i.remotes[idx];
            m.raw(
                with_index(A::Fetch, idx),
                format!("Fetch from \"{r}\""),
                true,
            );
            m.raw(
                with_index(A::DeleteRemoteTag, idx),
                "Delete remote tags...",
                true,
            );
        }
        if node == "refs/heads" || node.starts_with("refs/heads/") {
            m.item(A::CreateBranch, "Create &Branch...");
        }
        if node == "refs/tags" || node.starts_with("refs/tags/") {
            m.item(A::CreateTag, "Create &Tag...");
            m.item(A::DeleteAllTags, "Delete all tags");
            if !i.remotes.is_empty() {
                m.sep();
                for (idx, r) in i.remotes.iter().enumerate() {
                    m.raw(
                        with_index(A::DeleteRemoteTag, idx),
                        format!("Delete remote tags on \"{r}\"..."),
                        true,
                    );
                }
            }
        }
    }
    m.sep();
    m.item(A::Copy, "Copy ref names");
    m.finish()
}

/// The delete confirmation (`ConfirmDeleteRef`). `unmerged` tells whether the
/// single branch is not fast-forward to HEAD.
pub fn confirm_delete_text(leaves: &[RefLeaf], unmerged: bool) -> String {
    let remote = leaves
        .first()
        .is_some_and(|l| l.full.starts_with("refs/remotes/"));
    let branch = remote
        || leaves
            .first()
            .is_some_and(|l| l.full.starts_with("refs/heads/"));
    const UNMERGED: &str = "This branch is not fully merged into HEAD.";
    const NO_CHECK: &str =
        "It has not been checked if these branches have been fully merged into HEAD.";
    const REMOTE: &str = "This action will remove the branches on the remote.";
    let many = format!(
        "Do you really want to permanently delete the {} selected refs? It can NOT be recovered!",
        leaves.len()
    );
    if leaves.len() == 1 {
        let full = &leaves[0].full;
        let name = full
            .strip_prefix("refs/remotes/")
            .or_else(|| full.strip_prefix("refs/heads/"))
            .or_else(|| full.strip_prefix("refs/tags/"))
            .unwrap_or(full);
        let mut s = format!("Do you really want to delete \"{name}\"?");
        if branch && unmerged {
            s.push_str("\n\n");
            s.push_str(UNMERGED);
        }
        if remote {
            s.push_str("\n\n");
            s.push_str(REMOTE);
        }
        s
    } else if branch {
        let mut s = format!("{many}\n\n{NO_CHECK}");
        if remote {
            s.push_str("\n\n");
            s.push_str(REMOTE);
        }
        s
    } else {
        many
    }
}

/// Command lines deleting the leaves: `git branch -D -- <b>`, `git tag -d -- <t>`,
/// or one `git push -- <remote> :refs/heads/<b>...` per remote.
pub fn delete_commands(leaves: &[RefLeaf], remotes: &[String]) -> Vec<Vec<String>> {
    let mut cmds = Vec::new();
    let mut by_remote: Vec<(String, Vec<String>)> = Vec::new();
    for l in leaves {
        if let Some(b) = l.full.strip_prefix("refs/heads/") {
            cmds.push(vec!["branch".into(), "-D".into(), "--".into(), b.into()]);
        } else if let Some(t) = l.full.strip_prefix("refs/tags/") {
            cmds.push(vec!["tag".into(), "-d".into(), "--".into(), t.into()]);
        } else if let Some((r, b)) = split_remote(&l.full, remotes) {
            let spec = format!(":refs/heads/{b}");
            match by_remote.iter_mut().find(|(x, _)| x == r) {
                Some((_, v)) => v.push(spec),
                None => by_remote.push((r.to_owned(), vec![spec])),
            }
        }
    }
    for (r, specs) in by_remote {
        let mut c = vec!["push".to_owned(), "--".to_owned(), r];
        c.extend(specs);
        cmds.push(c);
    }
    cmds
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(e: &[Entry]) -> Vec<String> {
        e.iter()
            .map(|e| match e {
                Entry::Item(l) => l.title.replace('&', ""),
                Entry::Separator => "---".into(),
                Entry::Sub { title, .. } => format!("[{title}]"),
            })
            .collect()
    }

    fn leaf(full: &str) -> RefLeaf {
        RefLeaf {
            full: full.into(),
            is_commit: true,
            has_upstream: false,
        }
    }

    fn input(leaves: Vec<RefLeaf>) -> RefMenuInput {
        RefMenuInput {
            leaves,
            node: None,
            has_wc: true,
            current_branch: "main".into(),
            remotes: vec!["origin".into(), "origin/x".into()],
        }
    }

    #[test]
    fn local_branch_menu() {
        let mut l = leaf("refs/heads/feature");
        l.has_upstream = true;
        assert_eq!(
            titles(&build(&input(vec![l]))),
            [
                "Show log",
                "Browse repository",
                "Show RefLog",
                "---",
                "Compare with working tree",
                "---",
                "Merge to \"main\"...",
                "Switch/Checkout to this...",
                "---",
                "Edit description",
                "Rename",
                "---",
                "Unset tracked branch",
                "Select tracked branch",
                "---",
                "Delete branch",
                "---",
                "Copy ref names",
            ]
        );
        let cur = titles(&build(&input(vec![leaf("refs/heads/main")])));
        assert!(!cur.iter().any(|t| t.starts_with("Merge")));
    }

    #[test]
    fn remote_branch_and_annotated_tag() {
        let t = titles(&build(&input(vec![leaf("refs/remotes/origin/x/dev")])));
        assert!(t.contains(&"Fetch from \"origin/x\"".to_owned()), "{t:?}");
        assert!(t.contains(&"Create Branch...".to_owned()));
        assert!(t.contains(&"Delete remote branch".to_owned()));
        let mut tag = leaf("refs/tags/v1");
        tag.is_commit = false;
        let t = titles(&build(&input(vec![tag])));
        assert_eq!(
            t,
            [
                "Browse repository",
                "---",
                "Delete tag",
                "---",
                "Copy ref names"
            ]
        );
    }

    #[test]
    fn two_leaves_and_tree_nodes() {
        let t = titles(&build(&input(vec![
            leaf("refs/heads/a"),
            leaf("refs/heads/b"),
        ])));
        assert_eq!(
            t,
            [
                "Compare selected refs",
                "Show changes as unified diff",
                "Show log of a..b",
                "Show log of a...b",
                "---",
                "Delete 2 branches",
                "---",
                "Copy ref names",
            ]
        );
        let mixed = titles(&build(&input(vec![
            leaf("refs/heads/a"),
            leaf("refs/tags/b"),
        ])));
        assert!(!mixed.iter().any(|t| t.starts_with("Delete")));
        let mut node = input(vec![]);
        node.node = Some("refs/remotes/origin".into());
        assert_eq!(
            titles(&build(&node)),
            [
                "Manage Remotes",
                "Fetch from \"origin\"",
                "Delete remote tags...",
                "---",
                "Copy ref names"
            ]
        );
        node.node = Some("refs/tags".into());
        let t = titles(&build(&node));
        assert_eq!(t[..3], ["Create Tag...", "Delete all tags", "---"]);
        assert!(t.contains(&"Delete remote tags on \"origin/x\"...".to_owned()));
        assert_eq!(
            decode(with_index(RefAction::DeleteRemoteTag, 1)),
            Some((RefAction::DeleteRemoteTag, 1))
        );
    }

    #[test]
    fn delete_texts_and_commands() {
        let remotes = vec!["origin".to_owned()];
        let one = [leaf("refs/remotes/origin/dev")];
        assert_eq!(
            confirm_delete_text(&one, true),
            "Do you really want to delete \"origin/dev\"?\n\nThis branch is not fully merged into HEAD.\n\nThis action will remove the branches on the remote."
        );
        let two = [leaf("refs/heads/a"), leaf("refs/heads/b")];
        assert!(confirm_delete_text(&two, false).contains("It has not been checked"));
        let cmds = delete_commands(
            &[
                leaf("refs/remotes/origin/a"),
                leaf("refs/remotes/origin/b"),
                leaf("refs/heads/c"),
                leaf("refs/tags/v1"),
            ],
            &remotes,
        );
        let lines: Vec<String> = cmds.iter().map(|c| c.join(" ")).collect();
        assert_eq!(
            lines,
            [
                "branch -D -- c",
                "tag -d -- v1",
                "push -- origin :refs/heads/a :refs/heads/b"
            ]
        );
        assert_eq!(strip_ref_name("refs/heads/x"), "x");
        assert_eq!(strip_ref_name("refs/tags/v"), "tags/v");
    }
}
