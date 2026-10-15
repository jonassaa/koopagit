//! `CProgressDlg`: run git commands, stream output, offer post-command buttons.

use std::io::Read;
use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex};

use koopagit_core::git_cli::Git;
use koopagit_core::launcher::{CloseOnEnd, Invocation};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::{ProgressDialog, spawn_launcher};

/// When a post-command button is offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    Success,
    Failure,
    Always,
    /// On failure when the git output contains this text (e.g. `! [rejected]`).
    FailureWith(&'static str),
}

/// A post-command button. Clicking it launches another `/command:`, or, when
/// `git` is non-empty, runs those git command lines in a new progress window
/// (e.g. Merge's "Remove branch": `git branch -D -- <b>`).
#[derive(Debug, Clone)]
pub struct PostAction {
    pub label: String,
    pub when: When,
    pub invocation: Invocation,
    pub git: Vec<Vec<String>>,
}

impl PostAction {
    pub fn on_success(label: &str, invocation: Invocation) -> Self {
        PostAction {
            label: label.to_owned(),
            when: When::Success,
            invocation,
            git: Vec::new(),
        }
    }

    pub fn on_failure(label: &str, invocation: Invocation) -> Self {
        PostAction {
            label: label.to_owned(),
            when: When::Failure,
            invocation,
            git: Vec::new(),
        }
    }

    /// A button that runs git commands in-process instead of launching a command.
    pub fn git(label: &str, when: When, commands: Vec<Vec<String>>) -> Self {
        PostAction {
            label: label.to_owned(),
            when,
            invocation: Invocation::new("about"),
            git: commands,
        }
    }
}

/// The caption a post-command button has upstream, with its `&` access key
/// (shell string table IDS_MENU*, IDS_MSGBOX_RETRY, ...; ProgressDlg builds
/// Alt+letter accelerators from them). Labels that already carry `&`, or have
/// no upstream twin, are kept.
fn post_caption(label: &str) -> String {
    if label.contains('&') {
        return label.to_owned();
    }
    let (base, dots) = match label.strip_suffix("...") {
        Some(b) => (b, "..."),
        None => (label, ""),
    };
    let caption = match base {
        "Commit" => "&Commit",
        "Fetch" => "F&etch",
        "Show log" => "Show &log",
        "Merge" => "&Merge",
        "Pull" => "&Pull",
        "Push" => "Pus&h",
        "Switch/Checkout" => "S&witch/Checkout",
        "Create Tag" => "Create &Tag",
        "Merge unrelated history" => "&Merge unrelated history",
        "Retry" => "&Retry",
        "ReCommit" => "&ReCommit",
        "Remove branch" => "Remove &branch",
        "Clean up" => "&Clean up",
        "Revert" => "Re&vert",
        "Resolve" => "Res&olve",
        _ => base,
    };
    format!("{caption}{dots}")
}

/// What to run in the progress dialog.
#[derive(Debug, Clone)]
pub struct ProgressJob {
    pub title: String,
    pub git: Git,
    /// Command lines (arguments after `git`), run in order; stops at the first failure.
    pub commands: Vec<Vec<String>>,
    pub post_actions: Vec<PostAction>,
    pub close_on_end: CloseOnEnd,
}

/// Accumulates git output, honouring `\r` progress updates the way the
/// TortoiseGit progress log does (the line is overwritten in place).
#[derive(Debug, Default, Clone)]
pub struct OutputBuffer {
    lines: Vec<String>,
    overwrite_last: bool,
}

impl OutputBuffer {
    pub fn push_str(&mut self, chunk: &str) {
        for (i, segment) in chunk.split('\n').enumerate() {
            if i > 0 {
                self.overwrite_last = false;
                self.lines.push(String::new());
            }
            for (j, part) in segment.split('\r').enumerate() {
                if j > 0 {
                    self.overwrite_last = true;
                    if !part.is_empty() {
                        if let Some(last) = self.lines.last_mut() {
                            last.clear();
                        }
                    }
                }
                if part.is_empty() {
                    continue;
                }
                match self.lines.last_mut() {
                    Some(last) => last.push_str(part),
                    None => self.lines.push(part.to_owned()),
                }
            }
        }
    }

    pub fn push_line(&mut self, line: &str) {
        if self.lines.last().is_some_and(|l| !l.is_empty()) {
            self.lines.push(String::new());
        }
        self.push_str(line);
        self.lines.push(String::new());
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }
}

struct Shared {
    out: OutputBuffer,
    child: Option<Child>,
    aborted: bool,
}

/// Shows the progress dialog and runs the job. Returns whether every command succeeded.
pub fn run(job: ProgressJob) -> Result<bool, slint::PlatformError> {
    let (dlg, success) = start_inner(job, None)?;
    crate::window::prepare(&dlg);
    dlg.run()?;
    let ok = *success.lock().unwrap();
    Ok(ok)
}

type DoneCallbacks = std::collections::HashMap<u64, Box<dyn FnOnce(bool)>>;

thread_local! {
    static ON_DONE: std::cell::RefCell<DoneCallbacks> = std::cell::RefCell::new(DoneCallbacks::new());
}

/// Shows the progress dialog without blocking, for use from inside another
/// dialog's event loop. `on_done` runs on the UI thread when the job has finished.
pub fn start(
    job: ProgressJob,
    on_done: impl FnOnce(bool) + 'static,
) -> Result<ProgressDialog, slint::PlatformError> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    ON_DONE.with(|m| m.borrow_mut().insert(id, Box::new(on_done)));
    let (dlg, _) = start_inner(job, Some(id))?;
    dlg.show()?;
    Ok(dlg)
}

fn start_inner(
    job: ProgressJob,
    done_id: Option<u64>,
) -> Result<(ProgressDialog, Arc<Mutex<bool>>), slint::PlatformError> {
    let dlg = ProgressDialog::new()?;
    crate::window::apply_theme(&dlg);
    dlg.set_dialog_title(job.title.clone().into());
    let shared = Arc::new(Mutex::new(Shared {
        out: OutputBuffer::default(),
        child: None,
        aborted: false,
    }));
    let success = Arc::new(Mutex::new(false));

    {
        let shared = shared.clone();
        dlg.on_abort(move || {
            let mut s = shared.lock().unwrap();
            s.aborted = true;
            if let Some(c) = s.child.as_mut() {
                let _ = c.kill();
            }
        });
    }
    {
        let weak = dlg.as_weak();
        dlg.on_close_dialog(move || {
            if let Some(d) = weak.upgrade() {
                let _ = d.hide();
            }
        });
    }

    let weak = dlg.as_weak();
    let post_git = job.git.clone();
    let thread_shared = shared.clone();
    let thread_success = success.clone();
    std::thread::spawn(move || {
        let ok = run_commands(&job, &thread_shared, &weak);
        *thread_success.lock().unwrap() = ok;
        let aborted = thread_shared.lock().unwrap().aborted;
        let output = thread_shared.lock().unwrap().out.text();
        let actions: Vec<PostAction> = job
            .post_actions
            .iter()
            .filter(|a| match a.when {
                When::Always => true,
                When::Success => ok,
                When::Failure => !ok,
                When::FailureWith(needle) => !ok && output.contains(needle),
            })
            .cloned()
            .collect();
        let close = ok
            && match job.close_on_end {
                CloseOnEnd::Manual => false,
                CloseOnEnd::IfNoOptions => actions.is_empty(),
                CloseOnEnd::IfNoErrors => true,
            };
        let text = thread_shared.lock().unwrap().out.text();
        let _ = weak.upgrade_in_event_loop(move |d| {
            if let Some(cb) = done_id.and_then(|id| ON_DONE.with(|m| m.borrow_mut().remove(&id))) {
                cb(ok);
            }
            d.set_output(text.into());
            d.set_running(false);
            d.set_failed(!ok);
            d.set_status_text(
                if aborted {
                    "Aborted"
                } else if ok {
                    "Success"
                } else {
                    "Failed"
                }
                .into(),
            );
            let (labels, keys): (Vec<SharedString>, Vec<SharedString>) = actions
                .iter()
                .map(|a| {
                    let (label, key) = crate::msgbox::split_mnemonic(&post_caption(&a.label));
                    (
                        label.into(),
                        key.map(String::from).unwrap_or_default().into(),
                    )
                })
                .unzip();
            d.set_post_actions(ModelRc::new(VecModel::from(labels)));
            d.set_post_keys(ModelRc::new(VecModel::from(keys)));
            let w = d.as_weak();
            d.on_post_action(move |i| {
                if let Some(a) = actions.get(i as usize) {
                    if a.git.is_empty() {
                        spawn_launcher(&a.invocation);
                    } else {
                        let follow_up = ProgressJob {
                            title: a.label.replace('&', ""),
                            git: post_git.clone(),
                            commands: a.git.clone(),
                            post_actions: Vec::new(),
                            close_on_end: CloseOnEnd::Manual,
                        };
                        if let Err(e) = start(follow_up, |_| {}) {
                            eprintln!("koopagit: {e}");
                        }
                    }
                    if let Some(d) = w.upgrade() {
                        let _ = d.hide();
                    }
                }
            });
            if close {
                let _ = d.hide();
            }
        });
    });

    Ok((dlg, success))
}

fn run_commands(
    job: &ProgressJob,
    shared: &Arc<Mutex<Shared>>,
    weak: &slint::Weak<ProgressDialog>,
) -> bool {
    let refresh = |shared: &Arc<Mutex<Shared>>| {
        let text = shared.lock().unwrap().out.text();
        let _ = weak.upgrade_in_event_loop(move |d| d.set_output(text.into()));
    };
    for args in &job.commands {
        if shared.lock().unwrap().aborted {
            return false;
        }
        shared
            .lock()
            .unwrap()
            .out
            .push_line(&format!("git {}", args.join(" ")));
        refresh(shared);
        let started = std::time::Instant::now();
        let mut cmd = job.git.command(args);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                shared.lock().unwrap().out.push_line(&format!("error: {e}"));
                refresh(shared);
                return false;
            }
        };
        let readers: Vec<_> = [
            child
                .stdout
                .take()
                .map(|s| Box::new(s) as Box<dyn Read + Send>),
            child
                .stderr
                .take()
                .map(|s| Box::new(s) as Box<dyn Read + Send>),
        ]
        .into_iter()
        .flatten()
        .map(|mut r| {
            let shared = shared.clone();
            let weak = weak.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 4096];
                while let Ok(n) = r.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    let text = {
                        let mut s = shared.lock().unwrap();
                        s.out.push_str(&String::from_utf8_lossy(&buf[..n]));
                        s.out.text()
                    };
                    let _ = weak.upgrade_in_event_loop(move |d| d.set_output(text.into()));
                }
            })
        })
        .collect();
        shared.lock().unwrap().child = Some(child);
        for r in readers {
            let _ = r.join();
        }
        let status = shared.lock().unwrap().child.take().map(|mut c| c.wait());
        let code = match status {
            Some(Ok(s)) => s.code(),
            _ => None,
        };
        let ok = code == Some(0);
        // "Show git.exe execution timings and timestamp" (ShowGitexeTimings, default on).
        shared.lock().unwrap().out.push_line(&format!(
            "{} ({} ms @ {})",
            if ok {
                "Success".to_owned()
            } else {
                format!(
                    "git did not exit cleanly (exit code {})",
                    code.map_or_else(|| "?".to_owned(), |c| c.to_string())
                )
            },
            started.elapsed().as_millis(),
            crate::format::now_hms(),
        ));
        refresh(shared);
        if !ok {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_command_access_keys() {
        assert_eq!(post_caption("Push..."), "Pus&h...");
        assert_eq!(post_caption("Retry"), "&Retry");
        assert_eq!(post_caption("&Commit..."), "&Commit...");
        assert_eq!(post_caption("Pulled Diff"), "Pulled Diff");
    }

    #[test]
    fn carriage_returns_overwrite_progress_lines() {
        let mut b = OutputBuffer::default();
        b.push_line("git fetch");
        b.push_str("Receiving objects:  10%\rReceiving objects:  50%");
        b.push_str("\rReceiving objects: 100%, done.\nFrom x\n");
        assert_eq!(
            b.text(),
            "git fetch\nReceiving objects: 100%, done.\nFrom x\n"
        );
    }
}
