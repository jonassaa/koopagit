//! `koopagit-merge` — TortoiseGitMerge-compatible viewer:
//! `/base:<f> /mine:<f>` (diff), `+ /theirs:<f> /merged:<f>` (conflicts),
//! `/diff:<patch>` (unified diff). Set `KOOPAGIT_SNAPSHOT=<png>` to render once.

use std::process::ExitCode;

use koopagit_core::launcher::Invocation;
use koopagit_dialogs::{diffcmd, msgbox};

fn main() -> ExitCode {
    let inv = match Invocation::parse(std::env::args().skip(1)) {
        Ok(i) => i,
        Err(e) => {
            msgbox::error("KoopaGitMerge", &e.to_string());
            return ExitCode::FAILURE;
        }
    };
    match diffcmd::merge_app(&inv) {
        Ok(0) => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(e) => {
            msgbox::error("KoopaGitMerge", &e.to_string());
            ExitCode::FAILURE
        }
    }
}
