//! Helpers shared by the Push, Pull/Fetch, Sync and Clone dialogs.

use koopagit_core::Repo;
use koopagit_core::settings::{Settings, workdir_key};
use slint::{ModelRc, SharedString, VecModel};

/// A Slint string model.
pub fn strings<S: AsRef<str>>(items: &[S]) -> ModelRc<SharedString> {
    let v: Vec<SharedString> = items.iter().map(|s| s.as_ref().into()).collect();
    ModelRc::new(VecModel::from(v))
}

/// `<base>\<workdir key>` for per-repository settings.
pub fn wd_key(base: &str, repo: &Repo) -> String {
    format!(r"{base}\{}", workdir_key(repo.workdir()))
}

/// Remote-tracking branches of `remote` without the `<remote>/` prefix.
pub fn remote_branches_of(repo: &Repo, remote: &str) -> Vec<String> {
    let prefix = format!("{remote}/");
    repo.remote_branches()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|b| b.strip_prefix(&prefix).map(str::to_owned))
        .collect()
}

/// History items first, then `extra` entries not already present.
pub fn merged(history: Vec<String>, extra: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut v = history;
    for e in extra {
        if !e.is_empty() && !v.contains(&e) {
            v.push(e);
        }
    }
    v
}

/// Whether `name` is a valid branch name (`git check-ref-format --branch`).
pub fn valid_branch_name(repo: &Repo, name: &str) -> bool {
    !name.trim().is_empty()
        && repo
            .git()
            .output(["check-ref-format", "--branch", name])
            .is_ok_and(|o| o.success())
}

/// The " Default: X" label next to the Tags/Prune tri-states.
pub fn tags_default_label(repo: &Repo, remote: &str) -> String {
    let v = repo.config_get(&format!("remote.{remote}.tagopt"));
    let d = match v.as_deref() {
        Some("--no-tags") => "None",
        Some("--tags") => "All",
        _ => "Reachable",
    };
    format!("Default: {d}")
}

pub fn prune_default_label(repo: &Repo, remote: &str) -> String {
    let v = repo
        .config_get(&format!("remote.{remote}.prune"))
        .or_else(|| repo.config_get("fetch.prune"))
        .unwrap_or_else(|| "false".to_owned());
    format!("Default: {v}")
}

/// Tri-state UI value (0 off, 1 on, 2 indeterminate) to the core type.
pub fn tri(v: i32) -> koopagit_core::commands::TriState {
    use koopagit_core::commands::TriState;
    match v {
        0 => TriState::Off,
        1 => TriState::On,
        _ => TriState::Default,
    }
}

pub fn save(settings: &Settings) {
    if let Err(e) = settings.save() {
        eprintln!("koopagit: could not save settings: {e}");
    }
}
