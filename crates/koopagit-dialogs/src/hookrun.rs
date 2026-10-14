//! Running the client-side hook scripts configured on the Hook Scripts
//! settings page (`CHooks::StartCommit`, `PreCommit`, `PostCommit`,
//! `PrePush`, `PostPush`, `PreRebase`).

use std::path::Path;

use koopagit_core::hooks::{self, HookArgs, HookType};
use koopagit_core::settings::Settings;

use crate::msgbox;

fn find(kind: HookType, workdir: &Path) -> Option<hooks::Hook> {
    let all = hooks::parse(&Settings::load_default().string(hooks::SETTINGS_KEY, ""));
    hooks::find(&all, kind, workdir).cloned()
}

fn report(kind: HookType, code: i32, error: &str) -> String {
    format!(
        "The hook script returned an error:\n{} (exit code {code})\n\n{}",
        kind.label(),
        error.trim()
    )
}

/// start-commit: may rewrite the initial message. Errors are shown.
pub fn start_commit(workdir: &Path, paths: &[String], message: String) -> String {
    let Some(h) = find(HookType::StartCommit, workdir) else {
        return message;
    };
    match hooks::run(
        &h,
        workdir,
        HookArgs::Commit {
            paths,
            message: &message,
        },
    ) {
        Ok(r) if r.exit_code == 0 => r.message.unwrap_or(message),
        Ok(r) => {
            msgbox::error("KoopaGit", &report(h.kind, r.exit_code, &r.error));
            message
        }
        Err(e) => {
            msgbox::error("KoopaGit", &e.to_string());
            message
        }
    }
}

/// pre-commit: `Err` aborts the commit; `Ok(Some(msg))` is the rewritten message.
pub fn pre_commit(
    workdir: &Path,
    paths: &[String],
    message: &str,
) -> Result<Option<String>, String> {
    let Some(h) = find(HookType::PreCommit, workdir) else {
        return Ok(None);
    };
    match hooks::run(&h, workdir, HookArgs::Commit { paths, message }) {
        Ok(r) if r.exit_code == 0 => Ok(r.message),
        Ok(r) => Err(report(h.kind, r.exit_code, &r.error)),
        Err(e) => Err(e.to_string()),
    }
}

/// post-commit; failures are reported.
pub fn post_commit(workdir: &Path, amend: bool) {
    let Some(h) = find(HookType::PostCommit, workdir) else {
        return;
    };
    match hooks::run(&h, workdir, HookArgs::PostCommit { amend }) {
        Ok(r) if r.exit_code != 0 => {
            msgbox::error("KoopaGit", &report(h.kind, r.exit_code, &r.error))
        }
        Err(e) => msgbox::error("KoopaGit", &e.to_string()),
        _ => {}
    }
}

/// pre-push: `Err` = the hook failed; upstream lets the user decide to push anyway.
pub fn pre_push(workdir: &Path) -> Result<(), String> {
    let Some(h) = find(HookType::PrePush, workdir) else {
        return Ok(());
    };
    match hooks::run(&h, workdir, HookArgs::Push { error: "" }) {
        Ok(r) if r.exit_code == 0 => Ok(()),
        Ok(r) => Err(report(h.kind, r.exit_code, &r.error)),
        Err(e) => Err(e.to_string()),
    }
}

/// Runs pre-push and asks whether to proceed when it fails (AppUtils.cpp:2751:
/// IDS_HOOKFAILED_TASK2..4, "Abort" is the default). Returns `false` to abort.
pub fn pre_push_or_ask(workdir: &Path) -> bool {
    match pre_push(workdir) {
        Ok(()) => true,
        Err(e) => {
            msgbox::ask(
                "KoopaGit",
                &format!("Hook script failed\n{e}"),
                &["Abort", "Proceed anyway"],
            ) == Some(1)
        }
    }
}

/// pre-rebase (`CRebaseDlg::CheckRebaseCondition`, RebaseDlg.cpp:978): `Err`
/// = the hook failed; the caller asks whether to proceed anyway.
pub fn pre_rebase(workdir: &Path, upstream: &str, branch: &str) -> Result<(), String> {
    let Some(h) = find(HookType::PreRebase, workdir) else {
        return Ok(());
    };
    match hooks::run(&h, workdir, HookArgs::Rebase { upstream, branch }) {
        Ok(r) if r.exit_code == 0 => Ok(()),
        Ok(r) => Err(report(h.kind, r.exit_code, &r.error)),
        Err(e) => Err(e.to_string()),
    }
}

/// post-push; failures are reported.
pub fn post_push(workdir: &Path) {
    let Some(h) = find(HookType::PostPush, workdir) else {
        return;
    };
    match hooks::run(&h, workdir, HookArgs::Push { error: "" }) {
        Ok(r) if r.exit_code != 0 => {
            msgbox::error("KoopaGit", &report(h.kind, r.exit_code, &r.error))
        }
        Err(e) => msgbox::error("KoopaGit", &e.to_string()),
        _ => {}
    }
}
