//! `/command:export` (`CExportDlg`) and `/command:formatpatch` (`CFormatPatchDlg`).

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use koopagit_core::extras::{self, PatchRange};
use koopagit_core::git_cli::Git;
use koopagit_core::launcher::{CloseOnEnd, Invocation};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::progress::ProgressJob;
use crate::refs::Versions;
use crate::remote_util::{strings, wd_key};
use crate::{Context, DialogResult, ExportDialog, FormatPatchDialog, spawn_launcher};

/// The OK button's decision, read after the dialog closed.
type Decision<T> = Rc<RefCell<Option<T>>>;

fn close_on_end(ctx: &Context) -> CloseOnEnd {
    ctx.invocation.close_on_end().unwrap_or_else(|| {
        CloseOnEnd::from_value(i64::from(
            ctx.settings
                .dword(koopagit_core::settings::keys::AUTO_CLOSE_GIT_PROGRESS, 0),
        ))
    })
}

fn head_label(ctx: &Context) -> String {
    let branch = ctx
        .repo
        .current_branch()
        .ok()
        .flatten()
        .unwrap_or_else(|| "(no branch)".to_owned());
    format!("HEAD ({branch})")
}

// ---------------------------------------------------------------- Export

/// What the OK button decided.
struct ExportJob {
    file: String,
    version: String,
    cwd: PathBuf,
}

pub fn export(ctx: &mut Context) -> DialogResult<i32> {
    let versions = Versions::load(ctx, false);
    let dlg = ExportDialog::new()?;
    dlg.set_dialog_title(ctx.window_title("Export").into());
    dlg.set_head_label(head_label(ctx).into());
    dlg.set_branches(strings(&versions.branches));
    dlg.set_tags(strings(&versions.tags));
    match ctx
        .invocation
        .get("rev")
        .filter(|r| !r.is_empty() && *r != "HEAD")
    {
        None => dlg.set_mode(0),
        Some(rev) => {
            let (mode, b, t) = versions.locate(rev);
            dlg.set_mode(mode);
            dlg.set_branch_index(b);
            dlg.set_tag_index(t);
            if mode == 3 {
                dlg.set_commit_text(rev.into());
            }
        }
    }
    // The selected folder; the repository root forces "Whole Project".
    let org = ctx
        .paths
        .first()
        .cloned()
        .unwrap_or_else(|| ctx.repo.workdir().to_owned());
    let org = org.canonicalize().unwrap_or(org);
    let at_root = org == ctx.repo.workdir();
    dlg.set_whole_project(at_root);
    dlg.set_whole_project_enabled(!at_root);

    let job: Rc<RefCell<Option<ExportJob>>> = Rc::default();
    let confirmed_overwrite = Rc::new(Cell::new(false));
    let versions = Rc::new(versions);

    {
        let weak = dlg.as_weak();
        let versions = versions.clone();
        let start = ctx.repo.workdir().parent().map(|p| p.to_owned());
        dlg.on_browse(move || {
            let Some(d) = weak.upgrade() else { return };
            let name = versions.selected(
                d.get_mode(),
                d.get_branch_index(),
                d.get_tag_index(),
                &d.get_commit_text(),
            );
            let proposal = format!("{}.zip", name.replace(['/', '\\'], "_"));
            if let Some(p) = crate::pick::save_file("Export", start.as_deref(), &proposal) {
                d.set_file(p.to_string_lossy().as_ref().into());
            }
        });
    }
    {
        let weak = dlg.as_weak();
        let job = job.clone();
        let versions = versions.clone();
        let confirmed = confirmed_overwrite.clone();
        let workdir = ctx.repo.workdir().to_owned();
        let org = org.clone();
        dlg.on_ok(move || {
            let Some(d) = weak.upgrade() else { return };
            let version = versions.selected(
                d.get_mode(),
                d.get_branch_index(),
                d.get_tag_index(),
                &d.get_commit_text(),
            );
            if version.is_empty() {
                notice(&d, "Invalid revision number!", &[], true);
                return;
            }
            let raw = d.get_file().to_string();
            if raw.trim().is_empty() {
                notice(
                    &d,
                    "You must select a filename for the zip-file!",
                    &[],
                    true,
                );
                return;
            }
            let file = extras::export_file_name(raw.trim());
            d.set_file(file.clone().into());
            let path = PathBuf::from(&file);
            if path.is_dir() {
                notice(
                    &d,
                    "You selected a folder.\nExports are only possible to a (zip) file.",
                    &[],
                    true,
                );
                return;
            }
            if path.exists() && !confirmed.get() {
                notice(
                    &d,
                    &format!("The file\n{file}\nalready exists! Do you want to overwrite it?"),
                    &["Yes", "No"],
                    false,
                );
                return;
            }
            let cwd = if !d.get_whole_project() && org.is_dir() {
                org.clone()
            } else {
                workdir.clone()
            };
            *job.borrow_mut() = Some(ExportJob { file, version, cwd });
            let _ = d.hide();
        });
    }
    {
        let weak = dlg.as_weak();
        let confirmed = confirmed_overwrite.clone();
        dlg.on_notice_clicked(move |i| {
            let Some(d) = weak.upgrade() else { return };
            notice(&d, "", &[], false);
            if i == 0 {
                confirmed.set(true);
                d.invoke_ok();
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

    let Some(job) = job.borrow_mut().take() else {
        return Ok(1);
    };
    let ok = crate::progress::run(ProgressJob {
        title: ctx.window_title("Export"),
        git: Git::new(&job.cwd),
        commands: vec![extras::export_args(&job.file, &job.version)],
        post_actions: Vec::new(),
        close_on_end: close_on_end(ctx),
    })?;
    Ok(if ok { 0 } else { 1 })
}

fn notice(d: &ExportDialog, text: &str, buttons: &[&str], error: bool) {
    d.set_notice_text(text.into());
    let labels: Vec<SharedString> = buttons.iter().map(|b| (*b).into()).collect();
    d.set_notice_buttons(ModelRc::new(VecModel::from(labels)));
    d.set_notice_error(error);
}

// ---------------------------------------------------------------- Format Patch

const SEND_MAIL_KEY: &str = r"TortoiseProc\FormatPatch\SendMail";
const NO_PREFIX_KEY: &str = r"TortoiseProc\FormatPatch\NoPrefix";

/// Reads the dialog's Version group.
fn range_of(d: &FormatPatchDialog) -> Result<PatchRange, &'static str> {
    match d.get_mode() {
        0 => {
            let since = d.get_since().trim().to_owned();
            if since.is_empty() {
                Err("Invalid revision number!")
            } else {
                Ok(PatchRange::Since(since))
            }
        }
        1 => Ok(PatchRange::Number(d.get_number().max(1) as u32)),
        _ => {
            let (f, t) = (d.get_from().trim().to_owned(), d.get_to().trim().to_owned());
            if f.is_empty() || t.is_empty() {
                Err("Invalid revision number!")
            } else {
                Ok(PatchRange::Range(f, t))
            }
        }
    }
}

pub fn format_patch(ctx: &mut Context) -> DialogResult<i32> {
    let repo = &ctx.repo;
    let dlg = FormatPatchDialog::new()?;
    dlg.set_dialog_title(crate::window_title(repo.workdir(), "Format Patch").into());
    let workdir = repo.workdir().to_string_lossy().into_owned();
    let mut dirs = ctx.settings.history(r"History\FormatPatchURLS", "path");
    if !dirs.contains(&workdir) {
        dirs.push(workdir.clone());
    }
    dlg.set_dir(dirs.first().cloned().unwrap_or_default().into());
    dlg.set_dir_history(strings(&dirs));
    let mut branches = repo.local_branches().unwrap_or_default();
    branches.extend(
        repo.remote_branches()
            .unwrap_or_default()
            .into_iter()
            .map(|b| format!("remotes/{b}")),
    );
    dlg.set_branches(strings(&branches));
    let since_key = format!(r"{}\Since", wd_key(r"History\FormatPatch", repo));
    let from_hist = ctx.settings.history(r"History\FormatPatchFromURLS", "ver");
    let to_hist = ctx.settings.history(r"History\FormatPatchToURLS", "ver");
    dlg.set_from_history(strings(&from_hist));
    dlg.set_to_history(strings(&to_hist));
    dlg.set_from(from_hist.first().cloned().unwrap_or_default().into());
    dlg.set_to(to_hist.first().cloned().unwrap_or_default().into());
    dlg.set_since(ctx.settings.string(&since_key, "").into());

    // /startrev alone: Since; /startrev + /endrev: Range (FormatPatchCommand.cpp).
    let start = ctx.invocation.get("startrev").filter(|v| !v.is_empty());
    let end = ctx.invocation.get("endrev").filter(|v| !v.is_empty());
    match (start, end) {
        (Some(s), None) => {
            dlg.set_since(s.into());
            dlg.set_from(format!("{s}~1").into());
            dlg.set_to(s.into());
            dlg.set_mode(0);
        }
        (Some(s), Some(e)) => {
            dlg.set_from(s.into());
            dlg.set_to(e.into());
            dlg.set_mode(2);
        }
        _ => {
            if let Some((f, t)) = ctx.invocation.get("range").and_then(|r| r.split_once("..")) {
                dlg.set_from(f.into());
                dlg.set_to(t.into());
                dlg.set_mode(2);
            }
        }
    }
    dlg.set_send_mail(ctx.settings.bool(SEND_MAIL_KEY, false));
    dlg.set_no_prefix(ctx.settings.bool(NO_PREFIX_KEY, false));
    dlg.set_ok_enabled(repo.head_id()?.is_some());

    let decided: Decision<(Vec<String>, String)> = Rc::default();
    {
        let weak = dlg.as_weak();
        dlg.on_browse_dir(move || {
            let Some(d) = weak.upgrade() else { return };
            let current = PathBuf::from(d.get_dir().as_str());
            if let Some(p) = crate::pick::folder("Output Directory", Some(&current)) {
                d.set_dir(p.to_string_lossy().as_ref().into());
            }
        });
    }
    {
        let workdir = repo.workdir().to_owned();
        dlg.on_unified_diff(move || {
            spawn_launcher(
                &Invocation::new("showcompare")
                    .with_paths([workdir.clone()])
                    .with_switch("unified")
                    .with("revision1", "HEAD"),
            );
        });
    }
    {
        let weak = dlg.as_weak();
        let decided = decided.clone();
        dlg.on_ok(move || {
            let Some(d) = weak.upgrade() else { return };
            let dir = d.get_dir().trim().to_owned();
            if dir.is_empty() {
                d.set_notice_text("This field is required and must not be empty.".into());
                d.set_notice_error(true);
                return;
            }
            match range_of(&d) {
                Ok(range) => {
                    let args = extras::format_patch_args(d.get_no_prefix(), &dir, &range);
                    *decided.borrow_mut() = Some((args, dir));
                    let _ = d.hide();
                }
                Err(msg) => {
                    d.set_notice_text(msg.into());
                    d.set_notice_error(true);
                }
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

    let Some((args, dir)) = decided.borrow_mut().take() else {
        return Ok(1);
    };
    // OnBnClickedOk: save histories and options.
    let max = ctx
        .settings
        .dword(koopagit_core::settings::keys::MAX_HISTORY_ITEMS, 25) as usize;
    ctx.settings
        .push_history(r"History\FormatPatchURLS", "path", &dir, max);
    if dlg.get_mode() == 2 {
        ctx.settings.push_history(
            r"History\FormatPatchFromURLS",
            "ver",
            dlg.get_from().trim(),
            max,
        );
        ctx.settings.push_history(
            r"History\FormatPatchToURLS",
            "ver",
            dlg.get_to().trim(),
            max,
        );
    }
    if dlg.get_mode() == 0 {
        ctx.settings.set(&since_key, dlg.get_since().trim());
    }
    ctx.settings.set_bool(SEND_MAIL_KEY, dlg.get_send_mail());
    ctx.settings.set_bool(NO_PREFIX_KEY, dlg.get_no_prefix());
    let _ = ctx.settings.save();

    let ok = crate::progress::run(ProgressJob {
        title: ctx.window_title("Format Patch"),
        git: ctx.repo.git(),
        commands: vec![args],
        post_actions: Vec::new(),
        close_on_end: close_on_end(ctx),
    })?;
    if ok && dlg.get_send_mail() {
        spawn_launcher(&Invocation::new("sendmail").with_paths([PathBuf::from(&dir)]));
    }
    Ok(if ok { 0 } else { 1 })
}
