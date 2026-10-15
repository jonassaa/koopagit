//! `/command:sendmail` — the Send Patch dialog (`CSendMailDlg`,
//! `CAppUtils::SendPatchMail`).
//!
//! Delivery: SMTP (direct or configured) goes through `git send-email` with the
//! Email settings page's server; MAPI (or a missing `git send-email`) opens the
//! default mail client with a `mailto:` link. See `docs/divergences.md`.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use koopagit_core::misccmds::{self, MailPatch};
use koopagit_core::settings::Settings;
use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::refs::run_job;
use crate::{Context, DialogResult, MailPatchRow, SendMailDialog, msgbox, spawn_launcher};

pub const DELIVERY_KEY: &str = r"TortoiseProc\SendMail\DeliveryType";
const ADDRESS_HISTORY: &str = r"TortoiseProc\EmailAddress";

/// `TortoiseProc\SendMail\DeliveryType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    SmtpDirect,
    Mapi,
    SmtpConfigured,
}

impl Delivery {
    pub fn from_settings(s: &Settings) -> Self {
        match s.dword(DELIVERY_KEY, 0) {
            1 => Delivery::Mapi,
            2 => Delivery::SmtpConfigured,
            _ => Delivery::SmtpDirect,
        }
    }
}

/// What the dialog decided.
struct Request {
    to: Vec<String>,
    cc: Vec<String>,
    subject: String,
    attachment: bool,
    combine: bool,
    patches: Vec<PathBuf>,
}

/// `git send-email` options for the configured SMTP server.
fn smtp_args(s: &Settings) -> Vec<String> {
    let mut a = Vec::new();
    let server = s.string(r"TortoiseProc\SendMail\Address", "");
    if !server.is_empty() {
        a.push(format!("--smtp-server={server}"));
        a.push(format!(
            "--smtp-server-port={}",
            s.dword(r"TortoiseProc\SendMail\Port", 25)
        ));
        match s.dword(r"TortoiseProc\SendMail\Encryption", 0) {
            1 => a.push("--smtp-encryption=tls".to_owned()),
            2 => a.push("--smtp-encryption=ssl".to_owned()),
            _ => {}
        }
    }
    a
}

fn has_send_email(ctx: &Context) -> bool {
    ctx.repo
        .git()
        .output(["send-email", "--dump-aliases"])
        .is_ok_and(|o| o.success())
}

fn open_url(url: &str) -> bool {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener).arg(url).spawn().is_ok()
}

/// Mail bodies for the `mailto:` route (`CSendMailPatch::SendAsSingleMail` /
/// `SendAsCombinedMail`): `(subject, body)` per message.
fn mailto_messages(req: &Request) -> Vec<(String, String)> {
    let parsed: Vec<(PathBuf, MailPatch)> = req
        .patches
        .iter()
        .filter_map(|p| {
            std::fs::read_to_string(p)
                .ok()
                .map(|t| (p.clone(), misccmds::parse_patch(&t)))
        })
        .collect();
    let attach_note = |p: &PathBuf| format!("(attach: {})\n", p.display());
    if req.combine {
        let body: String = parsed
            .iter()
            .map(|(p, m)| {
                if req.attachment {
                    format!("{}\n{}", m.subject, attach_note(p))
                } else {
                    m.body.clone()
                }
            })
            .collect();
        vec![(req.subject.clone(), body)]
    } else {
        parsed
            .into_iter()
            .map(|(p, m)| {
                let body = if req.attachment {
                    attach_note(&p)
                } else {
                    m.body
                };
                (m.subject, body)
            })
            .collect()
    }
}

/// `/command:sendmail /path:<patch>*<patch>...`.
pub fn run(ctx: &mut Context) -> DialogResult<i32> {
    let patches: Vec<PathBuf> = ctx.paths.iter().filter(|p| p.is_file()).cloned().collect();
    let delivery = Delivery::from_settings(&ctx.settings);
    let dlg = SendMailDialog::new()?;
    dlg.set_dialog_title(ctx.window_title("Send Patch").into());
    let rows: Vec<MailPatchRow> = patches
        .iter()
        .map(|p| MailPatchRow {
            checked: true,
            path: p.to_string_lossy().into_owned().into(),
        })
        .collect();
    let model = Rc::new(VecModel::from(rows));
    dlg.set_patches(ModelRc::from(model.clone()));
    if let [only] = patches.as_slice()
        && let Ok(t) = std::fs::read_to_string(only)
    {
        dlg.set_subject(misccmds::parse_patch(&t).subject.into());
    }
    let via_mailto = delivery == Delivery::Mapi || !has_send_email(ctx);
    dlg.set_delivery_note(
        if via_mailto {
            "The patches are handed to your default mail client (mailto:); attachments must be added there."
        } else {
            "The patches are sent with git send-email (configure the server under Settings > Network > Email)."
        }
        .into(),
    );
    let history = ctx.settings.history(ADDRESS_HISTORY, "email");
    if let Some(last) = history.first() {
        dlg.set_to(last.as_str().into());
    }

    let request: Rc<RefCell<Option<Request>>> = Rc::default();
    {
        let model = model.clone();
        dlg.on_patch_toggled(move |i, c| {
            if let Some(mut r) = model.row_data(i as usize) {
                r.checked = c;
                model.set_row_data(i as usize, r);
            }
        });
    }
    {
        let model = model.clone();
        dlg.on_patch_activated(move |i| {
            if let Some(r) = model.row_data(i as usize) {
                // Opens the patch in the unified-diff viewer.
                spawn_launcher(
                    &koopagit_core::launcher::Invocation::new("diff")
                        .with("path", r.path.as_str())
                        .with_switch("unified"),
                );
            }
        });
    }
    {
        let workdir = ctx.repo.workdir().to_owned();
        dlg.on_email_settings(move || {
            spawn_launcher(
                &koopagit_core::launcher::Invocation::new("settings")
                    .with_paths([workdir.clone()])
                    .with("page", "smtp"),
            );
        });
    }
    dlg.on_combine_toggled(|| {});
    {
        let weak = dlg.as_weak();
        let request = request.clone();
        let model = model.clone();
        dlg.on_send(move || {
            let Some(d) = weak.upgrade() else { return };
            let to = misccmds::split_addresses(d.get_to().as_str());
            let cc = misccmds::split_addresses(d.get_cc().as_str());
            if to.is_empty() && cc.is_empty() && delivery != Delivery::Mapi {
                d.set_notice_text("Email Address To and CC can't empty at the same time.".into());
                return;
            }
            let patches: Vec<PathBuf> = model
                .iter()
                .filter(|r| r.checked)
                .map(|r| PathBuf::from(r.path.as_str()))
                .collect();
            if patches.is_empty() {
                d.set_notice_text("Select at least one patch.".into());
                return;
            }
            *request.borrow_mut() = Some(Request {
                to,
                cc,
                subject: d.get_subject().to_string(),
                attachment: d.get_attachment(),
                combine: d.get_combine(),
                patches,
            });
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
    let Some(req) = request.take() else {
        return Ok(1);
    };
    for a in req.to.iter().chain(req.cc.iter()) {
        ctx.settings
            .push_history(ADDRESS_HISTORY, "email", a, 0xFFFF);
    }
    let _ = ctx.settings.save();

    if via_mailto {
        for (subject, body) in mailto_messages(&req) {
            if !open_url(&misccmds::mailto(&req.to, &req.cc, &subject, &body)) {
                msgbox::error("Send Patch", "Could not open the default mail client.");
                return Ok(1);
            }
        }
        return Ok(0);
    }
    let mut args = misccmds::send_email_args(
        &req.to,
        &req.cc,
        req.combine.then_some(req.subject.as_str()),
        &req.patches,
    );
    // Server options go right after `send-email`, ahead of the patch files.
    args.splice(1..1, smtp_args(&ctx.settings));
    if req.attachment {
        args.insert(1, "--attach".to_owned());
    }
    let ok = run_job(ctx, "Send Patch", vec![args], vec![])?;
    Ok(if ok { 0 } else { 1 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mailto_bodies() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("0001.patch");
        std::fs::write(&p, "From x\nSubject: [PATCH] One\n\nbody\n").unwrap();
        let mut req = Request {
            to: vec!["a@b".into()],
            cc: vec![],
            subject: "Series".into(),
            attachment: false,
            combine: false,
            patches: vec![p.clone()],
        };
        assert_eq!(
            mailto_messages(&req),
            [("[PATCH] One".to_owned(), "body\n".to_owned())]
        );
        req.combine = true;
        req.attachment = true;
        let m = mailto_messages(&req);
        assert_eq!(m[0].0, "Series");
        assert!(m[0].1.starts_with("[PATCH] One\n(attach: "));
        let mut s = Settings::default();
        assert!(smtp_args(&s).is_empty());
        s.set(r"TortoiseProc\SendMail\Address", "smtp.x");
        s.set(r"TortoiseProc\SendMail\Encryption", 1);
        assert_eq!(
            smtp_args(&s),
            [
                "--smtp-server=smtp.x",
                "--smtp-server-port=25",
                "--smtp-encryption=tls"
            ]
        );
        assert_eq!(Delivery::from_settings(&s), Delivery::SmtpDirect);
    }
}
