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

/// `CAppUtils::GetClipboardLink` (AppUtils.cpp:2018) on the clipboard text:
/// its first line if it looks like a URL or a drive path, or what follows
/// `skip_git_prefix` (e.g. `git clone `) up to `params_count + 1` words.
pub fn clipboard_link(text: &str, skip_git_prefix: &str, params_count: i32) -> Option<String> {
    let mut s = text;
    if s.starts_with('"') && s.ends_with('"') {
        s = s.get(1..s.len() - 1).unwrap_or("");
    }
    if let Some(nl) = s.find('\n') {
        s = s[..nl].trim_end();
    }
    if s.is_empty() {
        return None;
    }
    for prefix in ["http://", "https://", "git://", "ssh://", "git@"] {
        if s.starts_with(prefix) && s.len() != prefix.len() {
            return Some(s.to_owned());
        }
    }
    let b = s.as_bytes();
    if b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic() {
        return Some(s.to_owned());
    }
    if skip_git_prefix.is_empty() || !s.starts_with(skip_git_prefix) {
        return None;
    }
    let s = s[skip_git_prefix.len()..].trim();
    // Keep `params_count + 1` space-separated words.
    let mut count = params_count;
    let mut space: Option<usize> = None;
    while count >= 0 {
        count -= 1;
        let from = space.map_or(0, |p| p + 1);
        space = s[from..].find(' ').map(|p| p + from);
        if space.is_none() {
            break;
        }
    }
    let s = match space {
        Some(p) if p > 0 && count < 0 => &s[..p],
        _ => s,
    };
    (!s.is_empty()).then(|| s.to_owned())
}

/// Strips one pair of matching `"` or `'` quotes from a value longer than two
/// characters, as the dialogs do with clipboard links.
pub fn unquote(s: &str) -> &str {
    let quoted = s.len() > 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')));
    if quoted { &s[1..s.len() - 1] } else { s }
}

/// The URL Clone and Submodule Add prefill from the clipboard
/// (CloneDlg.cpp:165, SubmoduleAddDlg.cpp:108).
pub fn clone_url_from_clipboard(text: &str) -> Option<String> {
    let link = clipboard_link(text, "git clone ", 0)?;
    let url = unquote(link.trim());
    (!url.is_empty()).then(|| url.to_owned())
}

/// The URL and remote branch Pull/Fetch and Push prefill when "Arbitrary URL"
/// is chosen (PullFetchDlg.cpp:391, PushDlg.cpp:371): a `git pull`/`git fetch`
/// command line is preferred in the order given by `commands`.
pub fn pull_url_from_clipboard(
    text: &str,
    commands: [&str; 2],
) -> Option<(String, Option<String>)> {
    let path =
        clipboard_link(text, commands[0], 1).or_else(|| clipboard_link(text, commands[1], 1))?;
    match path.find(' ') {
        Some(sep) if sep > 1 && path.len() > sep + 2 => Some((
            unquote(&path[..sep]).to_owned(),
            Some(unquote(&path[sep + 1..]).to_owned()),
        )),
        _ => Some((path, None)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_links() {
        assert_eq!(
            clipboard_link("https://example.com/r.git\nmore", "", 0).as_deref(),
            Some("https://example.com/r.git")
        );
        assert_eq!(
            clipboard_link("\"git@host:r.git\"", "", 0).as_deref(),
            Some("git@host:r.git")
        );
        assert_eq!(clipboard_link("https://", "", 0), None);
        assert_eq!(
            clipboard_link("C:\\repos\\r", "", 0).as_deref(),
            Some("C:\\repos\\r")
        );
        assert_eq!(clipboard_link("hello world", "git clone ", 0), None);
        assert_eq!(clipboard_link("", "git clone ", 0), None);
    }

    #[test]
    fn clone_prefill() {
        assert_eq!(
            clone_url_from_clipboard("git clone https://x/r.git dir").as_deref(),
            Some("https://x/r.git")
        );
        assert_eq!(
            clone_url_from_clipboard("git clone 'file:///srv/r'").as_deref(),
            Some("file:///srv/r")
        );
        assert_eq!(
            clone_url_from_clipboard("https://x/r.git").as_deref(),
            Some("https://x/r.git")
        );
        assert_eq!(clone_url_from_clipboard("git clone "), None);
        assert_eq!(clone_url_from_clipboard("just text"), None);
    }

    #[test]
    fn pull_prefill() {
        let pull = ["git pull", "git fetch"];
        assert_eq!(
            pull_url_from_clipboard("git pull https://x/r.git main", pull),
            Some(("https://x/r.git".to_owned(), Some("main".to_owned())))
        );
        assert_eq!(
            pull_url_from_clipboard("git fetch \"/srv/r\" 'topic' extra", pull),
            Some(("/srv/r".to_owned(), Some("topic".to_owned())))
        );
        assert_eq!(
            pull_url_from_clipboard("git pull /srv/r", pull),
            Some(("/srv/r".to_owned(), None))
        );
        assert_eq!(
            pull_url_from_clipboard("https://x/r.git", pull),
            Some(("https://x/r.git".to_owned(), None))
        );
        assert_eq!(pull_url_from_clipboard("nothing", pull), None);
    }
}
