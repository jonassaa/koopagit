//! `/command:subadd` (`CSubmoduleAddDlg`) and `/command:requestpull`
//! (`CRequestPullDlg`, `CAppUtils::RequestPull`).

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use koopagit_core::extras;
use koopagit_core::launcher::Invocation;
use koopagit_core::settings::keys;
use slint::ComponentHandle;

use crate::progress::ProgressJob;
use crate::remote_util::{clone_url_from_clipboard, merged, strings, wd_key};
use crate::{Context, DialogResult, RequestPullDialog, SubmoduleAddDialog, msgbox, spawn_launcher};

/// The OK button's decision, read after the dialog closed.
type Decision<T> = Rc<RefCell<Option<T>>>;

const REPO_HISTORY: (&str, &str) = (r"History\SubModuleRepoURLS", "url");
const PATH_HISTORY: (&str, &str) = (r"History\SubModulePath", "url");

/// The folder the command was started on, relative to the work tree
/// (`cmdLinePath.GetDirectory()`); a selected file means its folder.
fn start_folder(ctx: &Context) -> String {
    let Some(p) = ctx.paths.first() else {
        return String::new();
    };
    let dir = if p.is_dir() {
        p.clone()
    } else {
        p.parent().map(Path::to_owned).unwrap_or_default()
    };
    ctx.repo.relative_path(&dir).unwrap_or_default()
}

pub fn submodule_add(ctx: &mut Context) -> DialogResult<i32> {
    let dlg = SubmoduleAddDialog::new()?;
    let workdir = ctx.repo.workdir().to_owned();
    let base = start_folder(ctx);
    dlg.set_dialog_title(crate::window_title(&workdir.join(&base), "Submodule Add").into());
    dlg.set_project(workdir.to_string_lossy().as_ref().into());
    let repos = ctx.settings.history(REPO_HISTORY.0, REPO_HISTORY.1);
    let paths = ctx.settings.history(PATH_HISTORY.0, PATH_HISTORY.1);
    dlg.set_repository_history(strings(&repos));
    dlg.set_path_history(strings(&paths));
    dlg.set_path(base.clone().into());
    // SubmoduleAddDlg.cpp:108: a link on the clipboard, else the history.
    // Like SetWindowText/SetCurSel this does not update the path.
    if let Some(url) = crate::clipboard::text()
        .and_then(|t| clone_url_from_clipboard(&t))
        .or_else(|| repos.first().cloned())
    {
        dlg.set_repository(url.into());
    }

    {
        let weak = dlg.as_weak();
        let base = base.clone();
        dlg.on_repository_edited(move || {
            if let Some(d) = weak.upgrade() {
                let url = d.get_repository().trim().to_owned();
                d.set_path(extras::submodule_path_for_url(&base, &url).into());
            }
        });
    }
    {
        let weak = dlg.as_weak();
        let start = workdir.parent().map(Path::to_owned);
        dlg.on_browse_repository(move || {
            if let Some(d) = weak.upgrade()
                && let Some(p) = crate::pick::folder("Repository", start.as_deref())
            {
                d.set_repository(p.to_string_lossy().as_ref().into());
                d.invoke_repository_edited();
            }
        });
    }
    {
        let weak = dlg.as_weak();
        let workdir = workdir.clone();
        dlg.on_browse_path(move || {
            if let Some(d) = weak.upgrade()
                && let Some(p) = crate::pick::folder("Path", Some(&workdir))
            {
                let rel = p
                    .strip_prefix(&workdir)
                    .map(|r| r.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_else(|_| p.to_string_lossy().into_owned());
                d.set_path(rel.into());
            }
        });
    }
    let decided: Decision<(String, String, Option<String>, bool)> = Rc::default();
    {
        let weak = dlg.as_weak();
        let decided = decided.clone();
        let workdir = workdir.clone();
        dlg.on_ok(move || {
            let Some(d) = weak.upgrade() else { return };
            let required = "This field is required and must not be empty.";
            let branch = d.get_branch().trim().to_owned();
            if d.get_use_branch() && branch.is_empty() {
                d.set_notice_text(required.into());
                return;
            }
            let mut path = d.get_path().trim().to_owned();
            let repo = d.get_repository().trim().to_owned();
            if path.is_empty() || repo.is_empty() {
                d.set_notice_text(required.into());
                return;
            }
            // An absolute path inside the work tree becomes relative.
            if let Ok(rel) = Path::new(&path).strip_prefix(&workdir) {
                path = rel.to_string_lossy().replace('\\', "/");
            }
            *decided.borrow_mut() = Some((
                repo,
                path,
                d.get_use_branch().then_some(branch),
                d.get_force(),
            ));
            let _ = d.hide();
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
    crate::window::prepare(&dlg);
    dlg.run()?;

    let Some((repo, path, branch, force)) = decided.borrow_mut().take() else {
        return Ok(1);
    };
    let max = ctx.settings.dword(keys::MAX_HISTORY_ITEMS, 25) as usize;
    ctx.settings
        .push_history(REPO_HISTORY.0, REPO_HISTORY.1, &repo, max);
    ctx.settings
        .push_history(PATH_HISTORY.0, PATH_HISTORY.1, &path, max);
    let _ = ctx.settings.save();
    let ok = crate::progress::run(ProgressJob {
        title: ctx.window_title("Submodule Add"),
        git: ctx.repo.git(),
        commands: vec![extras::submodule_add_args(
            &repo,
            &path,
            branch.as_deref(),
            force,
        )],
        post_actions: Vec::new(),
        close_on_end: ctx.invocation.close_on_end().unwrap_or_default(),
    })?;
    Ok(if ok { 0 } else { 1 })
}

// ---------------------------------------------------------------- Request pull

const SEND_MAIL_KEY: &str = r"TortoiseProc\RequestPull\SendMail";

pub fn request_pull(ctx: &mut Context) -> DialogResult<i32> {
    let repo = &ctx.repo;
    let dlg = RequestPullDialog::new()?;
    dlg.set_dialog_title(ctx.window_title("Request pull").into());
    let base = wd_key(r"History\RequestPull", repo);
    let (start_key, url_key, end_key) = (
        format!(r"{base}\startrevision"),
        format!(r"{base}\repositoryurl"),
        format!(r"{base}\endrevision"),
    );
    let mut refs = repo.local_branches().unwrap_or_default();
    refs.extend(
        repo.remote_branches()
            .unwrap_or_default()
            .into_iter()
            .map(|b| format!("remotes/{b}")),
    );
    refs.extend(repo.tags().unwrap_or_default());
    dlg.set_start_items(strings(&refs));
    dlg.set_start(ctx.settings.string(&start_key, "").into());
    let urls = merged(
        ctx.settings.history(r"History\RequestPull", "url"),
        repo.remotes()
            .into_iter()
            .filter_map(|r| repo.config_get(&format!("remote.{r}.url"))),
    );
    dlg.set_url_history(strings(&urls));
    let url = ctx
        .invocation
        .get("url")
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| ctx.settings.string(&url_key, ""));
    dlg.set_url(url.into());
    let end = ctx
        .invocation
        .get("endrev")
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| ctx.settings.string(&end_key, "HEAD"));
    dlg.set_end_rev(end.into());
    dlg.set_send_mail(ctx.settings.bool(SEND_MAIL_KEY, false));

    let decided: Decision<(String, String, String)> = Rc::default();
    {
        let weak = dlg.as_weak();
        let decided = decided.clone();
        dlg.on_ok(move || {
            let Some(d) = weak.upgrade() else { return };
            let end = d.get_end_rev().trim().to_owned();
            if end.is_empty() {
                d.set_notice_text("Branch/Tag name must not be empty or is invalid.".into());
                return;
            }
            *decided.borrow_mut() = Some((
                d.get_start().trim().to_owned(),
                d.get_url().trim().to_owned(),
                end,
            ));
            let _ = d.hide();
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
    crate::window::prepare(&dlg);
    dlg.run()?;

    let Some((start, url, end)) = decided.borrow_mut().take() else {
        return Ok(1);
    };
    let max = ctx.settings.dword(keys::MAX_HISTORY_ITEMS, 25) as usize;
    ctx.settings
        .push_history(r"History\RequestPull", "url", &url, max);
    ctx.settings.set(&start_key, &start);
    ctx.settings.set(&url_key, &url);
    ctx.settings.set(&end_key, &end);
    ctx.settings.set_bool(SEND_MAIL_KEY, dlg.get_send_mail());
    let _ = ctx.settings.save();

    let out = ctx
        .repo
        .git()
        .output(extras::request_pull_args(&start, &url, &end))?;
    if !out.success() {
        msgbox::error(
            "KoopaGit",
            &format!("Failed to create pull-request.\n{}", out.stderr),
        );
        return Ok(1);
    }
    let dir = std::env::temp_dir().join(format!("koopagit-pr-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let file = dir.join("pullrequest.txt");
    std::fs::write(&file, &out.stdout)?;
    if dlg.get_send_mail() {
        spawn_launcher(&Invocation::new("sendmail").with_paths([file]));
        return Ok(0);
    }
    // AppUtils.cpp:3011.
    crate::editor::launch(&file)?;
    Ok(0)
}
