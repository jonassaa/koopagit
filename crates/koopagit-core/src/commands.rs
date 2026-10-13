//! Git command lines exactly as TortoiseGit builds them (`CAppUtils::Do*`).
//!
//! Each builder returns the argument vector after `git`. TortoiseGit quotes
//! values into a single command string; we pass argv directly, so quoting is
//! unnecessary but argument order and flags match upstream.

/// A tri-state checkbox: checked, unchecked, or indeterminate ("use default").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TriState {
    On,
    Off,
    #[default]
    Default,
}

impl TriState {
    fn push_flag(self, args: &mut Vec<String>, on: &str, off: &str) {
        match self {
            TriState::On => args.push(on.to_owned()),
            TriState::Off => args.push(off.to_owned()),
            TriState::Default => {}
        }
    }
}

fn s(v: &str) -> String {
    v.to_owned()
}

/// `push.recurseSubmodules` / the Push dialog's "Recurse submodule" combo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RecurseSubmodules {
    #[default]
    None,
    Check,
    OnDemand,
}

impl RecurseSubmodules {
    fn as_arg(self) -> &'static str {
        match self {
            RecurseSubmodules::None => "no",
            RecurseSubmodules::Check => "check",
            RecurseSubmodules::OnDemand => "on-demand",
        }
    }
}

/// Options of the Push dialog (`CPushDlg`).
#[derive(Debug, Clone, Default)]
pub struct PushOptions {
    /// Remote name or arbitrary URL.
    pub remote: String,
    /// `&Local:` ref; empty with a remote branch deletes it.
    pub local: String,
    /// `&Remote:` branch name.
    pub remote_branch: String,
    pub all_branches: bool,
    pub tags: bool,
    pub force: bool,
    pub force_with_lease: bool,
    pub set_upstream: bool,
    pub recurse_submodules: RecurseSubmodules,
    /// The value from git config; the flag is only passed when it differs.
    pub configured_recurse_submodules: RecurseSubmodules,
    pub push_option: String,
}

/// `CAppUtils::DoPush`. Returns one or two command lines (push-all with tags
/// needs a second `push --tags`).
pub fn push(o: &PushOptions) -> Vec<Vec<String>> {
    let mut arg = Vec::new();
    if o.tags && !o.all_branches {
        arg.push(s("--tags"));
    }
    if o.force {
        arg.push(s("--force"));
    }
    if o.force_with_lease {
        arg.push(s("--force-with-lease"));
    }
    if o.set_upstream {
        arg.push(s("--set-upstream"));
    }
    if o.recurse_submodules != o.configured_recurse_submodules {
        arg.push(format!(
            "--recurse-submodules={}",
            o.recurse_submodules.as_arg()
        ));
    }
    if !o.push_option.is_empty() {
        arg.push(format!("--push-option={}", o.push_option));
    }
    arg.push(s("--progress"));

    if o.all_branches {
        let mut cmds = vec![
            [
                vec![s("push"), s("--all")],
                arg.clone(),
                vec![s("--"), o.remote.clone()],
            ]
            .concat(),
        ];
        if o.tags {
            cmds.push(
                [
                    vec![s("push"), s("--tags")],
                    arg,
                    vec![s("--"), o.remote.clone()],
                ]
                .concat(),
            );
        }
        return cmds;
    }
    let mut cmd = [vec![s("push")], arg, vec![s("--"), o.remote.clone()]].concat();
    if !o.local.is_empty() || !o.remote_branch.is_empty() {
        let mut refspec = o.local.clone();
        if !o.remote_branch.is_empty() {
            refspec.push(':');
            refspec.push_str(&o.remote_branch);
        }
        cmd.push(refspec);
    }
    vec![cmd]
}

/// Options of the Pull/Fetch dialog (`CPullFetchDlg`).
#[derive(Debug, Clone, Default)]
pub struct PullFetchOptions {
    pub remote: String,
    pub remote_branch: String,
    pub squash: bool,
    pub no_commit: bool,
    pub no_ff: bool,
    pub ff_only: bool,
    pub depth: Option<u32>,
    pub tags: TriState,
    pub prune: TriState,
    pub allow_unrelated_histories: bool,
    /// Fetch only: `- all -` remotes.
    pub all_remotes: bool,
    /// Fetch only: registry `FetchVerbose` (default true).
    pub verbose: bool,
}

/// `CAppUtils::DoPull` (without "Launch Rebase After Fetch").
pub fn pull(o: &PullFetchOptions) -> Vec<String> {
    let mut a = vec![s("pull"), s("--progress"), s("-v"), s("--no-rebase")];
    o.tags.push_flag(&mut a, "--tags", "--no-tags");
    if o.no_ff {
        a.push(s("--no-ff"));
    }
    if o.ff_only {
        a.push(s("--ff-only"));
    }
    if o.squash {
        a.push(s("--squash"));
    }
    if o.no_commit {
        a.push(s("--no-commit"));
    }
    if let Some(d) = o.depth {
        a.extend([s("--depth"), d.to_string()]);
    }
    o.prune.push_flag(&mut a, "--prune", "--no-prune");
    if o.allow_unrelated_histories {
        a.push(s("--allow-unrelated-histories"));
    }
    a.extend([s("--"), o.remote.clone()]);
    if !o.remote_branch.is_empty() {
        a.push(o.remote_branch.clone());
    }
    a
}

/// `CAppUtils::DoFetch`.
pub fn fetch(o: &PullFetchOptions) -> Vec<String> {
    let mut a = vec![s("fetch")];
    if o.all_remotes {
        a.push(s("--all"));
    }
    a.push(s("--progress"));
    if o.verbose {
        a.push(s("-v"));
    }
    if let Some(d) = o.depth {
        a.extend([s("--depth"), d.to_string()]);
    }
    o.prune.push_flag(&mut a, "--prune", "--no-prune");
    o.tags.push_flag(&mut a, "--tags", "--no-tags");
    if !o.all_remotes {
        a.extend([s("--"), o.remote.clone()]);
        if !o.remote_branch.is_empty() {
            a.push(o.remote_branch.clone());
        }
    }
    a
}

/// Options of the Clone dialog (`CCloneDlg`).
#[derive(Debug, Clone, Default)]
pub struct CloneOptions {
    pub url: String,
    pub directory: String,
    pub recursive: bool,
    pub bare: bool,
    pub no_checkout: bool,
    pub branch: Option<String>,
    pub origin: Option<String>,
    pub depth: Option<u32>,
}

/// `CloneCommand::Execute`.
pub fn clone(o: &CloneOptions) -> Vec<String> {
    let mut a = vec![s("clone"), s("--progress")];
    if o.recursive {
        a.push(s("--recursive"));
    }
    if o.bare {
        a.push(s("--bare"));
    }
    if o.no_checkout {
        a.push(s("--no-checkout"));
    }
    if let Some(b) = &o.branch {
        a.extend([s("--branch"), b.clone()]);
    }
    if let Some(r) = &o.origin {
        a.extend([s("--origin"), r.clone()]);
    }
    if let Some(d) = o.depth {
        a.extend([s("--depth"), d.to_string()]);
    }
    a.extend([s("-v"), s("--"), o.url.clone(), o.directory.clone()]);
    a
}

/// Options of the Switch/Checkout dialog (`CGitSwitchDlg`).
#[derive(Debug, Clone, Default)]
pub struct SwitchOptions {
    pub version: String,
    pub new_branch: Option<String>,
    pub force: bool,
    pub merge: bool,
    pub track: TriState,
    /// "Override branch if exists" (`-B` instead of `-b`).
    pub override_branch: bool,
}

/// `CAppUtils::PerformSwitch`.
pub fn switch(o: &SwitchOptions) -> Vec<String> {
    let mut a = vec![s("checkout")];
    if o.force {
        a.push(s("-f"));
    }
    let version = match o.version.strip_prefix("refs/heads/") {
        Some(short) if !o.override_branch => short.to_owned(),
        _ => o.version.clone(),
    };
    if let Some(b) = &o.new_branch {
        o.track.push_flag(&mut a, "--track", "--no-track");
        a.push(s(if o.override_branch { "-B" } else { "-b" }));
        a.push(b.clone());
    }
    if o.merge {
        a.push(s("--merge"));
    }
    a.extend([s("--end-of-options"), version, s("--")]);
    a
}

/// The default new-branch name the Switch dialog proposes for a ref
/// (`CGitSwitchDlg::SetDefaultName`): the part after the remote for remote
/// branches (`true`: create-new-branch is checked), else `Branch_<version>`.
pub fn switch_default_branch_name(version: &str, remotes: &[String]) -> (String, bool) {
    let rest = version.strip_prefix("remotes/").unwrap_or(version);
    for r in remotes {
        if let Some(b) = rest.strip_prefix(&format!("{r}/")) {
            if version.starts_with("remotes/") || r == "origin" {
                return (b.to_owned(), true);
            }
        }
    }
    (format!("Branch_{version}"), false)
}

/// `CAppUtils::CreateBranchTag` for a branch.
pub fn create_branch(name: &str, base: &str, force: bool, track: TriState) -> Vec<String> {
    let mut a = vec![s("branch")];
    if force {
        a.push(s("-f"));
    }
    track.push_flag(&mut a, "--track", "--no-track");
    a.extend([s("--"), s(name), s(base)]);
    a
}

/// `CAppUtils::CreateBranchTag` for a tag. `message_file` holds the message
/// (annotated tag) when the message is non-empty.
pub fn create_tag(
    name: &str,
    base: &str,
    force: bool,
    sign: bool,
    message_file: Option<&str>,
) -> Vec<String> {
    let mut a = vec![s("tag")];
    if force {
        a.push(s("-f"));
    }
    if sign {
        a.push(s("-s"));
    }
    if let Some(f) = message_file {
        a.extend([s("-F"), s(f)]);
    }
    a.extend([s("--"), s(name), s(base)]);
    a
}

/// Options of the Merge dialog (`CMergeDlg`).
#[derive(Debug, Clone, Default)]
pub struct MergeOptions {
    pub version: String,
    pub no_ff: bool,
    pub ff_only: bool,
    pub squash: bool,
    pub no_commit: bool,
    pub allow_unrelated_histories: bool,
    pub log: Option<u32>,
    pub strategy: Option<String>,
    pub strategy_option: Option<String>,
    pub strategy_param: Option<String>,
    pub message_file: Option<String>,
}

/// `CAppUtils::DoMerge`.
pub fn merge(o: &MergeOptions) -> Vec<String> {
    let mut a = vec![s("merge")];
    if o.no_ff {
        a.push(s("--no-ff"));
    } else if o.ff_only {
        a.push(s("--ff-only"));
    }
    if o.squash {
        a.push(s("--squash"));
    }
    if o.no_commit {
        a.push(s("--no-commit"));
    }
    if o.allow_unrelated_histories {
        a.push(s("--allow-unrelated-histories"));
    }
    if let Some(n) = o.log {
        a.push(format!("--log={n}"));
    }
    if let Some(st) = &o.strategy {
        a.push(format!("--strategy={st}"));
        if st == "recursive" {
            if let Some(opt) = &o.strategy_option {
                match (&o.strategy_param, opt.as_str()) {
                    (Some(p), "rename-threshold" | "subtree") => {
                        a.push(format!("--strategy-option={opt}={p}"))
                    }
                    _ => a.push(format!("--strategy-option={opt}")),
                }
            }
        }
    }
    if let Some(f) = &o.message_file {
        a.extend([s("-F"), f.clone()]);
    }
    a.extend([s("--"), o.version.clone()]);
    a
}

/// Reset types of the Reset / Abort Merge dialogs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResetType {
    Soft,
    #[default]
    Mixed,
    Hard,
    /// Abort Merge's default: `git reset --merge`.
    Merge,
}

/// `CAppUtils::Reset`.
pub fn reset(kind: ResetType, rev: &str) -> Vec<String> {
    let flag = match kind {
        ResetType::Soft => "--soft",
        ResetType::Mixed => "--mixed",
        ResetType::Hard => "--hard",
        ResetType::Merge => return vec![s("reset"), s("--merge")],
    };
    vec![s("reset"), s(flag), s("--end-of-options"), s(rev), s("--")]
}

/// Untracked-file handling of the Stash dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StashUntracked {
    #[default]
    None,
    IncludeUntracked,
    All,
}

/// `CAppUtils::StashSave`.
pub fn stash_save(message: &str, untracked: StashUntracked) -> Vec<String> {
    let mut a = vec![s("stash"), s("push")];
    match untracked {
        StashUntracked::None => {}
        StashUntracked::IncludeUntracked => a.push(s("--include-untracked")),
        StashUntracked::All => a.push(s("--all")),
    }
    if !message.is_empty() {
        a.extend([s("-m"), s(message)]);
    }
    a
}

/// `CAppUtils::StashApply`: `stash{N}` is normalised to `stash@{N}`.
pub fn stash_apply(stash_ref: &str) -> Vec<String> {
    let mut a = vec![s("stash"), s("apply")];
    let r = stash_ref.strip_prefix("refs/").unwrap_or(stash_ref);
    if !r.is_empty() {
        a.push(normalize_stash_ref(r));
    }
    a
}

/// `CAppUtils::StashPop`.
pub fn stash_pop() -> Vec<String> {
    vec![s("stash"), s("pop")]
}

/// RefLog dialog "Delete" on a stash entry.
pub fn stash_drop(stash_ref: &str) -> Vec<String> {
    vec![
        s("stash"),
        s("drop"),
        s("--"),
        normalize_stash_ref(stash_ref),
    ]
}

fn normalize_stash_ref(r: &str) -> String {
    match r.strip_prefix("stash{") {
        Some(rest) => format!("stash@{{{rest}"),
        None => r.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(v: &[String]) -> String {
        v.join(" ")
    }

    #[test]
    fn push_lines() {
        let o = PushOptions {
            remote: s("origin"),
            local: s("main"),
            remote_branch: s("main"),
            set_upstream: true,
            ..Default::default()
        };
        assert_eq!(
            line(&push(&o)[0]),
            "push --set-upstream --progress -- origin main:main"
        );
        let all = push(&PushOptions {
            remote: s("origin"),
            all_branches: true,
            tags: true,
            force: true,
            ..Default::default()
        });
        assert_eq!(all.len(), 2);
        assert_eq!(line(&all[0]), "push --all --force --progress -- origin");
        assert_eq!(line(&all[1]), "push --tags --force --progress -- origin");
        let del = push(&PushOptions {
            remote: s("origin"),
            remote_branch: s("old"),
            recurse_submodules: RecurseSubmodules::Check,
            ..Default::default()
        });
        assert_eq!(
            line(&del[0]),
            "push --recurse-submodules=check --progress -- origin :old"
        );
    }

    #[test]
    fn pull_fetch_lines() {
        let o = PullFetchOptions {
            remote: s("origin"),
            remote_branch: s("main"),
            ff_only: true,
            tags: TriState::Off,
            prune: TriState::On,
            verbose: true,
            ..Default::default()
        };
        assert_eq!(
            line(&pull(&o)),
            "pull --progress -v --no-rebase --no-tags --ff-only --prune -- origin main"
        );
        assert_eq!(
            line(&fetch(&o)),
            "fetch --progress -v --prune --no-tags -- origin main"
        );
        let all = PullFetchOptions {
            all_remotes: true,
            ..Default::default()
        };
        assert_eq!(line(&fetch(&all)), "fetch --all --progress");
    }

    #[test]
    fn clone_switch_branch_tag_lines() {
        let c = CloneOptions {
            url: s("https://x/y.git"),
            directory: s("/tmp/y"),
            recursive: true,
            depth: Some(1),
            ..Default::default()
        };
        assert_eq!(
            line(&clone(&c)),
            "clone --progress --recursive --depth 1 -v -- https://x/y.git /tmp/y"
        );
        let sw = SwitchOptions {
            version: s("remotes/origin/dev"),
            new_branch: Some(s("dev")),
            track: TriState::On,
            ..Default::default()
        };
        assert_eq!(
            line(&switch(&sw)),
            "checkout --track -b dev --end-of-options remotes/origin/dev --"
        );
        let plain = SwitchOptions {
            version: s("refs/heads/main"),
            force: true,
            ..Default::default()
        };
        assert_eq!(
            line(&switch(&plain)),
            "checkout -f --end-of-options main --"
        );
        assert_eq!(
            switch_default_branch_name("remotes/origin/dev", &[s("origin")]),
            (s("dev"), true)
        );
        assert_eq!(
            switch_default_branch_name("v1.0", &[s("origin")]),
            (s("Branch_v1.0"), false)
        );
        assert_eq!(
            line(&create_branch("b", "HEAD", true, TriState::Off)),
            "branch -f --no-track -- b HEAD"
        );
        assert_eq!(
            line(&create_tag("v1", "HEAD", false, false, Some("/tmp/m"))),
            "tag -F /tmp/m -- v1 HEAD"
        );
    }

    #[test]
    fn merge_reset_stash_lines() {
        let m = MergeOptions {
            version: s("feature"),
            no_ff: true,
            log: Some(20),
            strategy: Some(s("recursive")),
            strategy_option: Some(s("rename-threshold")),
            strategy_param: Some(s("50")),
            ..Default::default()
        };
        assert_eq!(
            line(&merge(&m)),
            "merge --no-ff --log=20 --strategy=recursive --strategy-option=rename-threshold=50 -- feature"
        );
        assert_eq!(
            line(&reset(ResetType::Hard, "HEAD~1")),
            "reset --hard --end-of-options HEAD~1 --"
        );
        assert_eq!(line(&reset(ResetType::Merge, "HEAD")), "reset --merge");
        assert_eq!(
            line(&stash_save("wip", StashUntracked::IncludeUntracked)),
            "stash push --include-untracked -m wip"
        );
        assert_eq!(line(&stash_apply("refs/stash{1}")), "stash apply stash@{1}");
        assert_eq!(line(&stash_apply("")), "stash apply");
        assert_eq!(line(&stash_drop("stash{0}")), "stash drop -- stash@{0}");
    }
}
