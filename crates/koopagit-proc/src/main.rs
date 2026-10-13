//! `koopagit-proc /command:<name> /path:<path> ...` — the TortoiseGitProc launcher.
//!
//! Also accepts `koopagit <command> [options] [path...]` (see [`cli`]).

mod cli;

use std::path::PathBuf;
use std::process::ExitCode;

use koopagit_core::Repo;
use koopagit_core::launcher::Invocation;
use koopagit_core::settings::Settings;
use koopagit_dialogs::{Context, DialogResult, msgbox};

/// Which repository a command needs (`Command::PathRequirement`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Needs {
    /// No repository (about, clone, settings, repocreate, ...).
    Nothing,
    /// A working tree must be found (`WorkingTreeRequired`, the default).
    WorkingTree,
    /// A working tree or a bare repository (`WorkingTreeOrBareRepoRequired`, log).
    WorkingTreeOrBare,
}

/// Every TortoiseGitProc command name, with whether KoopaGit implements it.
const COMMANDS: &[(&str, Needs)] = &[
    ("about", Needs::Nothing),
    ("add", Needs::WorkingTree),
    ("autotexttest", Needs::Nothing),
    ("blame", Needs::WorkingTree),
    ("branch", Needs::WorkingTree),
    ("cat", Needs::WorkingTree),
    ("cleanup", Needs::WorkingTree),
    ("clone", Needs::Nothing),
    ("commit", Needs::WorkingTree),
    ("conflicteditor", Needs::WorkingTree),
    ("diff", Needs::Nothing),
    ("dropcopyadd", Needs::WorkingTree),
    ("dropmove", Needs::WorkingTree),
    ("fetch", Needs::WorkingTree),
    ("formatpatch", Needs::WorkingTree),
    ("export", Needs::WorkingTree),
    ("help", Needs::Nothing),
    ("ignore", Needs::WorkingTree),
    ("importpatch", Needs::Nothing),
    ("log", Needs::WorkingTreeOrBare),
    ("merge", Needs::WorkingTree),
    ("prevdiff", Needs::WorkingTree),
    ("pull", Needs::WorkingTree),
    ("push", Needs::WorkingTree),
    ("rtfm", Needs::Nothing),
    ("remove", Needs::WorkingTree),
    ("rebase", Needs::WorkingTree),
    ("rename", Needs::WorkingTree),
    ("repocreate", Needs::Nothing),
    ("repostatus", Needs::WorkingTree),
    ("resolve", Needs::WorkingTree),
    ("revert", Needs::WorkingTree),
    ("sendmail", Needs::WorkingTree),
    ("settings", Needs::Nothing),
    ("showcompare", Needs::WorkingTree),
    ("switch", Needs::WorkingTree),
    ("tag", Needs::WorkingTree),
    ("updatecheck", Needs::Nothing),
    ("stashsave", Needs::WorkingTree),
    ("stashapply", Needs::WorkingTree),
    ("stashpop", Needs::WorkingTree),
    ("subadd", Needs::WorkingTree),
    ("subupdate", Needs::WorkingTree),
    ("subsync", Needs::WorkingTree),
    ("reflog", Needs::WorkingTree),
    ("refbrowse", Needs::WorkingTree),
    ("svndcommit", Needs::WorkingTree),
    ("svnrebase", Needs::WorkingTree),
    ("svnfetch", Needs::WorkingTree),
    ("svnignore", Needs::WorkingTree),
    ("sync", Needs::WorkingTree),
    ("requestpull", Needs::WorkingTree),
    ("bisect", Needs::WorkingTree),
    ("repobrowser", Needs::WorkingTree),
    ("revisiongraph", Needs::WorkingTree),
    ("daemon", Needs::WorkingTree),
    ("pgpfp", Needs::Nothing),
    ("commitisonrefs", Needs::WorkingTree),
    ("lfslocks", Needs::WorkingTree),
    ("lfslock", Needs::WorkingTree),
    ("lfsunlock", Needs::WorkingTree),
    ("newworktree", Needs::WorkingTree),
    ("worktreelist", Needs::WorkingTree),
    ("dropnewworktree", Needs::WorkingTree),
    ("inaccessible", Needs::Nothing),
    ("firststart", Needs::Nothing),
];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let invocation = if cli::is_upstream_syntax(&args) {
        match Invocation::parse(&args) {
            Ok(i) => i,
            Err(e) => {
                msgbox::error("KoopaGit", &e.to_string());
                return ExitCode::FAILURE;
            }
        }
    } else {
        // Help, version and usage errors go to the terminal, not a message box.
        cli::parse(&args).unwrap_or_else(|e| e.exit())
    };
    // Unknown commands fall back to About, like TortoiseGitProc.
    let (name, needs) = COMMANDS
        .iter()
        .find(|(n, _)| *n == invocation.command)
        .copied()
        .unwrap_or(("about", Needs::Nothing));

    let paths = match invocation.paths() {
        Ok(p) => p,
        Err(e) => {
            msgbox::error("KoopaGit", &e.to_string());
            return ExitCode::FAILURE;
        }
    };
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let paths: Vec<PathBuf> = paths
        .into_iter()
        .map(|p| if p.is_absolute() { p } else { cwd.join(p) })
        .collect();
    let probe = if paths.is_empty() {
        vec![cwd.clone()]
    } else {
        paths.clone()
    };
    let repo = if needs == Needs::WorkingTreeOrBare {
        probe
            .iter()
            .find_map(|p| Repo::discover_including_bare(p).ok())
    } else {
        probe.iter().find_map(|p| Repo::discover(p).ok())
    };

    let settings = Settings::load_default();
    if let Some(git) = Some(settings.string(koopagit_core::settings::keys::GIT_EXE_PATH, ""))
        .filter(|g| !g.is_empty())
    {
        let exe = PathBuf::from(&git).join(if cfg!(windows) { "git.exe" } else { "git" });
        // SAFETY: single-threaded at this point; no other thread reads the environment.
        unsafe { std::env::set_var(koopagit_core::git_cli::GIT_EXE_ENV, exe) };
    }

    let result = match (needs, repo) {
        (Needs::WorkingTree, None) => {
            msgbox::error("KoopaGit", "No working directory found.");
            return ExitCode::FAILURE;
        }
        (Needs::WorkingTreeOrBare, None) => {
            msgbox::error("KoopaGit", "No valid git repository found.");
            return ExitCode::FAILURE;
        }
        (_, Some(repo)) => {
            // Commands without a repository requirement (clone, settings, ...)
            // keep the caller's working directory.
            if needs != Needs::Nothing {
                let _ = std::env::set_current_dir(repo.workdir());
            }
            let mut ctx = Context {
                repo,
                settings,
                invocation: invocation.clone(),
                paths,
            };
            dispatch(name, Some(&mut ctx), &invocation)
        }
        (Needs::Nothing, None) => dispatch(name, None, &invocation),
    };
    match result {
        Ok(0) => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(e) => {
            msgbox::error("KoopaGit", &e.to_string());
            ExitCode::FAILURE
        }
    }
}

fn dispatch(name: &str, ctx: Option<&mut Context>, inv: &Invocation) -> DialogResult<i32> {
    use koopagit_dialogs as d;
    match (name, ctx) {
        ("commit", Some(ctx)) => d::commit::run(ctx),
        ("diff", ctx) => d::diffcmd::diff(ctx.as_deref(), inv),
        ("prevdiff", Some(ctx)) => d::diffcmd::prevdiff(ctx, inv),
        ("showcompare", Some(ctx)) => d::diffcmd::showcompare(ctx, inv),
        ("conflicteditor", Some(ctx)) => d::diffcmd::conflicteditor(ctx, inv),
        ("blame", Some(ctx)) => d::diffcmd::blame(ctx, inv),
        ("push", Some(ctx)) => d::push::run(ctx),
        ("pull", Some(ctx)) => d::pullfetch::run_pull(ctx),
        ("fetch", Some(ctx)) => d::pullfetch::run_fetch(ctx),
        ("sync", Some(ctx)) => d::sync::run(ctx),
        ("clone", ctx) => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            match ctx {
                Some(ctx) => d::clone::run(&cwd, inv, &ctx.settings),
                None => d::clone::run(&cwd, inv, &Settings::load_default()),
            }
        }
        ("settings", ctx) => match ctx {
            Some(ctx) => d::settings::run(Some(&ctx.repo), ctx.settings.clone(), inv),
            None => d::settings::run(None, Settings::load_default(), inv),
        },
        ("switch", Some(ctx)) => d::refs::switch(ctx),
        ("branch", Some(ctx)) => d::refs::branch_tag(ctx, false),
        ("tag", Some(ctx)) => d::refs::branch_tag(ctx, true),
        ("merge", Some(ctx)) => d::refs::merge(ctx),
        ("stashsave", Some(ctx)) => d::refs::stash_save(ctx),
        ("stashapply", Some(ctx)) => d::refs::stash_apply_pop(ctx, false),
        ("stashpop", Some(ctx)) => d::refs::stash_apply_pop(ctx, true),
        ("rebase", Some(ctx)) => d::rebase::run(ctx),
        ("add", Some(ctx)) => d::fileops::add(ctx),
        ("revert", Some(ctx)) => d::fileops::revert(ctx),
        ("remove", Some(ctx)) => d::fileops::remove(ctx),
        ("resolve", Some(ctx)) => d::fileops::resolve(ctx),
        ("rename", Some(ctx)) => d::fileops::rename(ctx),
        ("ignore", Some(ctx)) => d::fileops::ignore(ctx),
        ("cleanup", Some(ctx)) => d::fileops::cleanup(ctx),
        ("repostatus", Some(ctx)) => d::workingtree::run(ctx),
        ("repocreate", _) => d::fileops::repocreate(inv),
        ("inaccessible", _) => d::fileops::inaccessible(inv),
        ("log", Some(ctx)) => d::log::run(ctx),
        ("reflog", Some(ctx)) => d::reflog::run(ctx),
        ("refbrowse", Some(ctx)) => d::refbrowse::run(ctx),
        ("subupdate", Some(ctx)) => d::misc::subupdate(ctx),
        ("subsync", Some(ctx)) => d::misc::subsync(ctx),
        ("bisect", Some(ctx)) => d::misc::bisect(ctx),
        ("lfslock", Some(ctx)) => d::misc::lfs(ctx, true),
        ("lfsunlock", Some(ctx)) => d::misc::lfs(ctx, false),
        ("lfslocks", Some(ctx)) => d::misc::lfs_locks(ctx),
        ("cat", Some(ctx)) => d::misc::cat(ctx),
        ("help" | "rtfm", _) => d::misc::help(),
        ("export", Some(ctx)) => d::export::export(ctx),
        ("formatpatch", Some(ctx)) => d::export::format_patch(ctx),
        ("importpatch", Some(ctx)) => d::applypatch::run(ctx),
        ("importpatch", None) => d::applypatch::run_choosing_repository(inv),
        ("newworktree", Some(ctx)) => d::worktree::new_worktree(ctx, None),
        ("dropnewworktree", Some(ctx)) => d::worktree::drop_new_worktree(ctx),
        ("worktreelist", Some(ctx)) => d::worktree::list(ctx),
        ("subadd", Some(ctx)) => d::subadd::submodule_add(ctx),
        ("requestpull", Some(ctx)) => d::subadd::request_pull(ctx),
        ("repobrowser", Some(ctx)) => d::repobrowser::run(ctx),
        // Worker E: remaining commands.
        ("revisiongraph", Some(ctx)) => d::revgraph::run(ctx),
        ("sendmail", Some(ctx)) => d::sendmail::run(ctx),
        ("svndcommit", Some(ctx)) => d::svn::dcommit(ctx),
        ("svnrebase", Some(ctx)) => d::svn::rebase(ctx),
        ("svnfetch", Some(ctx)) => d::svn::fetch(ctx),
        ("svnignore", Some(ctx)) => d::svn::ignore(ctx),
        ("daemon", Some(ctx)) => d::extras::daemon(ctx),
        ("pgpfp", _) => d::extras::pgpfp(),
        ("updatecheck", _) => d::extras::updatecheck(),
        ("commitisonrefs", Some(ctx)) => d::extras::commit_is_on_refs(ctx),
        ("firststart", ctx) => match ctx {
            Some(ctx) => d::extras::firststart(Some(&ctx.repo), ctx.settings.clone()),
            None => d::extras::firststart(None, Settings::load_default()),
        },
        ("dropmove", Some(ctx)) => d::extras::dropmove(ctx),
        ("dropcopyadd", Some(ctx)) => d::extras::dropcopyadd(ctx),
        ("about", _) => {
            msgbox::info(
                "About KoopaGit",
                &format!(
                    "KoopaGit {}\n\nA cross-platform clone of TortoiseGit.",
                    env!("CARGO_PKG_VERSION")
                ),
            );
            Ok(0)
        }
        (other, _) => {
            msgbox::error(
                "KoopaGit",
                &format!(
                    "Command not implemented: /command:{other}\n\nArguments: {}",
                    inv.to_args().join(" ")
                ),
            );
            Ok(1)
        }
    }
}
