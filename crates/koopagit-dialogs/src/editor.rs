//! TortoiseGit's "alternative editor" (Settings > External Programs >
//! Alternative editor): used for config files, "View revision in alternative
//! editor" and generated text. Without one configured upstream falls back to
//! Notepad; here it is the platform's default text editor.

use std::path::Path;
use std::process::Command;

use koopagit_core::settings::Settings;

const KEY: &str = "AlternativeEditor";

/// The configured editor (`CAppUtils::IsAlternativeEditorConfigured`,
/// AppUtils.cpp:676): set and not disabled with the `#` prefix.
pub fn configured(settings: &Settings) -> Option<String> {
    let editor = settings.string(KEY, "");
    (!editor.is_empty() && !editor.starts_with('#')).then_some(editor)
}

/// Opens `path` in the alternative editor, detached
/// (`CAppUtils::LaunchAlternativeEditor`, AppUtils.cpp:684).
pub fn launch(path: &Path) -> std::io::Result<()> {
    match configured(&Settings::load_default()) {
        Some(editor) => Command::new(editor).arg(path).spawn().map(drop),
        None => platform::command(path).spawn().map(drop),
    }
}

/// [`launch`] for a config file, created empty first if it doesn't exist.
pub fn open(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, "")?;
    }
    launch(path)
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;

    pub fn command(path: &Path) -> Command {
        let mut c = Command::new("notepad.exe");
        c.arg(path);
        c
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;

    pub fn command(path: &Path) -> Command {
        // -t: the default text editor, regardless of the file's extension.
        let mut c = Command::new("open");
        c.arg("-t").arg(path);
        c
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod platform {
    use super::*;

    pub fn command(path: &Path) -> Command {
        let mut c = Command::new("xdg-open");
        c.arg(path);
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_editor_rules() {
        let mut s = Settings::default();
        assert_eq!(configured(&s), None);
        s.set(KEY, "#/usr/bin/vim");
        assert_eq!(configured(&s), None);
        s.set(KEY, "/usr/bin/vim");
        assert_eq!(configured(&s).as_deref(), Some("/usr/bin/vim"));
    }
}
