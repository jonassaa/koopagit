//! `CResetDlg` / `CAppUtils::GitReset`: "Reset <branch> to this...".
//!
//! There is no `/command:` for reset upstream; it opens from the Log, RefLog
//! and Browse References dialogs, so it is shown non-blocking inside their
//! event loop.

use std::path::PathBuf;

use koopagit_core::Repo;
use koopagit_core::commands::{self, ResetType};
use koopagit_core::git_cli::Git;
use koopagit_core::launcher::{CloseOnEnd, Invocation};
use slint::ComponentHandle;

use crate::progress::{PostAction, ProgressJob};
use crate::refs::{Versions, string_model};
use crate::{ResetDialog, keep_alive, spawn_launcher};

/// The Branch/Tag/Commit lists of the "choose version" block.
fn versions(repo: Option<&Repo>) -> Versions {
    let Some(repo) = repo else {
        return Versions::default();
    };
    let mut branches = repo.local_branches().unwrap_or_default();
    branches.extend(
        repo.remote_branches()
            .unwrap_or_default()
            .into_iter()
            .map(|b| format!("remotes/{b}")),
    );
    Versions {
        branches,
        tags: repo.tags().unwrap_or_default(),
    }
}

fn reset_type(i: i32) -> ResetType {
    match i {
        0 => ResetType::Soft,
        2 => ResetType::Hard,
        _ => ResetType::Mixed,
    }
}

/// Opens the Reset dialog for `rev`. `on_done(success)` runs after git finished.
pub fn show(
    workdir: PathBuf,
    rev: &str,
    on_done: impl Fn(bool) + 'static,
) -> Result<(), slint::PlatformError> {
    let dlg = ResetDialog::new()?;
    let repo = Repo::discover(&workdir).ok();
    let branch = repo
        .as_ref()
        .and_then(|r| r.current_branch().ok().flatten())
        .unwrap_or_else(|| "(no branch)".to_owned());
    dlg.set_dialog_title(crate::window_title(&workdir, "Reset").into());
    dlg.set_current_branch(branch.into());
    let versions = versions(repo.as_ref());
    dlg.set_branches(string_model(&versions.branches));
    dlg.set_tags(string_model(&versions.tags));
    let (mode, b, t) = versions.locate(rev);
    dlg.set_version_mode(mode);
    dlg.set_branch_index(b);
    dlg.set_tag_index(t);
    dlg.set_revision(if mode == 3 { rev } else { "" }.into());
    let on_done = std::rc::Rc::new(on_done);
    {
        let weak = dlg.as_weak();
        dlg.on_cancel(move || {
            if let Some(d) = weak.upgrade() {
                let _ = d.hide();
            }
        });
    }
    {
        let workdir = workdir.clone();
        dlg.on_show_modified(move || {
            spawn_launcher(&Invocation::new("repostatus").with_paths([workdir.clone()]));
        });
    }
    {
        let weak = dlg.as_weak();
        dlg.on_ok(move || {
            let Some(d) = weak.upgrade() else { return };
            let rev = versions.selected(
                d.get_version_mode(),
                d.get_branch_index(),
                d.get_tag_index(),
                d.get_revision().as_str(),
            );
            let git = Git::new(&workdir);
            let valid = git
                .output([
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("{rev}^{{commit}}"),
                ])
                .is_ok_and(|o| o.success());
            if rev.trim().is_empty() || !valid {
                d.set_error_text(format!("\"{rev}\" is not a valid revision.").into());
                return;
            }
            let kind = reset_type(d.get_reset_type());
            let mut post = Vec::new();
            if kind == ResetType::Hard {
                post.push(PostAction::on_success(
                    "Clean up",
                    Invocation::new("cleanup").with_paths([workdir.clone()]),
                ));
                if workdir.join(".gitmodules").exists() {
                    post.push(PostAction::on_success(
                        "Update submodules",
                        Invocation::new("subupdate")
                            .with_paths([workdir.clone()])
                            .with("bkpath", workdir.to_string_lossy()),
                    ));
                }
            }
            let job = ProgressJob {
                title: crate::window_title(&workdir, "Reset"),
                git,
                commands: vec![commands::reset(kind, &rev)],
                post_actions: post,
                close_on_end: CloseOnEnd::Manual,
            };
            let _ = d.hide();
            let on_done = on_done.clone();
            match crate::progress::start(job, move |ok| on_done(ok)) {
                Ok(p) => keep_alive(p),
                Err(e) => eprintln!("reset: {e}"),
            }
        });
    }
    crate::snapshot::arm(&dlg);
    dlg.show()?;
    keep_alive(dlg);
    Ok(())
}
