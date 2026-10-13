//! Bridging menu actions to non-Rust extensions: menu → JSON, and performing
//! an action (`koopagit-shell invoke ...`).
//!
//! Every menu item carries an `invoke` array: the extension runs
//! `koopagit-shell invoke <invoke...>` when the item is clicked, so the
//! extension never needs to understand the action itself.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use koopagit_core::launcher::Invocation;
use koopagit_core::settings::Settings;

use crate::json::Value;
use crate::menu::{Action, ContextMenu, DIFF_LATER_KEY, Node, SUBMENU_LABEL, strip_accelerators};

/// The `invoke` arguments that perform `action`.
pub fn invoke_args(action: &Action) -> Vec<String> {
    match action {
        Action::Launch(inv) => std::iter::once("launch".to_owned())
            .chain(inv.to_args())
            .collect(),
        Action::LaunchMerge(args) => std::iter::once("merge-viewer".to_owned())
            .chain(args.iter().cloned())
            .collect(),
        Action::RememberDiffPath(p) => {
            vec!["remember-diff".to_owned(), p.to_string_lossy().into_owned()]
        }
    }
}

fn node_json(node: &Node) -> Value {
    match node {
        Node::Separator => Value::object().with("kind", "separator"),
        Node::Item {
            label,
            help,
            action,
        } => {
            let mut v = Value::object()
                .with("kind", "item")
                .with("label", strip_accelerators(label))
                .with("help", help.as_str())
                .with("invoke", invoke_args(action));
            if let Action::Launch(inv) = action {
                v = v.with("command", inv.command.as_str());
            }
            v
        }
        Node::Submenu { label, children } => Value::object()
            .with("kind", "submenu")
            .with("label", strip_accelerators(label))
            .with(
                "children",
                children.iter().map(node_json).collect::<Vec<_>>(),
            ),
    }
}

/// Renders the menu: `{"top":[...],"submenu":{"label":"KoopaGit","children":[...]}}`.
pub fn menu_json(menu: &ContextMenu) -> String {
    Value::object()
        .with("top", menu.top.iter().map(node_json).collect::<Vec<_>>())
        .with(
            "submenu",
            Value::object()
                .with("label", strip_accelerators(SUBMENU_LABEL))
                .with(
                    "children",
                    menu.submenu.iter().map(node_json).collect::<Vec<_>>(),
                ),
        )
        .render()
}

/// The full menu query an extension performs (`QueryContextMenu` early exits
/// included): `NoContextPaths`, paths inside `.git`, and
/// `HideMenusForUnversionedItems` produce an empty menu.
pub fn query_menu(
    items: &[PathBuf],
    background: Option<&Path>,
    extended: bool,
    settings: &Settings,
) -> ContextMenu {
    use crate::flags::*;
    use koopagit_core::settings::keys;

    let no_context = settings.string(keys::NO_CONTEXT_PATHS, "");
    let all: Vec<&Path> = items
        .iter()
        .map(PathBuf::as_path)
        .chain(background)
        .collect();
    if all.is_empty()
        || all.iter().any(|p| {
            crate::pathfilter::is_no_context_path(&no_context, p)
                || p.components().any(|c| c.as_os_str() == ".git")
        })
    {
        return ContextMenu::default();
    }
    let sel = crate::selection::analyse(items, background, extended);
    if settings.bool(keys::HIDE_MENUS_FOR_UNVERSIONED, false)
        && !extended
        && sel.flags & (INGIT | INVERSIONEDFOLDER | FOLDERINGIT | BAREREPO | INACCESSIBLE) == 0
    {
        return ContextMenu::default();
    }
    crate::menu::build(&sel, &crate::menu::MenuSettings::from_settings(settings))
}

/// Locates an executable shipped next to the helper (`koopagit-proc`,
/// `koopagit-merge`), honouring an environment override.
pub fn sibling_exe(name: &str, env_override: &str) -> PathBuf {
    if let Some(p) = std::env::var_os(env_override) {
        return PathBuf::from(p);
    }
    let file = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    };
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join(&file)))
        .filter(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from(file))
}

/// Starts a process without waiting for it, detached from our stdio, so a
/// file-manager extension never blocks on a dialog.
fn spawn_detached(exe: &Path, args: &[String]) -> std::io::Result<()> {
    Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
}

/// Performs `invoke` arguments. `settings` is updated (and saved) for the
/// "Diff later" state.
pub fn invoke(args: &[String], settings: &mut Settings) -> Result<(), String> {
    invoke_with(
        args,
        settings,
        &sibling_exe("koopagit-proc", "KOOPAGIT_PROC"),
        &sibling_exe("koopagit-merge", "KOOPAGIT_MERGE"),
    )
}

/// [`invoke`] with explicit launcher and merge-viewer executables (in-process
/// extensions cannot use `current_exe`, which is the file manager).
pub fn invoke_with(
    args: &[String],
    settings: &mut Settings,
    proc_exe: &Path,
    merge_exe: &Path,
) -> Result<(), String> {
    let Some((kind, rest)) = args.split_first() else {
        return Err("invoke: missing action".to_owned());
    };
    match kind.as_str() {
        "launch" => {
            let inv = Invocation::parse(rest).map_err(|e| e.to_string())?;
            // "Diff with <stored>" consumes the remembered path.
            if inv.command == "diff" {
                let stored = settings.string(DIFF_LATER_KEY, "");
                if !stored.is_empty() && inv.get("path2") == Some(stored.as_str()) {
                    settings.remove(DIFF_LATER_KEY);
                    settings.save().map_err(|e| e.to_string())?;
                }
            }
            let inv = crate::pathfile::use_pathfile(&inv, &crate::pathfile::temp_dir())
                .map_err(|e| e.to_string())?;
            spawn_detached(proc_exe, &inv.to_args())
                .map_err(|e| format!("failed to start {}: {e}", proc_exe.display()))
        }
        "merge-viewer" => spawn_detached(merge_exe, rest)
            .map_err(|e| format!("failed to start {}: {e}", merge_exe.display())),
        "remember-diff" => {
            let path = rest.first().ok_or("remember-diff: missing path")?;
            settings.set(DIFF_LATER_KEY, path);
            settings.save().map_err(|e| e.to_string())
        }
        "forget-diff" => {
            settings.remove(DIFF_LATER_KEY);
            settings.save().map_err(|e| e.to_string())
        }
        other => Err(format!("invoke: unknown action '{other}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flags::*;
    use crate::menu::{MenuSettings, Selection, build};

    #[test]
    fn menu_renders_with_invoke_arrays() {
        let sel = Selection {
            flags: FOLDER | ONLYONE,
            folder: Some(PathBuf::from("/w")),
            ..Default::default()
        };
        let json = menu_json(&build(&sel, &MenuSettings::default()));
        assert!(json.starts_with(r#"{"top":[{"kind":"item","label":"Git Clone...""#));
        assert!(json.contains(r#""invoke":["launch","/command:clone","/path:/w"]"#));
        assert!(json.contains(r#""submenu":{"label":"KoopaGit","children":["#));
    }

    #[test]
    fn remember_and_forget_diff_path() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Settings::load(&dir.path().join("s.ini")).unwrap();
        invoke(&["remember-diff".into(), "/a.txt".into()], &mut s).unwrap();
        assert_eq!(s.string(DIFF_LATER_KEY, ""), "/a.txt");
        let reloaded = Settings::load(&dir.path().join("s.ini")).unwrap();
        assert_eq!(reloaded.string(DIFF_LATER_KEY, ""), "/a.txt");
        invoke(&["forget-diff".into()], &mut s).unwrap();
        assert_eq!(s.string(DIFF_LATER_KEY, ""), "");
        assert!(invoke(&["bogus".into()], &mut s).is_err());
    }
}
