//! Running the `git` executable, the way TortoiseGit does for mutating operations.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::{Error, Result};

/// Environment variable that overrides the git executable (TortoiseGit's "Git.exe Path" setting).
pub const GIT_EXE_ENV: &str = "KOOPAGIT_GIT";

/// Captured result of a git invocation.
#[derive(Debug, Clone)]
pub struct GitOutput {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl GitOutput {
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }
}

/// A git invocation rooted in a working directory.
#[derive(Debug, Clone)]
pub struct Git {
    exe: PathBuf,
    workdir: PathBuf,
}

impl Git {
    pub fn new(workdir: impl Into<PathBuf>) -> Self {
        let exe = std::env::var_os(GIT_EXE_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("git"));
        Git {
            exe,
            workdir: workdir.into(),
        }
    }

    pub fn workdir(&self) -> &Path {
        &self.workdir
    }

    /// A [`Command`] ready to run `git <args>` in the working directory, for callers
    /// that want to stream output (progress dialogs).
    pub fn command<I, S>(&self, args: I) -> Command
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        let mut cmd = Command::new(&self.exe);
        cmd.current_dir(&self.workdir)
            // Never let git block on an interactive prompt or open an editor/pager.
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_EDITOR", "true")
            .env("GIT_PAGER", "cat")
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .args(args.into_iter().map(Into::into));
        cmd
    }

    /// Runs `git <args>`, capturing output, without treating a non-zero exit as an error.
    pub fn output<I, S>(&self, args: I) -> Result<GitOutput>
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        let out = self.command(args).output()?;
        Ok(GitOutput {
            code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }

    /// Runs `git <args>` and returns stdout, failing on a non-zero exit.
    pub fn run<I, S>(&self, args: I) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
        let out = self.output(args.iter().cloned())?;
        if out.success() {
            Ok(out.stdout)
        } else {
            Err(Error::GitFailed {
                args: args
                    .iter()
                    .map(|a| a.to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join(" "),
                code: out.code,
                stderr: out.stderr,
            })
        }
    }
}
