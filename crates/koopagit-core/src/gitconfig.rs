//! Reading and writing git configuration per level, as TortoiseGit's
//! `CGitSettings` does for the Git, Dialogs 3 and Remote settings pages.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::git_cli::{Git, GitOutput};
use crate::{Error, Result};

/// The "Config source" radio buttons (`CGitSettings::ConfigType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConfigScope {
    /// Merged view of every level (read-only in the UI).
    Effective,
    /// `.git/config`.
    Local,
    /// `.tgitconfig` in the work-tree root (TortoiseGit project properties).
    Project,
    /// `~/.gitconfig` (or the XDG file).
    Global,
    /// The system-wide gitconfig.
    System,
}

impl ConfigScope {
    pub fn label(self) -> &'static str {
        match self {
            ConfigScope::Effective => "Effective",
            ConfigScope::Local => "Local",
            ConfigScope::Project => "Project",
            ConfigScope::Global => "Global",
            ConfigScope::System => "System",
        }
    }
}

/// Access to git config levels for one working directory (or none).
#[derive(Debug, Clone)]
pub struct GitConfig {
    git: Git,
    workdir: Option<PathBuf>,
    envs: Vec<(OsString, OsString)>,
}

impl GitConfig {
    /// `workdir` is the work-tree root when the settings dialog runs inside a
    /// repository; otherwise only Effective/Global/System are available.
    pub fn new(workdir: Option<&Path>) -> Self {
        let dir = workdir
            .map(Path::to_owned)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        GitConfig {
            git: Git::new(dir),
            workdir: workdir.map(Path::to_owned),
            envs: Vec::new(),
        }
    }

    /// Extra environment for every git call (tests isolate the global config
    /// with `GIT_CONFIG_GLOBAL` / `GIT_CONFIG_NOSYSTEM`).
    pub fn with_env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.envs.push((key.into(), value.into()));
        self
    }

    pub fn has_repo(&self) -> bool {
        self.workdir.is_some()
    }

    pub fn workdir(&self) -> Option<&Path> {
        self.workdir.as_deref()
    }

    /// The `.tgitconfig` path of the work tree.
    pub fn project_file(&self) -> Option<PathBuf> {
        self.workdir.as_ref().map(|w| w.join(".tgitconfig"))
    }

    fn scope_args(&self, scope: ConfigScope) -> Result<Vec<String>> {
        Ok(match scope {
            ConfigScope::Effective => vec![],
            ConfigScope::Local => {
                if !self.has_repo() {
                    return Err(Error::InvalidCommandLine("no repository".into()));
                }
                vec!["--local".into()]
            }
            ConfigScope::Project => {
                let f = self
                    .project_file()
                    .ok_or_else(|| Error::InvalidCommandLine("no repository".into()))?;
                vec!["--file".into(), f.to_string_lossy().into_owned()]
            }
            ConfigScope::Global => vec!["--global".into()],
            ConfigScope::System => vec!["--system".into()],
        })
    }

    fn output(&self, args: &[String]) -> Result<GitOutput> {
        let mut cmd = self.git.command(args);
        cmd.envs(self.envs.iter().map(|(k, v)| (k, v)));
        let out = cmd.output()?;
        Ok(GitOutput {
            code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }

    fn run(&self, args: &[String]) -> Result<String> {
        let out = self.output(args)?;
        if out.success() {
            Ok(out.stdout)
        } else {
            Err(Error::GitFailed {
                args: args.join(" "),
                code: out.code,
                stderr: out.stderr,
            })
        }
    }

    /// The value of `key` at `scope`, or `None` when it is not set there.
    /// For Effective, `.tgitconfig` is consulted below the local config when
    /// `honor_project` is set (Dialogs 3 / Issue Tracker pages).
    pub fn get(&self, scope: ConfigScope, key: &str, honor_project: bool) -> Option<String> {
        let mut args = vec!["config".to_owned()];
        args.extend(self.scope_args(scope).ok()?);
        args.extend(["--get".to_owned(), key.to_owned()]);
        match self.output(&args) {
            Ok(out) if out.success() => Some(out.stdout.trim_end_matches('\n').to_owned()),
            _ if scope == ConfigScope::Effective && honor_project => {
                // Local and global win over the project file; only fall back to it.
                self.get(ConfigScope::Project, key, false)
            }
            _ => None,
        }
    }

    /// Writes `key = value` at `scope` (not Effective).
    pub fn set(&self, scope: ConfigScope, key: &str, value: &str) -> Result<()> {
        if scope == ConfigScope::Effective {
            return Err(Error::InvalidCommandLine(
                "cannot save to the effective configuration".into(),
            ));
        }
        let mut args = vec!["config".to_owned()];
        args.extend(self.scope_args(scope)?);
        args.extend([key.to_owned(), value.to_owned()]);
        self.run(&args).map(drop)
    }

    /// Removes `key` from `scope`; a key that is not set is not an error.
    pub fn unset(&self, scope: ConfigScope, key: &str) -> Result<()> {
        let mut args = vec!["config".to_owned()];
        args.extend(self.scope_args(scope)?);
        args.extend(["--unset-all".to_owned(), key.to_owned()]);
        let out = self.output(&args)?;
        // Exit code 5: the key was not set.
        if out.success() || out.code == Some(5) {
            Ok(())
        } else {
            Err(Error::GitFailed {
                args: args.join(" "),
                code: out.code,
                stderr: out.stderr,
            })
        }
    }

    /// Sets the value, or unsets it when `value` is `None` (inherit / empty /
    /// indeterminate), like `CGitSettings::Save(..., deleteKey)`.
    pub fn store(&self, scope: ConfigScope, key: &str, value: Option<&str>) -> Result<()> {
        match value {
            Some(v) => self.set(scope, key, v),
            None => self.unset(scope, key),
        }
    }

    /// The path of the config file of a level, for the "Edit ..." buttons.
    pub fn file_of(&self, scope: ConfigScope) -> Option<PathBuf> {
        match scope {
            ConfigScope::Effective => None,
            ConfigScope::Local => self
                .run(&["rev-parse".into(), "--git-path".into(), "config".into()])
                .ok()
                .map(|p| {
                    let p = PathBuf::from(p.trim());
                    match &self.workdir {
                        Some(w) if p.is_relative() => w.join(p),
                        _ => p,
                    }
                }),
            ConfigScope::Project => self.project_file(),
            ConfigScope::Global => {
                if let Some(f) = self
                    .envs
                    .iter()
                    .find(|(k, _)| k == "GIT_CONFIG_GLOBAL")
                    .map(|(_, v)| PathBuf::from(v))
                {
                    return Some(f);
                }
                let home = home_dir()?;
                let classic = home.join(".gitconfig");
                let xdg = xdg_config_file();
                // Git writes to the XDG file if it exists and ~/.gitconfig doesn't.
                match xdg {
                    Some(x) if x.exists() && !classic.exists() => Some(x),
                    _ => Some(classic),
                }
            }
            ConfigScope::System => self
                .run(&[
                    "config".into(),
                    "--system".into(),
                    "--list".into(),
                    "--show-origin".into(),
                ])
                .ok()
                .and_then(|out| {
                    out.lines()
                        .next()
                        .and_then(|l| l.strip_prefix("file:"))
                        .and_then(|l| l.split('\t').next())
                        .map(PathBuf::from)
                }),
        }
    }

    /// `git config --show-origin -l` ("View effective config").
    pub fn effective_listing(&self) -> Result<String> {
        self.run(&["config".into(), "--show-origin".into(), "-l".into()])
    }

    /// Names of the configured remotes (`git remote`), sorted.
    pub fn remotes(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .run(&["remote".into()])
            .map(|o| o.lines().map(str::to_owned).collect())
            .unwrap_or_default();
        v.sort();
        v
    }

    /// Runs a remote-management command line (`git remote add|rm|rename ...`).
    pub fn run_git(&self, args: &[String]) -> Result<String> {
        self.run(args)
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// `$XDG_CONFIG_HOME/git/config` or `~/.config/git/config`.
pub fn xdg_config_file() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|h| h.join(".config")))
        .map(|d| d.join("git").join("config"))
}

/// Parses a git boolean (`true/yes/on/1`, `false/no/off/0`).
pub fn parse_bool(v: &str) -> Option<bool> {
    match v.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

/// The Git page's combo parsing rule (`SettingGitConfig.cpp:173-219`): a true
/// boolean shows `true`; a non-boolean equal (case-insensitively) to one of
/// `specials` shows that; anything else shows `false`.
pub fn normalize_combo(raw: &str, specials: &[&str]) -> String {
    match parse_bool(raw) {
        Some(true) => "true".to_owned(),
        Some(false) => "false".to_owned(),
        None => specials
            .iter()
            .find(|s| s.eq_ignore_ascii_case(raw.trim()))
            .map(|s| (*s).to_owned())
            .unwrap_or_else(|| "false".to_owned()),
    }
}

/// Tri-state checkbox value of a boolean key: `None` = unset (indeterminate).
pub fn tristate(raw: Option<&str>) -> Option<bool> {
    raw.and_then(parse_bool)
}

/// `remote.<name>.tagopt` <-> the Tags combo ("Reachable", "None", "All").
pub fn tagopt_index(raw: Option<&str>) -> usize {
    match raw.map(str::trim) {
        Some("--no-tags") => 1,
        Some("--tags") => 2,
        _ => 0,
    }
}

pub fn tagopt_value(index: usize) -> Option<&'static str> {
    match index {
        1 => Some("--no-tags"),
        2 => Some("--tags"),
        _ => None,
    }
}

/// `git remote add -- <name> <url>`.
pub fn remote_add(name: &str, url: &str) -> Vec<String> {
    vec![
        "remote".into(),
        "add".into(),
        "--".into(),
        name.into(),
        url.replace('\\', "/"),
    ]
}

/// `git remote rm -- <name>`.
pub fn remote_remove(name: &str) -> Vec<String> {
    vec!["remote".into(), "rm".into(), "--".into(), name.into()]
}

/// `git remote rename -- <old> <new>`.
pub fn remote_rename(old: &str, new: &str) -> Vec<String> {
    vec![
        "remote".into(),
        "rename".into(),
        "--".into(),
        old.into(),
        new.into(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn isolated() -> (tempfile::TempDir, GitConfig) {
        let dir = tempfile::tempdir().unwrap();
        let wt = dir.path().join("wt");
        std::fs::create_dir(&wt).unwrap();
        Git::new(&wt).run(["init", "-q"]).unwrap();
        let global = dir.path().join("global.gitconfig");
        std::fs::write(&global, "").unwrap();
        let cfg = GitConfig::new(Some(&wt))
            .with_env("GIT_CONFIG_GLOBAL", &global)
            .with_env("GIT_CONFIG_NOSYSTEM", "1");
        (dir, cfg)
    }

    #[test]
    fn levels_are_read_and_written_independently() {
        let (_dir, cfg) = isolated();
        cfg.set(ConfigScope::Global, "user.name", "Global Name")
            .unwrap();
        assert_eq!(
            cfg.get(ConfigScope::Effective, "user.name", false)
                .as_deref(),
            Some("Global Name")
        );
        assert_eq!(cfg.get(ConfigScope::Local, "user.name", false), None);
        cfg.set(ConfigScope::Local, "user.name", "Local Name")
            .unwrap();
        assert_eq!(
            cfg.get(ConfigScope::Effective, "user.name", false)
                .as_deref(),
            Some("Local Name")
        );
        cfg.store(ConfigScope::Local, "user.name", None).unwrap();
        cfg.unset(ConfigScope::Local, "user.name").unwrap();
        assert_eq!(
            cfg.get(ConfigScope::Effective, "user.name", false)
                .as_deref(),
            Some("Global Name")
        );
        assert!(cfg.set(ConfigScope::Effective, "a.b", "c").is_err());
        assert!(cfg.file_of(ConfigScope::Local).unwrap().ends_with("config"));
        assert!(
            cfg.file_of(ConfigScope::Global)
                .unwrap()
                .ends_with("global.gitconfig")
        );
    }

    #[test]
    fn project_file_is_a_fallback_for_effective() {
        let (_dir, cfg) = isolated();
        cfg.set(ConfigScope::Project, "tgit.logminsize", "10")
            .unwrap();
        assert!(cfg.project_file().unwrap().exists());
        assert_eq!(
            cfg.get(ConfigScope::Effective, "tgit.logminsize", true)
                .as_deref(),
            Some("10")
        );
        assert_eq!(
            cfg.get(ConfigScope::Effective, "tgit.logminsize", false),
            None
        );
        cfg.set(ConfigScope::Local, "tgit.logminsize", "3").unwrap();
        assert_eq!(
            cfg.get(ConfigScope::Effective, "tgit.logminsize", true)
                .as_deref(),
            Some("3")
        );
    }

    #[test]
    fn remotes_commands() {
        let (_dir, cfg) = isolated();
        cfg.run_git(&remote_add("origin", r"C:\x\y")).unwrap();
        assert_eq!(
            cfg.get(ConfigScope::Local, "remote.origin.url", false)
                .as_deref(),
            Some("C:/x/y")
        );
        cfg.run_git(&remote_rename("origin", "up")).unwrap();
        assert_eq!(cfg.remotes(), ["up"]);
        cfg.run_git(&remote_remove("up")).unwrap();
        assert!(cfg.remotes().is_empty());
    }

    #[test]
    fn parsing_rules() {
        assert_eq!(normalize_combo("TRUE", &["input"]), "true");
        assert_eq!(normalize_combo("Input", &["input"]), "input");
        assert_eq!(normalize_combo("weird", &["warn"]), "false");
        assert_eq!(tristate(Some("off")), Some(false));
        assert_eq!(tristate(None), None);
        assert_eq!(tagopt_index(Some("--tags")), 2);
        assert_eq!(tagopt_value(1), Some("--no-tags"));
    }
}
