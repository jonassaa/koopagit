//! `/command:clone [/path:<dir>] [/url:<url>] [/exactpath]` — `CCloneDlg` and
//! `CloneCommand::Execute`. Runs without a repository.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use koopagit_core::commands::{self, CloneOptions};
use koopagit_core::git_cli::Git;
use koopagit_core::launcher::{CloseOnEnd, Invocation};
use koopagit_core::remote::{clone_module_name, clone_target_dir};
use koopagit_core::settings::Settings;
use slint::ComponentHandle;

use crate::progress::{PostAction, ProgressJob};
use crate::remote_util::{clone_url_from_clipboard, save, strings};
use crate::{CloneDialog, DialogResult};

/// Where the clone process runs: the nearest existing ancestor of the target.
fn existing_ancestor(dir: &Path) -> PathBuf {
    let mut p = dir.to_owned();
    while !p.is_dir() {
        match p.parent() {
            Some(parent) => p = parent.to_owned(),
            None => return std::env::temp_dir(),
        }
    }
    p
}

/// `\\server\share` URLs are given to git with forward slashes.
fn normalize_url(url: &str) -> String {
    if url.starts_with(r"\\") {
        url.replace('\\', "/")
    } else {
        url.to_owned()
    }
}

fn default_dir(inv: &Invocation, settings: &Settings, cwd: &Path) -> PathBuf {
    if let Some(p) = inv.paths().ok().and_then(|p| p.into_iter().next()) {
        return p;
    }
    // The launcher passes the CWD unless started as a URL handler.
    if !inv.has("hasurlhandler") {
        return cwd.to_owned();
    }
    let last = settings.string(r"TortoiseProc\CloneDir", "");
    if let Some(parent) = Path::new(&last).parent().filter(|_| !last.is_empty()) {
        return parent.to_owned();
    }
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|h| PathBuf::from(h).join("Documents"))
        .unwrap_or_else(|| cwd.to_owned())
}

pub fn run(cwd: &Path, inv: &Invocation, settings: &Settings) -> DialogResult<i32> {
    let dlg = CloneDialog::new()?;
    let history = settings.history(r"History\repoURLS", "url");
    dlg.set_url_items(strings(&history));
    // CloneDlg.cpp:163: /url, else a link on the clipboard, else the history.
    let url = inv
        .get("url")
        .map(str::to_owned)
        .or_else(|| crate::clipboard::text().and_then(|t| clone_url_from_clipboard(&t)))
        .or_else(|| history.first().cloned())
        .unwrap_or_default();
    let exact = inv.has("exactpath");
    let base = default_dir(inv, settings, cwd);
    let module = Rc::new(RefCell::new(String::new()));
    let dir = if exact || url.is_empty() {
        base.to_string_lossy().into_owned()
    } else {
        *module.borrow_mut() = clone_module_name(&url);
        clone_target_dir(&base.to_string_lossy(), "", &url)
    };
    dlg.set_url(url.into());
    dlg.set_directory(dir.into());
    dlg.set_recursive(settings.bool(r"TortoiseProc\CloneRecursive", false));
    dlg.set_origin("origin".into());

    if !exact {
        let weak = dlg.as_weak();
        let module = module.clone();
        dlg.on_url_changed(move |u| {
            let Some(d) = weak.upgrade() else { return };
            let previous = module.borrow().clone();
            let next = clone_target_dir(d.get_directory().as_str(), &previous, u.as_str());
            *module.borrow_mut() = clone_module_name(u.as_str());
            d.set_directory(next.into());
        });
    }
    {
        let weak = dlg.as_weak();
        dlg.on_browse_directory(move || {
            let Some(d) = weak.upgrade() else { return };
            let current = PathBuf::from(d.get_directory().as_str());
            let start = current
                .ancestors()
                .find(|p| p.is_dir())
                .map(std::path::Path::to_owned);
            if let Some(dir) = crate::pick::folder("Browse", start.as_deref()) {
                d.set_directory(dir.to_string_lossy().into_owned().into());
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
    let chosen: Rc<RefCell<Option<CloneOptions>>> = Rc::new(RefCell::new(None));
    {
        let weak = dlg.as_weak();
        let chosen = chosen.clone();
        dlg.on_ok(move || {
            let Some(d) = weak.upgrade() else { return };
            match validate(&d) {
                Ok(o) => {
                    *chosen.borrow_mut() = Some(o);
                    let _ = d.hide();
                }
                Err(msg) => {
                    d.set_notice_text(msg.into());
                    d.set_notice_error(true);
                }
            }
        });
    }

    crate::window::prepare(&dlg);
    dlg.run()?;
    let Some(o) = chosen.borrow_mut().take() else {
        return Ok(1);
    };

    let mut settings = settings.clone();
    settings.push_history(r"History\repoURLS", "url", &o.url, 25);
    settings.set(r"TortoiseProc\CloneDir", &o.directory);
    settings.set_bool(r"TortoiseProc\CloneRecursive", o.recursive);
    save(&settings);

    let target = PathBuf::from(&o.directory);
    let target = if target.is_absolute() {
        target
    } else {
        cwd.join(target)
    };
    let o = CloneOptions {
        directory: target.to_string_lossy().into_owned(),
        ..o
    };
    let retry = Invocation::new("clone")
        .with("path", o.directory.clone())
        .with("url", o.url.clone())
        .with_switch("exactpath");
    let job = ProgressJob {
        title: crate::window_title(&target, "Clone"),
        git: Git::new(existing_ancestor(&target)),
        commands: vec![commands::clone(&o)],
        post_actions: vec![
            PostAction::on_failure("Retry", retry),
            PostAction::on_success(
                "Show log",
                Invocation::new("log").with_paths([target.clone()]),
            ),
        ],
        close_on_end: CloseOnEnd::Manual,
    };
    Ok(if crate::progress::run(job)? { 0 } else { 1 })
}

fn validate(d: &CloneDialog) -> Result<CloneOptions, &'static str> {
    let url = normalize_url(d.get_url().trim());
    let directory = d.get_directory().trim().to_owned();
    if url.is_empty() || directory.is_empty() {
        return Err("URL and directory must not be empty.");
    }
    let branch = d.get_use_branch().then(|| d.get_branch().trim().to_owned());
    if let Some(b) = &branch {
        let valid = !b.is_empty()
            && Git::new(std::env::temp_dir())
                .output(["check-ref-format", "--branch", b])
                .is_ok_and(|o| o.success());
        if !valid {
            return Err("Branch/Tag name is invalid.");
        }
    }
    let bare = d.get_bare();
    let origin = (d.get_use_origin() && !bare).then(|| d.get_origin().trim().to_owned());
    if origin.as_deref() == Some("") {
        return Err("Remote name must not be empty.");
    }
    let depth = if d.get_use_depth() {
        Some(
            d.get_depth()
                .trim()
                .parse::<u32>()
                .map_err(|_| "Depth must be a number.")?,
        )
    } else {
        None
    };
    Ok(CloneOptions {
        url,
        directory,
        recursive: d.get_recursive() && !bare,
        bare,
        no_checkout: d.get_no_checkout() && !bare,
        branch,
        origin,
        depth,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert_eq!(normalize_url(r"\\srv\share\r"), "//srv/share/r");
        assert_eq!(normalize_url("https://x"), "https://x");
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(existing_ancestor(&dir.path().join("a/b")), dir.path());
    }
}
