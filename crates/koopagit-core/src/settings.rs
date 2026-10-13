//! TortoiseGit's per-user registry (`HKCU\Software\TortoiseGit`), stored as a
//! plain text file so it works on every OS.
//!
//! Keys keep their TortoiseGit names, with `\` separating sub-keys (e.g.
//! `LogDialog\NumberOfLogs`), so the settings map 1:1 onto upstream docs.
//! Values are typed at the call site with a default, like `CRegDWORD`/`CRegString`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::Result;

/// Default values of settings read by more than one crate. Keys are the
/// upstream registry value names.
pub mod keys {
    /// Low/high halves of the "show on first level of the context menu" mask.
    pub const CONTEXT_MENU_ENTRIES: &str = "ContextMenuEntries";
    pub const CONTEXT_MENU_ENTRIES_HIGH: &str = "ContextMenuEntrieshigh";
    /// Low/high halves of the "only show with Shift" mask.
    pub const CONTEXT_MENU_EXT_ENTRIES_LOW: &str = "ContextMenuExtEntriesLow";
    pub const CONTEXT_MENU_EXT_ENTRIES_HIGH: &str = "ContextMenuExtEntriesHigh";
    pub const HIDE_MENUS_FOR_UNVERSIONED: &str = "HideMenusForUnversionedItems";
    pub const NO_CONTEXT_PATHS: &str = "NoContextPaths";
    pub const CACHE_TYPE: &str = "CacheType";
    pub const OVERLAY_EXCLUDE_LIST: &str = "OverlayExcludeList";
    pub const OVERLAY_INCLUDE_LIST: &str = "OverlayIncludeList";
    pub const SHOW_EXCLUDED_AS_NORMAL: &str = "ShowExcludedAsNormal";
    pub const UNVERSIONED_AS_MODIFIED: &str = "UnversionedAsModified";
    pub const GIT_EXE_PATH: &str = "MSysGit";
    pub const NUMBER_OF_LOGS: &str = r"LogDialog\NumberOfLogs";
    pub const NUMBER_OF_LOGS_SCALE: &str = r"LogDialog\NumberOfLogsScale";
    pub const AUTO_CLOSE_GIT_PROGRESS: &str = "AutoCloseGitProgress";
    pub const MAX_HISTORY_ITEMS: &str = "MaxHistoryItems";
    pub const SELECT_FILES_FOR_COMMIT: &str = "SelectFilesForCommit";
    pub const AUTOSELECT_MISSING_FILES: &str = "AutoselectMissingFiles";
    pub const STRIP_COMMENTED_LINES: &str = "StripCommentedLines";
    pub const FETCH_VERBOSE: &str = "FetchVerbose";
    pub const DIFF: &str = "Diff";
    pub const MERGE: &str = "Merge";
}

/// The settings store.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    path: Option<PathBuf>,
    values: BTreeMap<String, String>,
}

impl Settings {
    /// Location of the settings file:
    /// `$KOOPAGIT_SETTINGS`, else `%APPDATA%\koopagit\settings.ini` on Windows,
    /// `$XDG_CONFIG_HOME/koopagit/settings.ini` or `~/.config/koopagit/settings.ini` elsewhere.
    pub fn default_path() -> Option<PathBuf> {
        if let Some(p) = std::env::var_os("KOOPAGIT_SETTINGS") {
            return Some(PathBuf::from(p));
        }
        let base = if cfg!(windows) {
            std::env::var_os("APPDATA").map(PathBuf::from)
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        }?;
        Some(base.join("koopagit").join("settings.ini"))
    }

    /// Loads the user's settings; a missing or unreadable file yields defaults.
    pub fn load_default() -> Self {
        match Self::default_path() {
            Some(p) => Self::load(&p).unwrap_or_else(|_| Settings {
                path: Some(p),
                values: BTreeMap::new(),
            }),
            None => Settings::default(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let mut values = BTreeMap::new();
        match std::fs::read_to_string(path) {
            Ok(text) => {
                for line in text.lines() {
                    let line = line.trim_start();
                    if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                        continue;
                    }
                    if let Some((k, v)) = line.split_once('=') {
                        values.insert(k.trim().to_owned(), unescape(v.trim()));
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        Ok(Settings {
            path: Some(path.to_owned()),
            values,
        })
    }

    pub fn save(&self) -> Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut out = String::from("# KoopaGit settings (TortoiseGit registry values)\n");
        for (k, v) in &self.values {
            out.push_str(&format!("{k} = {}\n", escape(v)));
        }
        std::fs::write(path, out)?;
        Ok(())
    }

    pub fn string(&self, key: &str, default: &str) -> String {
        self.values
            .get(key)
            .cloned()
            .unwrap_or_else(|| default.to_owned())
    }

    pub fn dword(&self, key: &str, default: u32) -> u32 {
        self.values
            .get(key)
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }

    pub fn qword(&self, key: &str, default: u64) -> u64 {
        self.values
            .get(key)
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }

    pub fn bool(&self, key: &str, default: bool) -> bool {
        self.values
            .get(key)
            .and_then(|v| v.parse::<u32>().ok())
            .map(|v| v != 0)
            .unwrap_or(default)
    }

    pub fn set(&mut self, key: &str, value: impl ToString) {
        self.values.insert(key.to_owned(), value.to_string());
    }

    pub fn set_bool(&mut self, key: &str, value: bool) {
        self.set(key, u32::from(value));
    }

    pub fn remove(&mut self, key: &str) {
        self.values.remove(key);
    }

    /// Removes every key below `prefix\` (Saved Data page "Clear").
    pub fn remove_prefix(&mut self, prefix: &str) {
        let p = format!("{prefix}\\");
        self.values.retain(|k, _| !k.starts_with(&p));
    }

    /// Reads a history list stored like TortoiseGit's `CHistoryCombo`:
    /// `<key>\<name>0`, `<key>\<name>1`, ...
    pub fn history(&self, key: &str, name: &str) -> Vec<String> {
        (0..)
            .map_while(|i| self.values.get(&format!("{key}\\{name}{i}")).cloned())
            .collect()
    }

    /// Puts `item` at the front of a history list, keeping at most `max` items.
    pub fn push_history(&mut self, key: &str, name: &str, item: &str, max: usize) {
        if item.is_empty() {
            return;
        }
        let mut list = self.history(key, name);
        list.retain(|i| i != item);
        list.insert(0, item.to_owned());
        list.truncate(max);
        let old = (0..).take_while(|i| self.values.contains_key(&format!("{key}\\{name}{i}")));
        for i in old.collect::<Vec<_>>() {
            self.values.remove(&format!("{key}\\{name}{i}"));
        }
        for (i, v) in list.into_iter().enumerate() {
            self.values.insert(format!("{key}\\{name}{i}"), v);
        }
    }
}

impl Settings {
    /// The raw stored value, if any.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    /// All stored keys, sorted.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.values.keys().map(String::as_str)
    }

    /// Number of stored keys below `prefix\` (Saved Data page counts).
    pub fn count_prefix(&self, prefix: &str) -> usize {
        let p = format!("{prefix}\\");
        self.values.keys().filter(|k| k.starts_with(&p)).count()
    }
}

/// Per-working-tree key component TortoiseGit uses (`C:\src\repo` -> `C_\src\repo`).
pub fn workdir_key(workdir: &Path) -> String {
    workdir.to_string_lossy().replace(':', "_")
}

fn escape(v: &str) -> String {
    v.replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn unescape(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some(o) => out.push(o),
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_values_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.ini");
        let mut s = Settings::load(&path).unwrap();
        assert_eq!(s.dword(keys::NUMBER_OF_LOGS, 1), 1);
        assert!(s.bool(keys::SELECT_FILES_FOR_COMMIT, true));
        s.set(keys::NUMBER_OF_LOGS, 100);
        s.set_bool(keys::SELECT_FILES_FOR_COMMIT, false);
        s.set(keys::OVERLAY_EXCLUDE_LIST, "C:\\a*\nC:\\b");
        s.save().unwrap();
        let s = Settings::load(&path).unwrap();
        assert_eq!(s.dword(keys::NUMBER_OF_LOGS, 1), 100);
        assert!(!s.bool(keys::SELECT_FILES_FOR_COMMIT, true));
        assert_eq!(s.string(keys::OVERLAY_EXCLUDE_LIST, ""), "C:\\a*\nC:\\b");
    }

    #[test]
    fn history_lists() {
        let mut s = Settings::default();
        for m in ["one", "two", "one", "three"] {
            s.push_history(r"History\commit", "logmsgs", m, 2);
        }
        assert_eq!(s.history(r"History\commit", "logmsgs"), ["three", "one"]);
        s.remove_prefix("History");
        assert!(s.history(r"History\commit", "logmsgs").is_empty());
        assert_eq!(workdir_key(Path::new(r"C:\src\r")), r"C_\src\r");
    }
}
