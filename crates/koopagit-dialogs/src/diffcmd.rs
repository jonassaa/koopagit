//! `/command:diff`, `prevdiff`, `showcompare`, `conflicteditor`, `blame`
//! (`DiffCommand`, `PrevDiffCommand`, `ShowCompareCommand`,
//! `CAppUtils::ConflictEdit`, `BlameCommand`) and the `koopagit-merge`
//! (TortoiseGitMerge) entry point.

use std::path::{Path, PathBuf};

use koopagit_core::Repo;
use koopagit_core::launcher::Invocation;
use koopagit_core::settings::Settings;
use koopagit_core::tools::{self, Tool};

use crate::viewers::{self, ConflictArgs};
use crate::{Context, DialogResult, msgbox, spawn_launcher};

/// Path of a sibling executable (`koopagit-merge`) next to the running one.
pub fn sibling_exe(name: &str) -> PathBuf {
    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join(&file)))
        .filter(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from(file))
}

fn spawn(args: &[String]) {
    let Some((exe, rest)) = args.split_first() else {
        return;
    };
    if let Err(e) = std::process::Command::new(exe).args(rest).spawn() {
        msgbox::error(
            "KoopaGit",
            &format!("Could not start the diff/merge program:\n{exe}\n\n{e}"),
        );
    }
}

/// `StartExtDiff`: the configured external tool, or the built-in viewer.
pub fn start_diff(
    settings: &Settings,
    base: &Path,
    mine: &Path,
    base_name: &str,
    mine_name: &str,
    alternative: bool,
    line: Option<i64>,
) {
    match tools::pick_diff_tool(settings, base, mine, alternative) {
        Tool::Internal => {
            let mut args = vec![
                sibling_exe("koopagit-merge").to_string_lossy().into_owned(),
                format!("/base:{}", base.display()),
                format!("/mine:{}", mine.display()),
                format!("/basename:{base_name}"),
                format!("/minename:{mine_name}"),
            ];
            if let Some(l) = line {
                args.push(format!("/line:{l}"));
            }
            spawn(&args);
        }
        Tool::External(cmd) => {
            let (b, m) = (base.to_string_lossy(), mine.to_string_lossy());
            spawn(&tools::expand_command(
                &cmd,
                &[
                    ("base", &b),
                    ("mine", &m),
                    ("bname", base_name),
                    ("yname", mine_name),
                    ("bpath", &b),
                    ("ypath", &m),
                ],
                &["base", "mine"],
            ));
        }
    }
}

/// `<path>: <shorthash>` title of a revision.
pub(crate) fn rev_title(repo: &Repo, rel: &str, rev: &str) -> String {
    format!(
        "{rel}: {}",
        repo.short_id(rev).unwrap_or_else(|_| rev.to_owned())
    )
}

pub(crate) fn wt_title(rel: &str) -> String {
    format!("{rel}: Working Tree")
}

/// `git diff` text for the unified viewer.
pub(crate) fn unified_text(repo: &Repo, revs: &[&str], rel: Option<&str>) -> DialogResult<String> {
    let mut args = vec!["diff".to_owned(), "--no-color".to_owned()];
    args.extend(revs.iter().map(|r| (*r).to_owned()));
    args.push("--".to_owned());
    if let Some(r) = rel.filter(|r| !r.is_empty()) {
        args.push(r.to_owned());
    }
    Ok(repo.git().run(args)?)
}

pub(crate) fn is_zero_rev(r: &str) -> bool {
    !r.is_empty() && r.bytes().all(|b| b == b'0')
}

/// `/command:diff`. `ctx` is `None` outside a repository (two plain files).
pub fn diff(ctx: Option<&Context>, inv: &Invocation) -> DialogResult<i32> {
    let alternative = inv.has("alternative");
    let unified = inv.has("unified");
    let line = inv.get_long("line");
    let paths = inv.paths()?;
    let cwd = std::env::current_dir().unwrap_or_default();
    let abs = |p: &Path| {
        if p.is_absolute() {
            p.to_owned()
        } else {
            cwd.join(p)
        }
    };
    let Some(path) = paths.first().map(|p| abs(p)) else {
        msgbox::error("KoopaGit", "No path given to diff.");
        return Ok(1);
    };
    let start = inv
        .get("startrev")
        .filter(|r| !r.is_empty() && !is_zero_rev(r));
    let end = inv
        .get("endrev")
        .filter(|r| !r.is_empty() && !is_zero_rev(r));
    let settings = ctx
        .map(|c| c.settings.clone())
        .unwrap_or_else(Settings::load_default);

    if let Some(path2) = inv.get("path2").map(|p| abs(Path::new(p))) {
        // Two repository paths at two revisions, else two files on disk.
        if let (Some(ctx), Some(s), Some(e)) = (ctx, start, end) {
            if let (Some(r1), Some(r2)) = (
                ctx.repo.relative_path(&path),
                ctx.repo.relative_path(&path2),
            ) {
                let left = ctx.repo.extract_blob(s, &r1)?;
                let right = ctx.repo.extract_blob(e, &r2)?;
                start_diff(
                    &settings,
                    &left,
                    &right,
                    &rev_title(&ctx.repo, &r1, s),
                    &rev_title(&ctx.repo, &r2, e),
                    alternative,
                    line,
                );
                return Ok(0);
            }
        }
        start_diff(
            &settings,
            &path,
            &path2,
            &path.to_string_lossy(),
            &path2.to_string_lossy(),
            alternative,
            line,
        );
        return Ok(0);
    }

    let Some(ctx) = ctx else {
        msgbox::error("KoopaGit", "No working directory found.");
        return Ok(1);
    };
    if path.is_dir() && !inv.has("submodule") {
        spawn_launcher(&Invocation::new("repostatus").with_paths([path]));
        return Ok(0);
    }
    let repo = &ctx.repo;
    let rel = repo.relative_path(&path).unwrap_or_default();
    if let (Some(s), Some(e)) = (start, end) {
        if unified {
            let text = unified_text(repo, &[s, e], Some(&rel))?;
            viewers::run_udiff(&format!("{rel}: {s}..{e}"), &text)?;
        } else {
            let left = repo.extract_blob(s, &rel)?;
            let right = repo.extract_blob(e, &rel)?;
            start_diff(
                &settings,
                &left,
                &right,
                &rev_title(repo, &rel, s),
                &rev_title(repo, &rel, e),
                alternative,
                line,
            );
        }
        return Ok(0);
    }
    // Only a start revision: that revision against the working tree file
    // (file list "Compare with working tree", amend "Compare with base").
    if let (Some(s), None) = (start, end) {
        if unified {
            let text = unified_text(repo, &[s], Some(&rel))?;
            viewers::run_udiff(&wt_title(&rel), &text)?;
        } else {
            let left = repo.extract_blob(s, &rel)?;
            start_diff(
                &settings,
                &left,
                &path,
                &rev_title(repo, &rel, s),
                &wt_title(&rel),
                alternative,
                line,
            );
        }
        return Ok(0);
    }
    if unified {
        let text = unified_text(repo, &["HEAD"], Some(&rel))?;
        viewers::run_udiff(&wt_title(&rel), &text)?;
        return Ok(0);
    }
    diff_with_head(ctx, &path, &rel, alternative, line)?;
    Ok(0)
}

/// Working tree file against HEAD; a new file is compared with its rename
/// source if git detects one, else with an empty file.
fn diff_with_head(
    ctx: &Context,
    path: &Path,
    rel: &str,
    alternative: bool,
    line: Option<i64>,
) -> DialogResult {
    let repo = &ctx.repo;
    if !path.exists() {
        msgbox::error(
            "KoopaGit",
            &format!("The file \"{rel}\" does not exist in your working tree."),
        );
        return Ok(());
    }
    let has_head = repo.head_id()?.is_some();
    let in_head = has_head
        && repo
            .git()
            .output(["cat-file", "-e", &format!("HEAD:{rel}")])
            .is_ok_and(|o| o.success());
    let base_rel = if in_head || !has_head {
        rel.to_owned()
    } else {
        repo.rename_source(rel).unwrap_or_else(|| rel.to_owned())
    };
    let left = if has_head {
        repo.extract_blob("HEAD", &base_rel)?
    } else {
        let empty = tools::temp_dir().join("empty");
        std::fs::write(&empty, b"")?;
        empty
    };
    let left_title = if has_head {
        rev_title(repo, &base_rel, "HEAD")
    } else {
        format!("{rel}: (empty)")
    };
    start_diff(
        &ctx.settings,
        &left,
        path,
        &left_title,
        &wt_title(rel),
        alternative,
        line,
    );
    Ok(())
}

/// `/command:prevdiff`: working tree against the previous revision of the file.
pub fn prevdiff(ctx: &Context, inv: &Invocation) -> DialogResult<i32> {
    let Some(path) = ctx.paths.first().cloned() else {
        return Ok(1);
    };
    let repo = &ctx.repo;
    let rel = repo.relative_path(&path).unwrap_or_default();
    if path.is_dir() {
        // PrevDiffCommand: a folder shows HEAD~1 against the working tree in
        // the Changed Files dialog.
        return crate::filediff::from_context(ctx, "HEAD~1", "");
    }
    let revs = repo.last_two_revisions(&rel)?;
    let Some(prev) = revs.get(1) else {
        msgbox::error("KoopaGit", "No previous revision found.");
        return Ok(1);
    };
    if inv.has("unified") {
        let text = unified_text(repo, &[prev], Some(&rel))?;
        viewers::run_udiff(&wt_title(&rel), &text)?;
        return Ok(0);
    }
    let left = repo.extract_blob(prev, &rel)?;
    start_diff(
        &ctx.settings,
        &left,
        &path,
        &rev_title(repo, &rel, prev),
        &wt_title(&rel),
        inv.has("alternative"),
        None,
    );
    Ok(0)
}

/// `/command:showcompare /revision1:<a> /revision2:<b> [/unified]`.
pub fn showcompare(ctx: &Context, inv: &Invocation) -> DialogResult<i32> {
    let repo = &ctx.repo;
    let rev1 = inv.get("revision1").unwrap_or("HEAD");
    let rev2 = inv.get("revision2").unwrap_or_default();
    let path = ctx.paths.first().cloned();
    let rel = path
        .as_ref()
        .and_then(|p| repo.relative_path(p))
        .unwrap_or_default();
    // An all-zero revision means the working tree.
    let wt = rev2.is_empty() || is_zero_rev(rev2);
    let is_file = path.as_ref().is_some_and(|p| p.is_file()) && !rel.is_empty();
    if !inv.has("unified") && is_file {
        let left = repo.extract_blob(rev1, &rel)?;
        let (right, right_title) = if wt {
            (repo.workdir().join(&rel), wt_title(&rel))
        } else {
            (repo.extract_blob(rev2, &rel)?, rev_title(repo, &rel, rev2))
        };
        start_diff(
            &ctx.settings,
            &left,
            &right,
            &rev_title(repo, &rel, rev1),
            &right_title,
            inv.has("alternative"),
            None,
        );
        return Ok(0);
    }
    if !inv.has("unified") {
        // A folder or the repository root opens the Changed Files dialog.
        return crate::filediff::from_context(ctx, rev1, if wt { "" } else { rev2 });
    }
    let revs: Vec<&str> = if wt { vec![rev1] } else { vec![rev1, rev2] };
    let text = unified_text(repo, &revs, Some(&rel))?;
    let title = if wt {
        format!("{rev1}..Working Tree")
    } else {
        format!("{rev1}..{rev2}")
    };
    viewers::run_udiff(&title, &text)?;
    Ok(0)
}

/// `/command:conflicteditor`: extract BASE/LOCAL/REMOTE stages and open the
/// merge tool (external, or koopagit-merge's conflict editor).
pub fn conflicteditor(ctx: &Context, inv: &Invocation) -> DialogResult<i32> {
    let Some(path) = ctx.paths.first().cloned() else {
        return Ok(1);
    };
    let repo = &ctx.repo;
    let rel = repo.relative_path(&path).unwrap_or_default();
    let unmerged = repo
        .git()
        .run(["ls-files", "-u", "-z", "--", &rel])
        .unwrap_or_default();
    if unmerged.is_empty() {
        msgbox::info(
            "KoopaGit",
            &format!("\"{rel}\" is not in a conflicted state."),
        );
        return Ok(1);
    }
    let base = repo.extract_stage(1, &rel, "BASE")?;
    let mine = repo.extract_stage(2, &rel, "LOCAL")?;
    let theirs = repo.extract_stage(3, &rel, "REMOTE")?;
    let (bname, yname, tname) = ("Base", "Mine", "Theirs");
    let mname = format!("Merged: {rel}");
    match tools::pick_merge_tool(&ctx.settings, &path, inv.has("alternative")) {
        Tool::Internal => spawn(&[
            sibling_exe("koopagit-merge").to_string_lossy().into_owned(),
            format!("/base:{}", base.display()),
            format!("/theirs:{}", theirs.display()),
            format!("/mine:{}", mine.display()),
            format!("/merged:{}", path.display()),
            format!("/basename:{bname}"),
            format!("/theirsname:{tname}"),
            format!("/minename:{yname}"),
            format!("/mergedname:{mname}"),
            "/saverequired".to_owned(),
            "/deletebasetheirsmineonclose".to_owned(),
        ]),
        Tool::External(cmd) => {
            let s = |p: &Path| p.to_string_lossy().into_owned();
            spawn(&tools::expand_command(
                &cmd,
                &[
                    ("base", &s(&base)),
                    ("theirs", &s(&theirs)),
                    ("mine", &s(&mine)),
                    ("merged", &s(&path)),
                    ("bname", bname),
                    ("tname", tname),
                    ("yname", yname),
                    ("mname", &mname),
                    ("wtroot", &s(repo.workdir())),
                ],
                &["base", "theirs", "mine", "merged"],
            ));
        }
    }
    Ok(0)
}

/// `/command:blame /path:<file> [/line:N] [/endrev:<rev>]`.
pub fn blame(ctx: &Context, inv: &Invocation) -> DialogResult<i32> {
    let Some(path) = ctx.paths.first().cloned() else {
        return Ok(1);
    };
    let rel = ctx.repo.relative_path(&path).unwrap_or_default();
    if rel.is_empty() || path.is_dir() {
        msgbox::error("KoopaGit", "Blame needs a single file.");
        return Ok(1);
    }
    let rev = inv
        .get("endrev")
        .filter(|r| !r.is_empty() && !is_zero_rev(r));
    let line = inv.get_long("line").and_then(|l| usize::try_from(l).ok());
    viewers::run_blame(&ctx.repo, &rel, rev, line)?;
    Ok(0)
}

/// Entry point of the `koopagit-merge` binary (TortoiseGitMerge command line):
/// `/base: /mine: [/theirs: /merged:] /basename: /minename: /theirsname:
/// /mergedname: /line: /diff:<patch>`.
pub fn merge_app(inv: &Invocation) -> DialogResult<i32> {
    let p = |k: &str| inv.get(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let name = |k: &str, file: &Option<PathBuf>| {
        inv.get(k)
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
            .or_else(|| file.as_ref().map(|f| f.display().to_string()))
            .unwrap_or_default()
    };
    if let Some(patch) = p("diff").or_else(|| p("patchfile")) {
        let text = std::fs::read_to_string(&patch)?;
        viewers::run_udiff(&patch.display().to_string(), &text)?;
        return Ok(0);
    }
    let (base, mine, theirs, merged) = (p("base"), p("mine"), p("theirs"), p("merged"));
    if let Some(merged) = merged {
        let cleanup = if inv.has("deletebasetheirsmineonclose") {
            [&base, &mine, &theirs]
                .into_iter()
                .flatten()
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        return viewers::run_conflict(ConflictArgs {
            theirs_title: name("theirsname", &theirs),
            mine_title: name("minename", &mine),
            merged_title: name("mergedname", &Some(merged.clone())),
            merged,
            cleanup,
        });
    }
    match (base, mine) {
        (Some(b), Some(m)) => {
            let line = inv.get_long("line").and_then(|l| usize::try_from(l).ok());
            viewers::run_diff(
                &b,
                &m,
                &name("basename", &Some(b.clone())),
                &name("minename", &Some(m.clone())),
                line,
            )?;
            Ok(0)
        }
        _ => {
            msgbox::info(
                "KoopaGitMerge",
                "Usage: koopagit-merge /base:<file> /mine:<file> [/theirs:<file> /merged:<file>]\n       koopagit-merge /diff:<patchfile>",
            );
            Ok(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_revisions_mean_working_tree() {
        assert!(is_zero_rev(&"0".repeat(40)));
        assert!(!is_zero_rev("HEAD"));
        assert!(!is_zero_rev(""));
    }
}
