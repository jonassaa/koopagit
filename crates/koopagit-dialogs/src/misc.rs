//! Commands that are a single git operation in a progress dialog:
//! submodules, bisect, LFS locks, `cat` and `help`.

use crate::progress::{PostAction, ProgressJob};
use crate::{Context, DialogResult, msgbox};

fn s(v: &str) -> String {
    v.to_owned()
}

fn run_job(
    ctx: &Context,
    title: &str,
    commands: Vec<Vec<String>>,
    post: Vec<PostAction>,
) -> DialogResult<i32> {
    let ok = crate::progress::run(ProgressJob {
        title: ctx.window_title(title),
        git: ctx.repo.git(),
        commands,
        post_actions: post,
        close_on_end: ctx.invocation.close_on_end().unwrap_or_else(|| {
            koopagit_core::launcher::CloseOnEnd::from_value(i64::from(
                ctx.settings
                    .dword(koopagit_core::settings::keys::AUTO_CLOSE_GIT_PROGRESS, 0),
            ))
        }),
    })?;
    Ok(if ok { 0 } else { 1 })
}

/// Paths of the selection relative to the repo root, for `-- <paths>`.
fn pathspec(ctx: &Context) -> Vec<String> {
    let rel = ctx.rel_paths();
    if rel.is_empty() {
        vec![]
    } else {
        std::iter::once(s("--")).chain(rel).collect()
    }
}

/// `/command:subupdate` (Submodule Update): `git submodule update --init --recursive`.
pub fn subupdate(ctx: &mut Context) -> DialogResult<i32> {
    let mut cmd = vec![s("submodule"), s("update"), s("--init"), s("--recursive")];
    cmd.extend(pathspec(ctx));
    run_job(ctx, "Submodule Update", vec![cmd], vec![])
}

/// `/command:subsync` (Submodule Sync): `git submodule sync --recursive`.
pub fn subsync(ctx: &mut Context) -> DialogResult<i32> {
    let mut cmd = vec![s("submodule"), s("sync"), s("--recursive")];
    cmd.extend(pathspec(ctx));
    run_job(ctx, "Submodule Sync", vec![cmd], vec![])
}

/// `/command:bisect /start|/good|/bad|/skip|/reset`.
pub fn bisect(ctx: &mut Context) -> DialogResult<i32> {
    let inv = &ctx.invocation;
    let log = ctx.launch("log");
    if inv.has("start") {
        let Some(good) =
            crate::fileops::input("Bisect start", "Good revision (known to work):", "", |v| {
                v.trim().is_empty().then(|| s("Enter a revision"))
            })?
        else {
            return Ok(1);
        };
        let bad = crate::fileops::input("Bisect start", "Bad revision:", "HEAD", |v| {
            v.trim().is_empty().then(|| s("Enter a revision"))
        })?
        .unwrap_or_else(|| s("HEAD"));
        let cmd = vec![s("bisect"), s("start"), s("--")];
        let mark_bad = vec![s("bisect"), s("bad"), bad.trim().to_owned()];
        let mark_good = vec![s("bisect"), s("good"), good.trim().to_owned()];
        return run_job(
            ctx,
            "Bisect start",
            vec![cmd, mark_bad, mark_good],
            vec![PostAction::on_success("Show log", log)],
        );
    }
    let verb = ["good", "bad", "skip", "reset"]
        .into_iter()
        .find(|v| inv.has(v));
    let Some(verb) = verb else {
        msgbox::error("KoopaGit", "Missing /start, /good, /bad, /skip or /reset.");
        return Ok(1);
    };
    let mut cmd = vec![s("bisect"), s(verb)];
    if let Some(rev) = inv.get("ref").filter(|r| !r.is_empty()) {
        cmd.push(rev.to_owned());
    }
    let post = if verb == "reset" {
        vec![]
    } else {
        vec![PostAction::on_success("Show log", log)]
    };
    run_job(ctx, &format!("Bisect {verb}"), vec![cmd], post)
}

/// `/command:lfslock` / `/command:lfsunlock`.
pub fn lfs(ctx: &mut Context, lock: bool) -> DialogResult<i32> {
    let commands = ctx
        .rel_paths()
        .into_iter()
        .map(|p| vec![s("lfs"), s(if lock { "lock" } else { "unlock" }), p])
        .collect();
    run_job(
        ctx,
        if lock { "LFS Lock" } else { "LFS Unlock" },
        commands,
        vec![],
    )
}

/// `/command:lfslocks`: `git lfs locks`.
pub fn lfs_locks(ctx: &mut Context) -> DialogResult<i32> {
    run_job(ctx, "LFS Locks", vec![vec![s("lfs"), s("locks")]], vec![])
}

/// `/command:cat /path:<file> /savepath:<out> [/revision:<rev>]`: writes a file
/// version to disk without UI.
pub fn cat(ctx: &mut Context) -> DialogResult<i32> {
    let inv = &ctx.invocation;
    let (Some(rel), Some(out)) = (ctx.rel_paths().into_iter().next(), inv.get("savepath")) else {
        msgbox::error("KoopaGit", "/command:cat needs /path and /savepath.");
        return Ok(1);
    };
    let rev = inv
        .get("revision")
        .filter(|r| !r.is_empty())
        .unwrap_or("HEAD");
    let output = ctx
        .repo
        .git()
        .command(["show", &format!("{rev}:{rel}")])
        .output()?;
    if !output.status.success() {
        msgbox::error("KoopaGit", &String::from_utf8_lossy(&output.stderr));
        return Ok(1);
    }
    std::fs::write(out, output.stdout)?;
    Ok(0)
}

/// `/command:help` / `/command:rtfm`: opens the TortoiseGit manual (the
/// behaviour KoopaGit clones) in the default browser.
pub fn help() -> DialogResult<i32> {
    let url = "https://tortoisegit.org/docs/tortoisegit/";
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    if std::process::Command::new(opener).arg(url).spawn().is_err() {
        msgbox::info("KoopaGit", url);
    }
    Ok(0)
}
