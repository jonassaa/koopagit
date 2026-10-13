//! TortoiseGit client-side hook scripts (`Utils/Hooks.cpp`): start-commit,
//! pre-commit, post-commit, pre-push, post-push, pre-rebase.
//!
//! Stored in the `hooks` setting as five lines per hook: type, working-tree
//! path (`!` prefix = disabled, `*` = any), command line, `true`/`false`
//! (wait), `show`/`hide`.

use std::path::{Path, PathBuf};
use std::process::Command;

pub const SETTINGS_KEY: &str = "hooks";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookType {
    StartCommit,
    PreCommit,
    PostCommit,
    PrePush,
    PostPush,
    PreRebase,
}

impl HookType {
    pub const ALL: [HookType; 6] = [
        HookType::StartCommit,
        HookType::PreCommit,
        HookType::PostCommit,
        HookType::PrePush,
        HookType::PostPush,
        HookType::PreRebase,
    ];

    /// Storage name (`GetHookTypeString`).
    pub fn key(self) -> &'static str {
        match self {
            HookType::StartCommit => "start_commit_hook",
            HookType::PreCommit => "pre_commit_hook",
            HookType::PostCommit => "post_commit_hook",
            HookType::PrePush => "pre_push_hook",
            HookType::PostPush => "post_push_hook",
            HookType::PreRebase => "pre_rebase_hook",
        }
    }

    /// Label in the Hook Scripts settings (`IDS_HOOKTYPE_*`).
    pub fn label(self) -> &'static str {
        match self {
            HookType::StartCommit => "Start Commit Hook",
            HookType::PreCommit => "Pre-Commit Hook",
            HookType::PostCommit => "Post-Commit Hook",
            HookType::PrePush => "Pre-Push Hook",
            HookType::PostPush => "Post-Push Hook",
            HookType::PreRebase => "Pre-rebase hook",
        }
    }

    pub fn from_key(s: &str) -> Option<Self> {
        HookType::ALL.into_iter().find(|t| t.key() == s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hook {
    pub kind: HookType,
    /// Working tree path the hook applies to (and below); `*` = everywhere.
    pub path: String,
    pub command: String,
    pub wait: bool,
    pub show: bool,
    pub enabled: bool,
}

/// Parses the `hooks` setting (`ParseHookString`); unknown types are skipped.
pub fn parse(s: &str) -> Vec<Hook> {
    let lines: Vec<&str> = s.split('\n').map(|l| l.trim_end_matches('\r')).collect();
    lines
        .chunks(5)
        .filter(|c| c.len() == 5)
        .filter_map(|c| {
            let kind = HookType::from_key(c[0])?;
            let (enabled, path) = match c[1].strip_prefix('!') {
                Some(p) if !p.is_empty() => (false, p),
                _ => (true, c[1]),
            };
            Some(Hook {
                kind,
                path: path.to_owned(),
                command: c[2].to_owned(),
                wait: c[3] == "true",
                show: c[4] == "show",
                enabled,
            })
        })
        .collect()
}

/// Inverse of [`parse`] (`CHooks::Save`).
pub fn serialize(hooks: &[Hook]) -> String {
    let mut s = String::new();
    for h in hooks {
        s.push_str(h.kind.key());
        s.push('\n');
        if !h.enabled {
            s.push('!');
        }
        s.push_str(&h.path);
        s.push('\n');
        s.push_str(&h.command);
        s.push('\n');
        s.push_str(if h.wait { "true" } else { "false" });
        s.push('\n');
        s.push_str(if h.show { "show" } else { "hide" });
        s.push('\n');
    }
    s
}

fn same_path(a: &Path, b: &Path) -> bool {
    if cfg!(any(windows, target_os = "macos")) {
        a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}

/// `CHooks::FindItem`: the enabled hook for the working tree or its closest
/// parent folder, else one registered for `*`.
pub fn find<'a>(hooks: &'a [Hook], kind: HookType, workdir: &Path) -> Option<&'a Hook> {
    let mut dir = Some(workdir);
    while let Some(d) = dir {
        if let Some(h) = hooks.iter().find(|h| {
            h.kind == kind && h.enabled && h.path != "*" && same_path(Path::new(&h.path), d)
        }) {
            return Some(h);
        }
        dir = d.parent();
    }
    hooks
        .iter()
        .find(|h| h.kind == kind && h.enabled && h.path == "*")
}

/// Outcome of running a hook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookRun {
    pub exit_code: i32,
    pub error: String,
    /// The (possibly rewritten) commit message for start/pre-commit hooks.
    pub message: Option<String>,
}

fn temp_file(tag: &str, contents: &str) -> std::io::Result<PathBuf> {
    let dir = std::env::temp_dir().join("koopagit");
    std::fs::create_dir_all(&dir)?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let p = dir.join(format!("hook-{tag}-{}-{nanos}.txt", std::process::id()));
    std::fs::write(&p, contents)?;
    Ok(p)
}

fn quote(arg: &Path) -> String {
    format!("\"{}\"", arg.to_string_lossy())
}

/// The parameters each hook type receives (`CHooks::StartCommit` etc.).
pub enum HookArgs<'a> {
    /// start/pre-commit: affected paths and the message (read back on exit 0).
    Commit {
        paths: &'a [String],
        message: &'a str,
    },
    /// post-commit: whether the commit was an amend.
    PostCommit { amend: bool },
    /// pre/post-push: an error text file.
    Push { error: &'a str },
}

/// Runs `hook` for `workdir` (`%root%` is replaced by the work-tree root) and
/// waits for it if configured. The command line goes through the platform
/// shell, like upstream's `CreateProcess` of a command line.
pub fn run(hook: &Hook, workdir: &Path, args: HookArgs<'_>) -> std::io::Result<HookRun> {
    let mut cmd = hook.command.replace("%root%", &workdir.to_string_lossy());
    let mut message_file = None;
    match args {
        HookArgs::Commit { paths, message } => {
            let pf = temp_file("paths", &paths.join("\n"))?;
            let mf = temp_file("msg", message)?;
            cmd.push(' ');
            cmd.push_str(&quote(&pf));
            cmd.push(' ');
            cmd.push_str(&quote(&mf));
            message_file = Some(mf);
        }
        HookArgs::PostCommit { .. } | HookArgs::Push { .. } => {}
    }
    if let HookArgs::Push { error } = args {
        let ef = temp_file("error", error)?;
        cmd.push(' ');
        cmd.push_str(&quote(&ef));
    }
    cmd.push(' ');
    cmd.push_str(&quote(workdir));
    if let HookArgs::PostCommit { amend } = args {
        cmd.push_str(if amend { " \"true\"" } else { " \"false\"" });
    }
    let mut c = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", &cmd]);
        c
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", &cmd]);
        c
    };
    c.current_dir(workdir);
    if !hook.wait {
        c.spawn()?;
        return Ok(HookRun {
            exit_code: 0,
            error: String::new(),
            message: None,
        });
    }
    let out = c.output()?;
    let code = out.status.code().unwrap_or(-1);
    let message = match (&message_file, code) {
        (Some(f), 0) => std::fs::read_to_string(f).ok(),
        _ => None,
    };
    if let Some(f) = message_file {
        let _ = std::fs::remove_file(f);
    }
    Ok(HookRun {
        exit_code: code,
        error: String::from_utf8_lossy(&out.stderr).into_owned(),
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_lookup() {
        let hooks = vec![
            Hook {
                kind: HookType::PreCommit,
                path: "/work".into(),
                command: "check.sh".into(),
                wait: true,
                show: false,
                enabled: true,
            },
            Hook {
                kind: HookType::PostPush,
                path: "*".into(),
                command: "notify".into(),
                wait: false,
                show: true,
                enabled: true,
            },
            Hook {
                kind: HookType::PreCommit,
                path: "/other".into(),
                command: "x".into(),
                wait: true,
                show: true,
                enabled: false,
            },
        ];
        let s = serialize(&hooks);
        assert!(s.contains("pre_commit_hook\n!/other\nx\ntrue\nshow\n"));
        assert_eq!(parse(&s), hooks);
        assert_eq!(
            find(&hooks, HookType::PreCommit, Path::new("/work/repo")).map(|h| h.command.as_str()),
            Some("check.sh")
        );
        assert!(find(&hooks, HookType::PreCommit, Path::new("/other")).is_none());
        assert_eq!(
            find(&hooks, HookType::PostPush, Path::new("/x")).map(|h| h.command.as_str()),
            Some("notify")
        );
    }

    #[cfg(unix)]
    #[test]
    fn commit_hook_can_rewrite_message() {
        let dir = tempfile::tempdir().unwrap();
        let hook = Hook {
            kind: HookType::PreCommit,
            path: "*".into(),
            // $2 is the message file.
            command: "f() { echo \"[checked] $(cat \"$2\")\" > \"$2\"; }; f".into(),
            wait: true,
            show: false,
            enabled: true,
        };
        let r = run(
            &hook,
            dir.path(),
            HookArgs::Commit {
                paths: &["a.txt".into()],
                message: "msg",
            },
        )
        .unwrap();
        assert_eq!(r.exit_code, 0, "{}", r.error);
        assert_eq!(r.message.as_deref().map(str::trim), Some("[checked] msg"));
        let failing = Hook {
            command: "exit 3; :".into(),
            ..hook
        };
        let r = run(&failing, dir.path(), HookArgs::PostCommit { amend: false }).unwrap();
        assert_eq!(r.exit_code, 3);
    }
}
