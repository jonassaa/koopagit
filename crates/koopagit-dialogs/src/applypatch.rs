//! `/command:importpatch` — `CImportPatchDlg` ("Apply Patches"): applies a
//! series of patch files with `git am`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use koopagit_core::extras::{self, AmControl, AmOptions};
use koopagit_core::git_cli::Git;
use koopagit_core::launcher::Invocation;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::{ApplyPatchDialog, Context, DialogResult, PatchRow, spawn_launcher};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowState {
    Pending = 0,
    Applying = 1,
    Applied = 2,
    Failed = 3,
    Skipped = 4,
}

#[derive(Debug, Clone)]
struct Row {
    path: PathBuf,
    checked: bool,
    state: RowState,
}

#[derive(Default)]
struct Shared {
    rows: Vec<Row>,
    /// Index of the next patch to apply (`m_CurrentItem`).
    current: usize,
    running: bool,
    log: String,
}

impl Shared {
    fn finished(&self) -> bool {
        !self.rows.is_empty() && self.current >= self.rows.len()
    }

    fn log_line(&mut self, line: &str) {
        self.log.push_str(line);
        if !line.ends_with('\n') {
            self.log.push('\n');
        }
    }
}

/// Pending answer in the notice bar.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ask {
    None,
    RebaseApplyFound,
    AmActiveOnCancel,
}

fn refresh(d: &ApplyPatchDialog, sh: &Shared) {
    let rows: Vec<PatchRow> = sh
        .rows
        .iter()
        .map(|r| PatchRow {
            checked: r.checked,
            name: r.path.to_string_lossy().as_ref().into(),
            state: r.state as i32,
        })
        .collect();
    d.set_rows(ModelRc::new(VecModel::from(rows)));
    d.set_running(sh.running);
    d.set_finished(sh.finished());
    d.set_log_text(sh.log.clone().into());
}

fn notice(d: &ApplyPatchDialog, text: &str, buttons: &[&str], error: bool) {
    d.set_notice_text(text.into());
    let labels: Vec<SharedString> = buttons.iter().map(|b| (*b).into()).collect();
    d.set_notice_buttons(ModelRc::new(VecModel::from(labels)));
    d.set_notice_error(error);
}

/// Moves the row at `i` by `delta` (Up = -1, Down = +1); returns the new index.
fn move_row<T>(rows: &mut [T], i: usize, delta: isize) -> Option<usize> {
    let j = i as isize + delta;
    if j < 0 || j as usize >= rows.len() || i >= rows.len() {
        return None;
    }
    rows.swap(i, j as usize);
    Some(j as usize)
}

/// `ImportPatchCommand`: the patch files may live outside any repository. Then
/// the current directory is used, else the user picks the repository
/// (`CAppUtils::ChooseRepository`).
pub fn run_choosing_repository(inv: &koopagit_core::launcher::Invocation) -> DialogResult<i32> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let repo = match koopagit_core::Repo::discover(&cwd) {
        Ok(r) => r,
        Err(_) => {
            let Some(dir) = crate::pick::folder("Choose repository", None) else {
                return Ok(1);
            };
            match koopagit_core::Repo::discover(&dir) {
                Ok(r) => r,
                Err(_) => {
                    crate::msgbox::error(
                        "KoopaGit",
                        &format!("\"{}\" is not a working tree root.", dir.display()),
                    );
                    return Ok(1);
                }
            }
        }
    };
    let mut ctx = Context {
        repo,
        settings: koopagit_core::settings::Settings::load_default(),
        invocation: inv.clone(),
        paths: inv.paths()?,
    };
    run(&mut ctx)
}

pub fn run(ctx: &mut Context) -> DialogResult<i32> {
    let dlg = ApplyPatchDialog::new()?;
    dlg.set_dialog_title(ctx.window_title("Apply Patches").into());
    let workdir = ctx.repo.workdir().to_owned();
    let shared = Arc::new(Mutex::new(Shared {
        rows: ctx
            .paths
            .iter()
            .filter(|p| !p.is_dir())
            .map(|p| Row {
                path: p.clone(),
                checked: true,
                state: RowState::Pending,
            })
            .collect(),
        ..Default::default()
    }));
    let exit = Arc::new(AtomicBool::new(false));
    let ask = Arc::new(Mutex::new(Ask::None));
    refresh(&dlg, &shared.lock().unwrap());
    let (name, email) = ctx.repo.user_identity();
    let has_identity = !name.is_empty() && !email.is_empty();

    {
        let shared = shared.clone();
        let weak = dlg.as_weak();
        dlg.on_row_toggled(move |i, c| {
            if let Some(r) = shared.lock().unwrap().rows.get_mut(i as usize) {
                r.checked = c;
            }
            if let Some(d) = weak.upgrade() {
                refresh(&d, &shared.lock().unwrap());
            }
        });
    }
    {
        // CPatchListCtrl: View Patch / Review Patch with KoopaGitMerge
        // (`StartExtPatch`) / Send Mail... (PatchListCtrl.cpp:85).
        let path_of = {
            let shared = shared.clone();
            move |i: i32| {
                shared
                    .lock()
                    .unwrap()
                    .rows
                    .get(usize::try_from(i).ok()?)
                    .map(|r| r.path.clone())
            }
        };
        let merge = |args: Vec<String>| {
            let exe = crate::diffcmd::sibling_exe("koopagit-merge");
            if let Err(e) = std::process::Command::new(exe).args(args).spawn() {
                eprintln!("koopagit: {e}");
            }
        };
        let diff_arg = |p: &PathBuf| format!("/diff:{}", p.to_string_lossy());
        {
            let path_of = path_of.clone();
            dlg.on_view_patch(move |i| {
                if let Some(p) = path_of(i) {
                    merge(vec![diff_arg(&p)]);
                }
            });
        }
        {
            let path_of = path_of.clone();
            let workdir = workdir.clone();
            dlg.on_review_patch(move |i| {
                if let Some(p) = path_of(i) {
                    merge(vec![
                        diff_arg(&p),
                        format!("/patchpath:{}", workdir.to_string_lossy()),
                    ]);
                }
            });
        }
        let workdir = workdir.clone();
        dlg.on_send_mail(move |i| {
            if let Some(p) = path_of(i) {
                // Send Mail mails the files among its paths; the working tree
                // folder lets it find the repository for a patch kept outside.
                spawn_launcher(&Invocation::new("sendmail").with_paths([p, workdir.clone()]));
            }
        });
    }
    {
        let shared = shared.clone();
        let weak = dlg.as_weak();
        dlg.on_row_selected(move |i| {
            let Some(d) = weak.upgrade() else { return };
            let path = shared
                .lock()
                .unwrap()
                .rows
                .get(i as usize)
                .map(|r| r.path.clone());
            if let Some(p) = path {
                let text = std::fs::read(&p)
                    .map(|b| {
                        // IDS_ERR_FILE_TOOBIG: don't load huge patches into the view.
                        if b.len() > 2 * 1024 * 1024 {
                            "The file is too big to be shown.".to_owned()
                        } else {
                            String::from_utf8_lossy(&b).into_owned()
                        }
                    })
                    .unwrap_or_else(|e| e.to_string());
                d.set_patch_text(text.into());
                d.set_tab(0);
            }
        });
    }
    {
        let shared = shared.clone();
        let weak = dlg.as_weak();
        let start = workdir.clone();
        dlg.on_add(move || {
            let files = crate::pick::open_files(
                "Add patches",
                Some(&start),
                &[
                    ("Patchfiles (*.diff, *.patch)", &["diff", "patch"]),
                    ("All Files", &["*"]),
                ],
            );
            let mut sh = shared.lock().unwrap();
            for f in files {
                if !sh.rows.iter().any(|r| r.path == f) {
                    sh.rows.push(Row {
                        path: f,
                        checked: true,
                        state: RowState::Pending,
                    });
                }
            }
            if let Some(d) = weak.upgrade() {
                refresh(&d, &sh);
            }
        });
    }
    for (delta, up) in [(-1isize, true), (1, false)] {
        let shared = shared.clone();
        let weak = dlg.as_weak();
        let handler = move || {
            let Some(d) = weak.upgrade() else { return };
            let mut sh = shared.lock().unwrap();
            let cur = d.get_current_row();
            if cur < 0 {
                return;
            }
            if let Some(j) = move_row(&mut sh.rows, cur as usize, delta) {
                d.set_current_row(j as i32);
            }
            refresh(&d, &sh);
        };
        if up {
            dlg.on_up(handler);
        } else {
            dlg.on_down(handler);
        }
    }
    {
        let shared = shared.clone();
        let weak = dlg.as_weak();
        dlg.on_remove(move || {
            let Some(d) = weak.upgrade() else { return };
            let mut sh = shared.lock().unwrap();
            let cur = d.get_current_row();
            if cur >= 0 && (cur as usize) < sh.rows.len() {
                sh.rows.remove(cur as usize);
                if sh.current > cur as usize {
                    sh.current -= 1;
                }
                d.set_current_row(-1);
            }
            refresh(&d, &sh);
        });
    }

    let opts_of = |d: &ApplyPatchDialog| AmOptions {
        three_way: d.get_three_way(),
        ignore_space_change: d.get_ignore_space(),
        sign_off: d.get_sign_off(),
        keep_cr: d.get_keep_cr(),
    };

    let start_thread = {
        let shared = shared.clone();
        let exit = exit.clone();
        let weak = dlg.as_weak();
        let workdir = workdir.clone();
        move |opts: AmOptions| {
            exit.store(false, Ordering::SeqCst);
            shared.lock().unwrap().running = true;
            let shared = shared.clone();
            let exit = exit.clone();
            let weak = weak.clone();
            let workdir = workdir.clone();
            std::thread::spawn(move || {
                let git = Git::new(&workdir);
                let push = |shared: &Arc<Mutex<Shared>>| {
                    let shared = shared.clone();
                    let _ =
                        weak.upgrade_in_event_loop(move |d| refresh(&d, &shared.lock().unwrap()));
                };
                loop {
                    let (i, row) = {
                        let mut sh = shared.lock().unwrap();
                        let i = sh.current;
                        if i >= sh.rows.len() || exit.load(Ordering::SeqCst) {
                            break;
                        }
                        sh.rows[i].state = RowState::Applying;
                        (i, sh.rows[i].clone())
                    };
                    push(&shared);
                    if row.checked {
                        let args = extras::am_args(&opts, &row.path.to_string_lossy());
                        shared
                            .lock()
                            .unwrap()
                            .log_line(&format!("git {}", args.join(" ")));
                        let out = git.output(&args);
                        let mut sh = shared.lock().unwrap();
                        match out {
                            Ok(o) if o.success() => {
                                sh.rows[i].state = RowState::Applied;
                                sh.log_line("Success");
                            }
                            Ok(o) => {
                                // Keep the row so the user can retry it.
                                sh.rows[i].state = RowState::Failed;
                                sh.log_line(&format!("{}{}", o.stdout, o.stderr));
                                sh.log_line("Fail");
                                break;
                            }
                            Err(e) => {
                                sh.rows[i].state = RowState::Failed;
                                sh.log_line(&e.to_string());
                                sh.log_line("Fail");
                                break;
                            }
                        }
                    } else {
                        let mut sh = shared.lock().unwrap();
                        let line = format!("Skip Patch: {}", row.path.to_string_lossy());
                        sh.log_line(&line);
                        sh.rows[i].state = RowState::Skipped;
                    }
                    shared.lock().unwrap().current = i + 1;
                    push(&shared);
                }
                shared.lock().unwrap().running = false;
                push(&shared);
            });
        }
    };
    let start_thread = std::rc::Rc::new(start_thread);

    {
        let shared = shared.clone();
        let weak = dlg.as_weak();
        let ask = ask.clone();
        let start_thread = start_thread.clone();
        let workdir = workdir.clone();
        dlg.on_apply(move || {
            let Some(d) = weak.upgrade() else { return };
            if shared.lock().unwrap().finished() {
                let _ = d.hide();
                return;
            }
            if !has_identity {
                notice(
                    &d,
                    "User name and email must be set before commit.",
                    &[],
                    true,
                );
                return;
            }
            notice(&d, "", &[], false);
            d.set_tab(1);
            if koopagit_core::Repo::discover(&workdir).is_ok_and(|r| r.am_in_progress()) {
                *ask.lock().unwrap() = Ask::RebaseApplyFound;
                notice(
                    &d,
                    "previous rebase directory rebase-apply still exists but mbox given.\n\nDo you want to",
                    &["Abort", "Skip", "Resolved"],
                    true,
                );
                return;
            }
            // Retry a failed patch from where it stopped.
            {
                let mut sh = shared.lock().unwrap();
                let cur = sh.current;
                if let Some(r) = sh.rows.get_mut(cur) {
                    r.state = RowState::Pending;
                }
            }
            start_thread(opts_of(&d));
            refresh(&d, &shared.lock().unwrap());
        });
    }
    {
        let shared = shared.clone();
        let weak = dlg.as_weak();
        let ask = ask.clone();
        let workdir = workdir.clone();
        dlg.on_notice_clicked(move |i| {
            let Some(d) = weak.upgrade() else { return };
            let which = std::mem::replace(&mut *ask.lock().unwrap(), Ask::None);
            notice(&d, "", &[], false);
            let git = Git::new(&workdir);
            match which {
                Ask::RebaseApplyFound => {
                    let control = match i {
                        0 => AmControl::Abort,
                        1 => AmControl::Skip,
                        _ => AmControl::Resolved,
                    };
                    let args = extras::am_control_args(control);
                    let mut sh = shared.lock().unwrap();
                    sh.log_line(&format!("git {}", args.join(" ")));
                    match git.output(&args) {
                        Ok(o) if o.success() => sh.log_line("Done"),
                        Ok(o) => {
                            sh.log_line(&format!("{}{}", o.stdout, o.stderr));
                            sh.log_line("Fail");
                        }
                        Err(e) => sh.log_line(&e.to_string()),
                    }
                    if control == AmControl::Skip {
                        let cur = sh.current;
                        if let Some(r) = sh.rows.get_mut(cur) {
                            r.state = RowState::Skipped;
                        }
                        sh.current += 1;
                    }
                    refresh(&d, &sh);
                }
                Ask::AmActiveOnCancel => match i {
                    0 => {
                        if let Ok(o) = git.output(extras::am_control_args(AmControl::Abort)) {
                            if !o.success() {
                                notice(&d, &o.stderr, &[], true);
                                return;
                            }
                        }
                        let _ = d.hide();
                    }
                    1 => {
                        let _ = d.hide();
                    }
                    _ => {}
                },
                Ask::None => {}
            }
        });
    }
    {
        let shared = shared.clone();
        let weak = dlg.as_weak();
        let exit = exit.clone();
        let ask = ask.clone();
        dlg.on_cancel(move || {
            let Some(d) = weak.upgrade() else { return };
            if shared.lock().unwrap().running {
                exit.store(true, Ordering::SeqCst);
                return;
            }
            if koopagit_core::Repo::discover(&workdir).is_ok_and(|r| r.am_in_progress()) {
                *ask.lock().unwrap() = Ask::AmActiveOnCancel;
                notice(
                    &d,
                    "\"git am\" is still in apply mode.\nDo you want to abort?",
                    &["Yes", "No", "Cancel"],
                    false,
                );
                return;
            }
            let _ = d.hide();
        });
    }

    crate::snapshot::arm(&dlg);
    dlg.run()?;
    let ok = shared.lock().unwrap().finished();
    Ok(if ok { 0 } else { 1 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_rows() {
        let mut v = vec![1, 2, 3];
        assert_eq!(move_row(&mut v, 0, -1), None);
        assert_eq!(move_row(&mut v, 0, 1), Some(1));
        assert_eq!(v, [2, 1, 3]);
        assert_eq!(move_row(&mut v, 2, 1), None);
        let mut sh = Shared::default();
        assert!(!sh.finished());
        sh.rows.push(Row {
            path: "a.patch".into(),
            checked: true,
            state: RowState::Pending,
        });
        sh.current = 1;
        assert!(sh.finished());
    }
}
