//! git-svn commands: `svndcommit`, `svnrebase`, `svnfetch`, `svnignore`
//! (`CAppUtils::SVNDCommit`, `SVNRebaseCommand`, `SVNFetchCommand`,
//! `SVNIgnoreCommand`).

use std::cell::Cell;
use std::rc::Rc;

use koopagit_core::misccmds;
use slint::ComponentHandle;

use crate::progress::PostAction;
use crate::refs::run_job;
use crate::{Context, DialogResult, SvnDcommitDialog, SvnIgnoreDialog, msgbox};

const NOT_CLEAN: &str = "The current working tree is not clean.\nDo you want to stash the changes?";
const STASH_POP: &str = "Do you want to stash pop now?";

/// Offers to stash a dirty working tree; `None` = abort, `Some(stashed)`.
fn stash_if_dirty(ctx: &Context, with_path: bool) -> Option<bool> {
    if ctx.repo.is_clean() {
        return Some(false);
    }
    let text = if with_path {
        format!("{}\r\n{NOT_CLEAN}", ctx.repo.workdir().display())
    } else {
        NOT_CLEAN.to_owned()
    };
    if msgbox::ask("KoopaGit", &text, &["Stash", "Abort"]) != Some(0) {
        return None;
    }
    match ctx.repo.git().run(["stash"]) {
        Ok(_) => Some(true),
        Err(e) => {
            msgbox::error("KoopaGit", &e.to_string());
            None
        }
    }
}

fn ask_stash_pop(ctx: &Context) {
    if msgbox::ask("KoopaGit", STASH_POP, &["Yes", "No"]) == Some(0) {
        if let Err(e) = ctx.repo.git().run(["stash", "pop"]) {
            msgbox::error("KoopaGit", &e.to_string());
        }
    }
}

/// `/command:svndcommit`.
pub fn dcommit(ctx: &mut Context) -> DialogResult<i32> {
    let git = ctx.repo.git();
    let configured = git
        .run(["config", "--get", "svn.rmdir"])
        .map(|v| v.trim().to_owned())
        .unwrap_or_default();
    let rmdir = if configured.is_empty() {
        let dlg = SvnDcommitDialog::new()?;
        let accepted = Rc::new(Cell::new(false));
        {
            let weak = dlg.as_weak();
            let accepted = accepted.clone();
            dlg.on_ok(move || {
                accepted.set(true);
                if let Some(d) = weak.upgrade() {
                    let _ = d.hide();
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
        crate::snapshot::arm(&dlg);
        dlg.run()?;
        if !accepted.get() {
            return Ok(1);
        }
        let rmdir = dlg.get_rmdir();
        if dlg.get_remember() {
            let value = if rmdir { "true" } else { "false" };
            if git.run(["config", "svn.rmdir", value]).is_err() {
                msgbox::error(
                    "KoopaGit",
                    &format!("Saving config failed (key: \"svn.rmdir\", value: \"{value}\")."),
                );
            }
        }
        rmdir
    } else {
        configured == "true"
    };
    let Some(stashed) = stash_if_dirty(ctx, false) else {
        return Ok(1);
    };
    let ok = run_job(
        ctx,
        "SVN DCommit",
        vec![misccmds::svn_dcommit(rmdir)],
        vec![],
    )?;
    if ok && stashed {
        ask_stash_pop(ctx);
    }
    Ok(if ok { 0 } else { 1 })
}

/// `/command:svnrebase`.
pub fn rebase(ctx: &mut Context) -> DialogResult<i32> {
    let Some(stashed) = stash_if_dirty(ctx, true) else {
        return Ok(1);
    };
    let Some(upstream) = ctx.repo.svn_upstream() else {
        msgbox::error(
            "KoopaGit",
            "Could not get \"svn-remote.svn.fetch\" config value.",
        );
        return Ok(1);
    };
    let fetch = vec!["svn".to_owned(), "fetch".to_owned()];
    let mut job_ctx_ok = run_job(ctx, "SVN Fetch", vec![fetch], vec![])?;
    if !job_ctx_ok {
        return Ok(1);
    }
    let workdir = ctx.repo.workdir().display().to_string();
    let new_hash = ctx.repo.rev_parse(&upstream).unwrap_or_default();
    let head = ctx.repo.rev_parse("HEAD").unwrap_or_default();
    if new_hash == head {
        msgbox::info("KoopaGit", &format!("{workdir}\r\nEverything updated."));
    } else if ctx.repo.is_ancestor("HEAD", &upstream) {
        job_ctx_ok = run_job(
            ctx,
            "Fast Forward",
            vec![koopagit_core::commands::reset(
                koopagit_core::commands::ResetType::Hard,
                &upstream,
            )],
            vec![],
        )?;
        if job_ctx_ok {
            msgbox::info("KoopaGit", &format!("{workdir}\r\nFast Forward"));
        }
    } else {
        // Need a rebase: the Rebase dialog, waited for so the stash question
        // comes afterwards like upstream.
        let exe = std::env::current_exe()?;
        let inv = ctx.launch("rebase").with("upstream", upstream);
        let status = std::process::Command::new(exe)
            .args(inv.to_args())
            .status()?;
        job_ctx_ok = status.success();
    }
    if stashed {
        ask_stash_pop(ctx);
    }
    Ok(if job_ctx_ok { 0 } else { 1 })
}

/// `/command:svnfetch`.
pub fn fetch(ctx: &mut Context) -> DialogResult<i32> {
    let Some(upstream) = ctx.repo.svn_upstream() else {
        msgbox::error("KoopaGit", "Found no SVN remote.");
        return Ok(1);
    };
    let Ok(old) = ctx.repo.rev_parse(&upstream) else {
        msgbox::error("KoopaGit", "Could not get upstream hash.");
        return Ok(1);
    };
    // "Pulled Diff" / "Pulled Log" need the new hash, which is only known after
    // the fetch; they compare against the upstream ref name instead.
    let post = vec![
        PostAction::on_success(
            "Pulled Diff",
            ctx.launch("showcompare")
                .with("revision1", old.clone())
                .with("revision2", upstream.clone()),
        ),
        PostAction::on_success(
            "Pulled Log",
            ctx.launch("log")
                .with("range", format!("{old}..{upstream}")),
        ),
    ];
    let ok = run_job(
        ctx,
        "SVN Fetch",
        vec![vec!["svn".to_owned(), "fetch".to_owned()]],
        post,
    )?;
    Ok(if ok { 0 } else { 1 })
}

/// `/command:svnignore`.
pub fn ignore(ctx: &mut Context) -> DialogResult<i32> {
    let dlg = SvnIgnoreDialog::new()?;
    let accepted = Rc::new(Cell::new(false));
    {
        let weak = dlg.as_weak();
        let accepted = accepted.clone();
        dlg.on_ok(move || {
            accepted.set(true);
            if let Some(d) = weak.upgrade() {
                let _ = d.hide();
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
    crate::snapshot::arm(&dlg);
    dlg.run()?;
    if !accepted.get() {
        return Ok(1);
    }
    if dlg.get_ignore_type() == 0 {
        // `git svn show-ignore > .git/info/exclude`
        let exclude = ctx.repo.git_dir().join("info").join("exclude");
        match ctx.repo.git().run(["svn", "show-ignore"]) {
            Ok(out) => {
                if let Some(dir) = exclude.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                std::fs::write(&exclude, out)?;
                msgbox::info(
                    "Import SVN Ignore",
                    &format!("git svn show-ignore > {}\n\nSuccess", exclude.display()),
                );
                Ok(0)
            }
            Err(e) => {
                msgbox::error("Import SVN Ignore", &e.to_string());
                Ok(1)
            }
        }
    } else {
        let ok = run_job(
            ctx,
            "Import SVN Ignore",
            vec![vec!["svn".to_owned(), "create-ignore".to_owned()]],
            vec![],
        )?;
        Ok(if ok { 0 } else { 1 })
    }
}
