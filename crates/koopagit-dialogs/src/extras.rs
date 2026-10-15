//! Smaller commands: `daemon`, `pgpfp`, `updatecheck`, `commitisonrefs`,
//! `firststart`, `dropmove`, `dropcopyadd`.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use koopagit_core::Repo;
use koopagit_core::launcher::{CloseOnEnd, Invocation};
use koopagit_core::misccmds;
use koopagit_core::settings::Settings;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::menu_actions;
use crate::menus::{Entry, Menu, MenuCache};
use crate::progress::ProgressJob;
use crate::refs::run_job;
use crate::{
    CommitIsOnRefsDialog, Context, DialogResult, FirstStartDialog, msgbox, spawn_launcher,
};

// ---------------------------------------------------------------- daemon

const DAEMON_WARNING: &str = "While running daemon, all data of this repository is exposed without authentication and/or encryption.";

/// The primary non-loopback address, found without sending any packet.
fn local_address() -> Option<std::net::IpAddr> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.0.2.1:9").ok()?;
    s.local_addr().ok().map(|a| a.ip())
}

/// `/command:daemon` (`DaemonCommand.cpp`).
pub fn daemon(ctx: &mut Context) -> DialogResult<i32> {
    if !ctx.settings.bool("DaemonNoSecurityWarning", false) {
        let (choice, dont_ask) = msgbox::ask_with_check(
            "KoopaGit",
            DAEMON_WARNING,
            &["Proceed", "Abort"],
            0,
            Some("Do not show again"),
        );
        if choice != Some(0) {
            return Ok(1);
        }
        // Only "Proceed" is remembered.
        if dont_ask {
            ctx.settings.set_bool("DaemonNoSecurityWarning", true);
            let _ = ctx.settings.save();
        }
    }
    let urls = match local_address() {
        Some(std::net::IpAddr::V6(ip)) => format!("git://[{ip}]/"),
        Some(ip) => format!("git://{ip}/"),
        None => "git://localhost/".to_owned(),
    };
    let ok = crate::progress::run(ProgressJob {
        title: format!("{urls} - Daemon - KoopaGit"),
        git: ctx.repo.git(),
        commands: vec![misccmds::daemon_args(ctx.repo.workdir())],
        post_actions: vec![],
        close_on_end: CloseOnEnd::Manual,
    })?;
    Ok(if ok { 0 } else { 1 })
}

// ---------------------------------------------------------------- pgpfp / updatecheck

/// `/command:pgpfp`: upstream shows the TortoiseGit release signing key.
pub fn pgpfp() -> DialogResult<i32> {
    msgbox::info(
        "KoopaGit",
        "KoopaGit releases are not signed yet, so there is no release signing key fingerprint to show.\n\n(TortoiseGit shows the fingerprint of its own release signing key here.)",
    );
    Ok(0)
}

/// `/command:updatecheck`: there is no koopagit update server.
pub fn updatecheck() -> DialogResult<i32> {
    msgbox::info(
        "Check for Updates",
        &format!(
            "koopagit {}\n\nAutomatic update checks are not available in koopagit. Update it the way you installed it.",
            env!("CARGO_PKG_VERSION")
        ),
    );
    Ok(0)
}

// ---------------------------------------------------------------- commitisonrefs

menu_actions! {
    /// "References commit is on" list menu (`CommitIsOnRefsDlg.cpp:287-315`).
    pub enum RefsAction {
        ShowLog = 1,
        BrowseRepo = 2,
        CompareWorkingTree = 3,
        Copy = 4,
    }
}

pub fn refs_menu(selected: Option<&str>) -> Vec<Entry> {
    let mut m = Menu::new();
    if selected.is_some() {
        m.item(RefsAction::ShowLog, "Show &log");
        m.item(RefsAction::BrowseRepo, "&Browse repository");
        m.item(RefsAction::CompareWorkingTree, "Compare with &working tree");
        m.sep();
        m.item(RefsAction::Copy, "&Copy");
    }
    m.finish()
}

/// `/command:commitisonrefs /rev:<commit>`.
pub fn commit_is_on_refs(ctx: &mut Context) -> DialogResult<i32> {
    let dlg = CommitIsOnRefsDialog::new()?;
    dlg.set_dialog_title(ctx.window_title("References commit is on").into());
    dlg.set_commit(ctx.invocation.get("rev").unwrap_or("HEAD").into());
    let workdir = ctx.repo.workdir().to_owned();
    let all: Rc<RefCell<Vec<String>>> = Rc::default();
    let shown: Rc<RefCell<Vec<String>>> = Rc::default();
    let menu = Rc::new(MenuCache::default());

    let apply_filter = {
        let all = all.clone();
        let shown = shown.clone();
        let weak = dlg.as_weak();
        let menu = menu.clone();
        Rc::new(move || {
            let Some(d) = weak.upgrade() else { return };
            let f = d.get_filter().to_lowercase();
            let v: Vec<String> = all
                .borrow()
                .iter()
                .filter(|r| f.is_empty() || r.to_lowercase().contains(&f))
                .cloned()
                .collect();
            let m: Vec<SharedString> = v.iter().map(|s| s.as_str().into()).collect();
            d.set_refs(ModelRc::new(VecModel::from(m)));
            *shown.borrow_mut() = v;
            menu.invalidate();
        })
    };
    let load = {
        let all = all.clone();
        let weak = dlg.as_weak();
        let workdir = workdir.clone();
        let apply_filter = apply_filter.clone();
        Rc::new(move || {
            let Some(d) = weak.upgrade() else { return };
            let rev = d.get_commit().to_string();
            let repo = Repo::discover(&workdir);
            let (subject, refs, empty) = match repo.as_ref().map(|r| (r, r.rev_parse(&rev))) {
                Ok((r, Ok(id))) => {
                    let subject = r
                        .git()
                        .run(["log", "-1", "--format=%h %s", &id])
                        .map(|s| s.trim().to_owned())
                        .unwrap_or_default();
                    let refs = r.refs_containing(&id).unwrap_or_default();
                    (subject, refs, "No references found.".to_owned())
                }
                _ => (
                    String::new(),
                    vec![],
                    format!("\"{rev}\" is not a valid ref."),
                ),
            };
            d.set_subject(subject.into());
            d.set_empty_text(empty.into());
            *all.borrow_mut() = refs;
            apply_filter();
        })
    };
    load();
    {
        let load = load.clone();
        dlg.on_commit_entered(move || load());
    }
    {
        // F5 (CommitIsOnRefsDlg.cpp:557-561).
        let load = load.clone();
        dlg.on_refresh(move || load());
    }
    {
        // Ctrl+C / Ctrl+Insert in the ref list (CopySelectionToClipboard).
        let shown = shown.clone();
        let weak = dlg.as_weak();
        dlg.on_copy_selected(move || {
            let Some(d) = weak.upgrade() else { return };
            if let Some(r) = shown.borrow().get(d.get_current_row().max(0) as usize) {
                crate::clipboard::copy(&format!("{r}\n"));
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
    {
        let f = apply_filter.clone();
        dlg.on_filter_changed(move || f());
    }
    {
        let weak = dlg.as_weak();
        let workdir = workdir.clone();
        dlg.on_show_log(move || {
            let Some(d) = weak.upgrade() else { return };
            let rev = d.get_commit().to_string();
            spawn_launcher(
                &Invocation::new("log")
                    .with_paths([workdir.clone()])
                    .with("endrev", rev.clone())
                    .with("rev", rev),
            );
        });
    }
    {
        let shown = shown.clone();
        let workdir = workdir.clone();
        dlg.on_ref_activated(move |i| {
            if let Some(r) = shown.borrow().get(i as usize) {
                spawn_launcher(
                    &Invocation::new("log")
                        .with_paths([workdir.clone()])
                        .with("range", r.clone()),
                );
            }
        });
    }
    {
        let shown = shown.clone();
        let menu = menu.clone();
        dlg.on_menu_for(move |i| {
            let sel = shown.borrow().get(i as usize).cloned();
            menu.get(i, || refs_menu(sel.as_deref()))
        });
    }
    {
        let shown = shown.clone();
        let workdir = workdir.clone();
        dlg.on_menu_chosen(move |i, id| {
            use crate::menus::ActionId;
            let Some(r) = shown.borrow().get(i as usize).cloned() else {
                return;
            };
            let launch = |c: &str| Invocation::new(c).with_paths([workdir.clone()]);
            match RefsAction::from_id(id) {
                Some(RefsAction::ShowLog) => spawn_launcher(&launch("log").with("range", r)),
                Some(RefsAction::BrowseRepo) => {
                    spawn_launcher(&launch("repobrowser").with("rev", r))
                }
                Some(RefsAction::CompareWorkingTree) => spawn_launcher(
                    &launch("showcompare")
                        .with("revision1", r)
                        .with("revision2", "0000000000000000000000000000000000000000"),
                ),
                Some(RefsAction::Copy) => {
                    crate::clipboard::copy(&r);
                }
                None => {}
            }
        });
    }
    crate::window::prepare(&dlg);
    dlg.run()?;
    Ok(0)
}

// ---------------------------------------------------------------- firststart

/// SSH client choices: label and `core.sshCommand` value (empty = unset).
pub fn ssh_clients() -> Vec<(&'static str, &'static str)> {
    let mut v = vec![("(git default)", ""), ("OpenSSH (ssh)", "ssh")];
    if cfg!(windows) {
        v.push(("PuTTY (plink)", "plink -batch"));
    }
    v
}

/// Credential helper choices for this platform (`credential.helper`).
pub fn credential_helpers() -> Vec<&'static str> {
    if cfg!(target_os = "macos") {
        vec!["", "osxkeychain", "cache", "store"]
    } else if cfg!(windows) {
        vec!["", "manager", "wincred", "store"]
    } else {
        vec!["", "libsecret", "cache", "store"]
    }
}

fn git_version(settings: &Settings) -> String {
    let path = settings.string(koopagit_core::settings::keys::GIT_EXE_PATH, "");
    let exe = if path.is_empty() {
        PathBuf::from("git")
    } else {
        Path::new(&path).join(if cfg!(windows) { "git.exe" } else { "git" })
    };
    match std::process::Command::new(&exe).arg("--version").output() {
        Ok(o) if o.status.success() => format!(
            "Version: {}",
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .trim_start_matches("git version ")
        ),
        _ => format!("{} could not be run.", exe.display()),
    }
}

fn global_config(key: &str, value: &str) -> Result<(), String> {
    let git = koopagit_core::git_cli::Git::new(std::env::temp_dir());
    let r = if value.is_empty() {
        git.output(["config", "--global", "--unset", key])
            .map(|_| ())
    } else {
        git.run(["config", "--global", key, value]).map(|_| ())
    };
    r.map_err(|e| e.to_string())
}

/// `/command:firststart` — the First Start Wizard.
pub fn firststart(repo: Option<&Repo>, mut settings: Settings) -> DialogResult<i32> {
    let dlg = FirstStartDialog::new()?;
    let git_cfg = |k: &str| {
        koopagit_core::git_cli::Git::new(std::env::temp_dir())
            .run(["config", "--global", "--get", k])
            .map(|v| v.trim().to_owned())
            .unwrap_or_default()
    };
    let _ = repo;
    dlg.set_git_path(
        settings
            .string(koopagit_core::settings::keys::GIT_EXE_PATH, "")
            .into(),
    );
    dlg.set_extra_path(settings.string("MSysGitExtra", "").into());
    dlg.set_git_version(git_version(&settings).into());
    dlg.set_user_name(git_cfg("user.name").into());
    dlg.set_user_email(git_cfg("user.email").into());
    let ssh = ssh_clients();
    let current_ssh = git_cfg("core.sshCommand");
    let ssh_labels: Vec<SharedString> = ssh.iter().map(|(l, _)| (*l).into()).collect();
    dlg.set_ssh_clients(ModelRc::new(VecModel::from(ssh_labels)));
    dlg.set_ssh_index(ssh.iter().position(|(_, v)| *v == current_ssh).unwrap_or(0) as i32);
    let helpers = credential_helpers();
    let current_helper = git_cfg("credential.helper");
    let helper_labels: Vec<SharedString> = helpers
        .iter()
        .map(|h| {
            if h.is_empty() {
                "(none)".into()
            } else {
                (*h).into()
            }
        })
        .collect();
    dlg.set_credential_helpers(ModelRc::new(VecModel::from(helper_labels)));
    dlg.set_credential_index(
        helpers
            .iter()
            .position(|h| *h == current_helper)
            .unwrap_or(0) as i32,
    );

    let finished = Rc::new(RefCell::new(false));
    {
        let weak = dlg.as_weak();
        dlg.on_back(move || {
            if let Some(d) = weak.upgrade() {
                d.set_page((d.get_page() - 1).max(0));
            }
        });
    }
    {
        let weak = dlg.as_weak();
        dlg.on_next(move || {
            let Some(d) = weak.upgrade() else { return };
            // The user page requires both values unless "Don't store" is checked.
            if d.get_page() == 3
                && !d.get_dont_save_user()
                && (d.get_user_name().trim().is_empty() || d.get_user_email().trim().is_empty())
            {
                d.set_notice_text("User name and email must not be empty.".into());
                return;
            }
            d.set_notice_text(SharedString::new());
            d.set_page((d.get_page() + 1).min(4));
        });
    }
    {
        let weak = dlg.as_weak();
        dlg.on_browse_git(move || {
            let Some(d) = weak.upgrade() else { return };
            if let Some(dir) = crate::pick::folder("Git.exe Path", None) {
                d.set_git_path(dir.to_string_lossy().into_owned().into());
            }
        });
    }
    {
        let weak = dlg.as_weak();
        dlg.on_check_git(move || {
            let Some(d) = weak.upgrade() else { return };
            let mut s = Settings::default();
            s.set(
                koopagit_core::settings::keys::GIT_EXE_PATH,
                d.get_git_path().as_str(),
            );
            d.set_git_version(git_version(&s).into());
        });
    }
    {
        let weak = dlg.as_weak();
        let finished = finished.clone();
        dlg.on_finish(move || {
            *finished.borrow_mut() = true;
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
    crate::window::prepare(&dlg);
    dlg.run()?;
    if !*finished.borrow() {
        return Ok(1);
    }
    settings.set(
        koopagit_core::settings::keys::GIT_EXE_PATH,
        dlg.get_git_path().as_str(),
    );
    settings.set("MSysGitExtra", dlg.get_extra_path().as_str());
    settings.save()?;
    let mut errors = Vec::new();
    if !dlg.get_dont_save_user() {
        for (k, v) in [
            ("user.name", dlg.get_user_name()),
            ("user.email", dlg.get_user_email()),
        ] {
            if let Err(e) = global_config(k, v.trim()) {
                errors.push(e);
            }
        }
    }
    if !dlg.get_dont_save_auth() {
        let ssh_value = ssh
            .get(dlg.get_ssh_index().max(0) as usize)
            .map(|(_, v)| *v)
            .unwrap_or("");
        if ssh_value != current_ssh {
            if let Err(e) = global_config("core.sshCommand", ssh_value) {
                errors.push(e);
            }
        }
        let helper = helpers
            .get(dlg.get_credential_index().max(0) as usize)
            .copied()
            .unwrap_or("");
        if helper != current_helper {
            if let Err(e) = global_config("credential.helper", helper) {
                errors.push(e);
            }
        }
    }
    if !errors.is_empty() {
        msgbox::error("First Start Wizard", &errors.join("\n"));
        return Ok(1);
    }
    Ok(0)
}

// ---------------------------------------------------------------- drag and drop

fn drop_target(ctx: &Context) -> Option<PathBuf> {
    ctx.invocation
        .get("droptarget")
        .filter(|t| !t.is_empty())
        .map(PathBuf::from)
}

/// `/command:dropmove /droptarget:<dir> [/rename]` (`DropMoveCommand.cpp`).
pub fn dropmove(ctx: &mut Context) -> DialogResult<i32> {
    let Some(target) = drop_target(ctx) else {
        return Ok(1);
    };
    let same_tree = Repo::discover(&target)
        .map(|r| r.workdir() == ctx.repo.workdir())
        .unwrap_or(false);
    if !same_tree {
        msgbox::error(
            "KoopaGit",
            "Target and source must be in the same working tree!",
        );
        return Ok(1);
    }
    let target_rel = ctx.repo.relative_path(&target).unwrap_or_default();
    let sources: Vec<PathBuf> = ctx
        .paths
        .iter()
        .filter(|p| !p.components().any(|c| c.as_os_str() == ".git"))
        .cloned()
        .collect();
    let join = |name: &str| {
        if target_rel.is_empty() {
            name.to_owned()
        } else {
            format!("{target_rel}/{name}")
        }
    };
    let mut new_name = None;
    if ctx.invocation.has("rename") && sources.len() == 1 {
        let name = sources[0]
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let base = target.clone();
        match crate::fileops::input("Move and rename", "New name:", &name, move |v| {
            base.join(v).exists().then(|| "The file exists.".to_owned())
        })? {
            Some(n) => new_name = Some(n),
            None => return Ok(1),
        }
    }
    let git = ctx.repo.git();
    let mut moved = 0usize;
    for src in &sources {
        let Some(src_rel) = ctx.repo.relative_path(src) else {
            continue;
        };
        let mut name = new_name.clone().unwrap_or_else(|| {
            src.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        if target.join(&name).exists() {
            let base = target.clone();
            match crate::fileops::input(
                &format!("Move: New name for {name}"),
                "New name:",
                &name,
                move |v| base.join(v).exists().then(|| "The file exists.".to_owned()),
            )? {
                Some(n) => name = n,
                None => return Ok(1),
            }
        }
        let out = git.output(misccmds::mv_args(&src_rel, &join(&name)))?;
        // On failure DropMoveCommand.cpp:130 asks Ignore/Abort, Abort default.
        if out.success() {
            moved += 1;
        } else if msgbox::ask_default("KoopaGit", &out.stderr, &["Ignore", "Abort"], 1) != Some(0) {
            return Ok(1);
        }
    }
    Ok(if moved == sources.len() { 0 } else { 1 })
}

/// `/command:dropcopyadd /droptarget:<dir>` (`DropCopyAddCommand.cpp`).
pub fn dropcopyadd(ctx: &mut Context) -> DialogResult<i32> {
    let Some(target) = drop_target(ctx) else {
        return Ok(1);
    };
    if Repo::discover(&target)
        .map(|r| r.workdir() != ctx.repo.workdir())
        .unwrap_or(true)
    {
        return Ok(1);
    }
    let mut copied = Vec::new();
    for src in ctx.paths.clone() {
        if src == target {
            continue;
        }
        let Some(name) = src.file_name() else {
            continue;
        };
        let dest = target.join(name);
        if dest.exists() {
            if dest.is_dir() && src.is_dir() {
                continue;
            }
            match msgbox::ask(
                "KoopaGit",
                &format!(
                    "The file\n{}\nalready exists! Do you want to overwrite it?",
                    dest.display()
                ),
                &["Overwrite", "Keep", "Cancel"],
            ) {
                Some(0) => {}
                Some(1) => {
                    copied.push(dest);
                    continue;
                }
                _ => return Ok(1),
            }
        }
        match misccmds::copy_recursive(&src, &dest) {
            Ok(files) => copied.extend(files),
            Err(e) => {
                msgbox::error("KoopaGit", &format!("Error while copying files:\n{e}"));
                return Ok(1);
            }
        }
    }
    let rel: Vec<String> = copied
        .iter()
        .filter_map(|p| ctx.repo.relative_path(p))
        .filter(|p| !p.is_empty())
        .collect();
    if rel.is_empty() {
        return Ok(0);
    }
    let ok = run_job(ctx, "Add", vec![koopagit_core::fileops::add(&rel)], vec![])?;
    Ok(if ok { 0 } else { 1 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refs_menu_and_choices() {
        assert!(refs_menu(None).is_empty());
        let m = refs_menu(Some("main"));
        assert_eq!(m.len(), 5);
        assert!(credential_helpers().contains(&""));
        assert_eq!(ssh_clients()[0].1, "");
    }
}
