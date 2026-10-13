//! The `TortoiseGitProc /command:<name> /path:<path> ...` command-line grammar.
//!
//! Shell extensions build these command lines; the launcher (`koopagit-proc`)
//! parses them. Keeping both sides here guarantees they agree.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::{Error, Result};

/// A parsed launcher invocation.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Invocation {
    /// The `/command:` value, lowercased.
    pub command: String,
    /// Every other `/key:value` (or bare `/key`, stored with an empty value).
    /// Keys are lowercased like TortoiseGit's `CCmdLineParser`.
    params: BTreeMap<String, String>,
}

impl Invocation {
    pub fn new(command: impl Into<String>) -> Self {
        Invocation {
            command: command.into().to_lowercase(),
            params: BTreeMap::new(),
        }
    }

    /// Builder: sets `/key:value`.
    pub fn with(mut self, key: &str, value: impl Into<String>) -> Self {
        self.params.insert(key.to_lowercase(), value.into());
        self
    }

    /// Builder: sets `/path:` from one or more paths, `*`-separated as TortoiseGit does.
    pub fn with_paths<P: Into<PathBuf>>(self, paths: impl IntoIterator<Item = P>) -> Self {
        let joined = paths
            .into_iter()
            .map(|p| p.into().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("*");
        self.with("path", joined)
    }

    /// Builder: sets a bare `/key` switch.
    pub fn with_switch(self, key: &str) -> Self {
        self.with(key, "")
    }

    /// Parses launcher arguments (excluding the program name) like TortoiseGit's
    /// `CCmdLineParser`: `/key:value`, `-key:value`, `/key value` and bare `/key`.
    /// Keys are case-insensitive; if a key is repeated the first occurrence wins.
    pub fn parse<I, S>(args: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let args: Vec<String> = args.into_iter().map(|a| a.as_ref().to_owned()).collect();
        let mut inv = Invocation::default();
        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            i += 1;
            let Some(body) = arg.strip_prefix('/').or_else(|| arg.strip_prefix('-')) else {
                return Err(Error::InvalidCommandLine(format!(
                    "unexpected argument '{arg}' (expected /key:value)"
                )));
            };
            let (key, value) = match body.split_once(':') {
                Some((k, v)) => (k.to_owned(), unquote(v).to_owned()),
                // `/key value`: the next argument is the value unless it is an option.
                None => match args.get(i) {
                    Some(next) if !next.starts_with('/') && !next.starts_with('-') => {
                        i += 1;
                        (body.to_owned(), next.clone())
                    }
                    _ => (body.to_owned(), String::new()),
                },
            };
            if key.is_empty() {
                return Err(Error::InvalidCommandLine(format!("empty key in '{arg}'")));
            }
            inv.params.entry(key.to_lowercase()).or_insert(value);
        }
        match inv.params.remove("command") {
            Some(c) if !c.is_empty() => inv.command = c.to_lowercase(),
            // TortoiseGitProc without /command shows the About dialog.
            _ => inv.command = "about".to_owned(),
        }
        Ok(inv)
    }

    /// Renders back to argument form (program name excluded).
    pub fn to_args(&self) -> Vec<String> {
        let mut args = vec![format!("/command:{}", self.command)];
        for (k, v) in &self.params {
            if v.is_empty() {
                args.push(format!("/{k}"));
            } else {
                args.push(format!("/{k}:{v}"));
            }
        }
        args
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.params.get(&key.to_lowercase()).map(String::as_str)
    }

    /// Whether `/key` is present (with or without a value).
    pub fn has(&self, key: &str) -> bool {
        self.params.contains_key(&key.to_lowercase())
    }

    /// Integer value of `/key:n`, like `CCmdLineParser::GetLongVal`.
    pub fn get_long(&self, key: &str) -> Option<i64> {
        self.get(key)?.trim().parse().ok()
    }

    /// The target paths: `/path:` split on `*`, plus the lines of `/pathfile:`.
    pub fn paths(&self) -> Result<Vec<PathBuf>> {
        if self.has("path") && self.has("pathfile") {
            return Err(Error::InvalidCommandLine(
                "The parameters '/path' and '/pathfile' are mutually exclusive.\nYou must only specify one of them.".to_owned(),
            ));
        }
        if self.get("path") == Some("") {
            return Err(Error::InvalidCommandLine("invalid /path".to_owned()));
        }
        let mut paths: Vec<PathBuf> = self
            .get("path")
            .into_iter()
            .flat_map(|p| p.split('*'))
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .collect();
        if let Some(file) = self.get("pathfile") {
            let contents = std::fs::read(file)?;
            let text = decode_pathfile(&contents);
            paths.extend(
                text.lines()
                    .map(str::trim_end)
                    .filter(|l| !l.is_empty())
                    .map(PathBuf::from),
            );
            if self.has("deletepathfile") {
                let _ = std::fs::remove_file(file);
            }
        }
        Ok(paths)
    }

    /// `/closeonend:n`, TortoiseGit's progress-dialog auto-close mode
    /// (overrides the `AutoCloseGitProgress` setting).
    pub fn close_on_end(&self) -> Option<CloseOnEnd> {
        self.has("closeonend")
            .then(|| CloseOnEnd::from_value(self.get_long("closeonend").unwrap_or(0)))
    }
}

/// Progress-dialog auto-close modes (`AUTOCLOSE_*`, `ProgressDlg.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CloseOnEnd {
    /// 0: close manually.
    #[default]
    Manual,
    /// 1: close on success if no post-command buttons are offered.
    IfNoOptions,
    /// 2: close on success.
    IfNoErrors,
}

impl CloseOnEnd {
    /// Values 3 and 4 are documented upstream but unimplemented; like any
    /// other value they mean "manual".
    pub fn from_value(v: i64) -> Self {
        match v {
            1 => CloseOnEnd::IfNoOptions,
            2 => CloseOnEnd::IfNoErrors,
            _ => CloseOnEnd::Manual,
        }
    }
}

fn unquote(v: &str) -> &str {
    v.strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(v)
}

/// TortoiseGit writes path files as UTF-16LE with BOM; accept that or UTF-8.
fn decode_pathfile(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let units: Vec<u16> = rest
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tortoisegitproc_style_arguments() {
        let inv = Invocation::parse([
            "/command:Log",
            r#"/path:"C:\repo\a.txt*C:\repo\b""#,
            "/closeonend:2",
            "-Amend",
        ])
        .unwrap();
        assert_eq!(inv.command, "log");
        assert_eq!(
            inv.paths().unwrap(),
            [PathBuf::from(r"C:\repo\a.txt"), PathBuf::from(r"C:\repo\b")]
        );
        assert_eq!(inv.close_on_end(), Some(CloseOnEnd::IfNoErrors));
        assert!(inv.has("amend"));
        assert_eq!(inv.get("amend"), Some(""));
    }

    #[test]
    fn space_separated_values_first_key_wins_and_exclusive_paths() {
        let inv = Invocation::parse([
            "/command",
            "commit",
            "/logmsg",
            "hi",
            "/LogMsg:other",
            "/closeonend:4",
        ])
        .unwrap();
        assert_eq!(inv.command, "commit");
        assert_eq!(inv.get("logmsg"), Some("hi"));
        assert_eq!(inv.close_on_end(), Some(CloseOnEnd::Manual));
        let both = Invocation::parse(["/command:add", "/path:a", "/pathfile:b"]).unwrap();
        assert!(both.paths().is_err());
    }

    #[test]
    fn missing_command_means_about() {
        assert_eq!(
            Invocation::parse(Vec::<String>::new()).unwrap().command,
            "about"
        );
        assert!(Invocation::parse(["log"]).is_err());
    }

    #[test]
    fn round_trips_and_reads_pathfile() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("paths.tmp");
        let mut utf16 = vec![0xFF, 0xFE];
        for u in "/x/one\r\n/x/two\r\n".encode_utf16() {
            utf16.extend(u.to_le_bytes());
        }
        std::fs::write(&file, utf16).unwrap();
        let inv = Invocation::new("commit")
            .with("pathfile", file.to_string_lossy())
            .with_switch("deletepathfile");
        let parsed = Invocation::parse(inv.to_args()).unwrap();
        assert_eq!(parsed, inv);
        assert_eq!(
            parsed.paths().unwrap(),
            [PathBuf::from("/x/one"), PathBuf::from("/x/two")]
        );
        assert!(!file.exists());
    }
}
