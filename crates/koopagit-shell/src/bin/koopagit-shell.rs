//! `koopagit-shell`: the helper non-Rust file-manager extensions call.
//!
//! ```text
//! koopagit-shell menu [--extended] [--background <dir>] [-- <paths...>]
//! koopagit-shell status <paths...>
//! koopagit-shell invoke <action> [args...]
//! ```
//!
//! `menu` prints the context menu as JSON, `status` prints one
//! `<overlay-id>\t<path>` line per path (`none` when no overlay applies), and
//! `invoke` performs the `invoke` array of a menu item.

use std::path::PathBuf;
use std::process::ExitCode;

use koopagit_core::settings::Settings;
use koopagit_shell::actions;
use koopagit_shell::overlay::{OverlaySettings, overlay_for_path};

const USAGE: &str = "usage:
  koopagit-shell menu [--extended] [--background <dir>] [-- <paths...>]
  koopagit-shell status <paths...>
  koopagit-shell invoke <launch|merge-viewer|remember-diff|forget-diff> [args...]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((cmd, rest)) = args.split_first() else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };
    let mut settings = Settings::load_default();
    match cmd.as_str() {
        "menu" => {
            let mut extended = false;
            let mut background = None;
            let mut paths = Vec::new();
            let mut it = rest.iter();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--extended" => extended = true,
                    "--background" => background = it.next().map(PathBuf::from),
                    "--" => paths.extend(it.by_ref().map(PathBuf::from)),
                    other => paths.push(PathBuf::from(other)),
                }
            }
            let menu = actions::query_menu(&paths, background.as_deref(), extended, &settings);
            println!("{}", actions::menu_json(&menu));
            ExitCode::SUCCESS
        }
        "status" => {
            let s = OverlaySettings::from_settings(&settings);
            for p in rest {
                let id = overlay_for_path(&PathBuf::from(p), &s).map_or("none", |o| o.id());
                println!("{id}\t{p}");
            }
            ExitCode::SUCCESS
        }
        "invoke" => match actions::invoke(rest, &mut settings) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("koopagit-shell: {e}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}
