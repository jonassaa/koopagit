//! Running the `git` executable, the way TortoiseGit does for mutating operations.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

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

    /// The command line [`Git::command`] runs for `args`, as progress dialogs echo it
    /// (`git <args>`, after [`compat_args`] adapted it to the installed git).
    pub fn command_line<S: AsRef<str>>(&self, args: &[S]) -> String {
        let args = args.iter().map(|a| OsString::from(a.as_ref())).collect();
        let args = compat_args(args, || version_of(&self.exe));
        let args: Vec<_> = args.iter().map(|a| a.to_string_lossy()).collect();
        format!("git {}", args.join(" "))
    }

    /// A [`Command`] ready to run `git <args>` in the working directory, for callers
    /// that want to stream output (progress dialogs).
    pub fn command<I, S>(&self, args: I) -> Command
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        let args = args.into_iter().map(Into::into).collect();
        let args = compat_args(args, || version_of(&self.exe));
        let mut cmd = Command::new(&self.exe);
        cmd.current_dir(&self.workdir)
            // Never let git block on an interactive prompt or open an editor/pager.
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_EDITOR", "true")
            .env("GIT_PAGER", "cat")
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .args(args);
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

/// First git whose `checkout` and `reset` accept `--end-of-options` before the revision.
/// Older ones (Ubuntu 22.04 ships 2.34, 24.04 ships 2.43) keep the marker as an argument
/// and fail with "only one reference expected, 2 given" or "option '--end-of-options'
/// must come before non-option arguments" (fixed upstream in 2.44, see its release notes).
const END_OF_OPTIONS_FIXED: (u32, u32) = (2, 44);

/// Subcommands that mis-parse `--end-of-options` before [`END_OF_OPTIONS_FIXED`]. Every other
/// command KoopaGit passes it to handles it since git 2.24.
const KEEPS_END_OF_OPTIONS: &[&str] = &["checkout", "reset"];

/// Adapts TortoiseGit's argument list to the installed git: drops `--end-of-options` from
/// `checkout` and `reset` when git is older than 2.44. The marker is kept when the next
/// argument starts with `-`, so such a revision still fails instead of becoming an option.
/// `version` is only asked when an affected command line is seen.
fn compat_args(
    mut args: Vec<OsString>,
    version: impl FnOnce() -> Option<(u32, u32)>,
) -> Vec<OsString> {
    let Some(pos) = args.iter().position(|a| a == "--end-of-options") else {
        return args;
    };
    let protects_dash = args
        .get(pos + 1)
        .is_some_and(|next| next.to_string_lossy().starts_with('-') && next != "--");
    let affected = subcommand(&args)
        .and_then(OsStr::to_str)
        .is_some_and(|c| KEEPS_END_OF_OPTIONS.contains(&c));
    if affected && !protects_dash && version().is_some_and(|v| v < END_OF_OPTIONS_FIXED) {
        args.remove(pos);
    }
    args
}

/// The git subcommand in `args`, skipping global options (`-c <key=value>`, `-C <dir>`).
fn subcommand(args: &[OsString]) -> Option<&OsStr> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "-c" || a == "-C" {
            it.next();
        } else if !a.to_string_lossy().starts_with('-') {
            return Some(a);
        }
    }
    None
}

type Version = (u32, u32);

/// `(major, minor)` of the git at `exe`, from `git --version`, asked once per executable.
fn version_of(exe: &Path) -> Option<Version> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Option<Version>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(v) = cache.lock().unwrap().get(exe) {
        return *v;
    }
    let v = Command::new(exe)
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| parse_version(&String::from_utf8_lossy(&o.stdout)));
    cache.lock().unwrap().insert(exe.to_owned(), v);
    v
}

/// Parses `git version 2.43.0` (also `2.45.1.windows.1`, `2.39.3 (Apple Git-146)`).
fn parse_version(s: &str) -> Option<(u32, u32)> {
    let mut parts = s.trim().strip_prefix("git version ")?.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapt(args: &str, version: (u32, u32)) -> String {
        let args = args.split(' ').map(OsString::from).collect();
        let out: Vec<_> = compat_args(args, || Some(version))
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        out.join(" ")
    }

    #[test]
    fn parses_git_versions() {
        assert_eq!(parse_version("git version 2.43.0\n"), Some((2, 43)));
        assert_eq!(parse_version("git version 2.45.1.windows.1"), Some((2, 45)));
        assert_eq!(
            parse_version("git version 2.39.3 (Apple Git-146)"),
            Some((2, 39))
        );
        assert_eq!(parse_version("not git"), None);
    }

    #[test]
    fn old_git_checkout_and_reset_lose_end_of_options() {
        let old = (2, 43);
        assert_eq!(
            adapt("reset --hard --end-of-options 192d48a0 --", old),
            "reset --hard 192d48a0 --"
        );
        assert_eq!(
            adapt("checkout --end-of-options master --", old),
            "checkout master --"
        );
        assert_eq!(
            adapt(
                "-c core.quotepath=false checkout -f --end-of-options HEAD -- a",
                old
            ),
            "-c core.quotepath=false checkout -f HEAD -- a"
        );
    }

    #[test]
    fn end_of_options_is_kept_where_git_understands_it() {
        let line = "reset --hard --end-of-options HEAD --";
        assert_eq!(adapt(line, (2, 44)), line);
        assert_eq!(adapt(line, (3, 0)), line);
        let log = "log -1 --format=%B --end-of-options HEAD --";
        assert_eq!(adapt(log, (2, 34)), log);
        // A revision that looks like an option must keep failing, not run as one.
        let dash = "checkout --end-of-options --orphan --";
        assert_eq!(adapt(dash, (2, 34)), dash);
    }

    #[test]
    fn version_is_only_asked_for_affected_commands() {
        let args = vec![OsString::from("status")];
        compat_args(args, || panic!("git --version should not run"));
        let args = ["log", "--end-of-options", "HEAD"]
            .map(OsString::from)
            .to_vec();
        compat_args(args, || panic!("git --version should not run"));
    }

    /// Runs the real `git`: both commands must work whatever version is installed.
    #[test]
    fn checkout_and_reset_run_on_the_installed_git() {
        let dir = tempfile::tempdir().unwrap();
        let git = Git::new(dir.path());
        git.run(["init", "-q", "-b", "main"]).unwrap();
        for (k, v) in [("user.name", "t"), ("user.email", "t@t")] {
            git.run(["config", k, v]).unwrap();
        }
        std::fs::write(dir.path().join("f"), "1").unwrap();
        git.run(["add", "f"]).unwrap();
        git.run(["commit", "-qm", "one"]).unwrap();
        git.run(["branch", "dev"]).unwrap();
        std::fs::write(dir.path().join("f"), "2").unwrap();
        git.run(["reset", "--hard", "--end-of-options", "HEAD", "--"])
            .unwrap();
        assert_eq!(std::fs::read_to_string(dir.path().join("f")).unwrap(), "1");
        git.run(["checkout", "--end-of-options", "dev", "--"])
            .unwrap();
        let head = git.run(["rev-parse", "--abbrev-ref", "HEAD"]).unwrap();
        assert_eq!(head.trim(), "dev");
        std::fs::write(dir.path().join("f"), "3").unwrap();
        git.run(["checkout", "-f", "--end-of-options", "HEAD", "--", "f"])
            .unwrap();
        assert_eq!(std::fs::read_to_string(dir.path().join("f")).unwrap(), "1");
    }
}
