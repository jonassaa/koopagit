//! The conventional `koopagit <command> [options] [path...]` syntax.
//!
//! A front end for people typing commands in a terminal. It builds the same
//! [`Invocation`] as TortoiseGitProc's `/command:<name> /key:value` grammar,
//! which stays the form the shell extensions use, so both behave identically.
//! Every option's long name is its upstream key in kebab-case (`--close-on-end`
//! is `/closeonend`); the upstream spelling is accepted as an alias.

use std::ffi::OsString;

use clap::{Arg, ArgAction, ArgMatches, Command};
use koopagit_core::launcher::Invocation;

/// Whether `args` use TortoiseGitProc's grammar (`/command:log`, `-command:log`)
/// rather than the subcommand syntax.
pub fn is_upstream_syntax<S: AsRef<str>>(args: &[S]) -> bool {
    match args.first().map(AsRef::as_ref) {
        Some(first) => {
            first.starts_with('/')
                || (first.starts_with('-')
                    && !first.starts_with("--")
                    && !matches!(first, "-h" | "-V"))
        }
        None => false,
    }
}

/// Parses the subcommand syntax. `Err` carries help, version or a usage error;
/// `clap::Error::exit` prints it and exits with the conventional status.
pub fn parse<I, S>(args: I) -> Result<Invocation, clap::Error>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString> + Clone,
{
    let matches = command().try_get_matches_from(
        std::iter::once(OsString::from(BIN)).chain(args.into_iter().map(Into::into)),
    )?;
    let (name, sub) = matches
        .subcommand()
        .expect("subcommand_required guarantees a subcommand");
    let spec = SPECS
        .iter()
        .find(|s| s.name == name)
        .expect("every subcommand comes from SPECS");
    Ok(invocation(spec, sub))
}

const BIN: &str = "koopagit";

/// A command's positional path argument.
struct PathArg {
    /// Placeholder shown in usage, e.g. `PATH` or `FILE`.
    name: &'static str,
    help: &'static str,
    multiple: bool,
    required: bool,
}

#[derive(Clone, Copy)]
enum Kind {
    /// A bare `/key`.
    Switch,
    /// `/key:<value>`, with the placeholder shown in usage.
    Value(&'static str),
    /// `/key:<n>`.
    Number(&'static str),
}

/// An upstream `/key` parameter.
struct Opt {
    key: &'static str,
    long: &'static str,
    kind: Kind,
    help: &'static str,
    required: bool,
}

struct Spec {
    name: &'static str,
    about: &'static str,
    paths: Option<PathArg>,
    opts: &'static [Opt],
    /// Internal commands (drag-and-drop handlers, first start) stay out of help.
    hidden: bool,
}

fn command() -> Command {
    Command::new(BIN)
        .about("KoopaGit: open TortoiseGit-style dialogs from the command line")
        .version(env!("CARGO_PKG_VERSION"))
        .subcommand_required(true)
        .arg_required_else_help(true)
        .after_help(
            "TortoiseGitProc's syntax is also accepted:\n  \
             koopagit /command:log /path:.\n\n\
             See 'koopagit help <command>' for a command's options.",
        )
        .subcommands(SPECS.iter().map(subcommand))
}

fn subcommand(spec: &Spec) -> Command {
    let mut cmd = Command::new(spec.name).about(spec.about).hide(spec.hidden);
    for opt in spec.opts {
        let mut arg = Arg::new(opt.key)
            .long(opt.long)
            .help(opt.help)
            .required(opt.required);
        if opt.long != opt.key {
            arg = arg.alias(opt.key);
        }
        arg = match opt.kind {
            Kind::Switch => arg.action(ArgAction::SetTrue),
            Kind::Value(name) => arg.value_name(name),
            Kind::Number(name) => arg.value_name(name).value_parser(clap::value_parser!(i64)),
        };
        cmd = cmd.arg(arg);
    }
    if let Some(p) = &spec.paths {
        cmd = cmd.arg(
            Arg::new(PATHS)
                .value_name(p.name)
                .help(p.help)
                .required(p.required)
                .action(if p.multiple {
                    ArgAction::Append
                } else {
                    ArgAction::Set
                }),
        );
    }
    cmd
}

/// Argument id of the positional paths; not a valid upstream key.
const PATHS: &str = " paths";

fn invocation(spec: &Spec, m: &ArgMatches) -> Invocation {
    let mut inv = Invocation::new(spec.name);
    for opt in spec.opts {
        inv = match opt.kind {
            Kind::Switch if m.get_flag(opt.key) => inv.with_switch(opt.key),
            Kind::Switch => inv,
            Kind::Value(_) => match m.get_one::<String>(opt.key) {
                Some(v) => inv.with(opt.key, v.clone()),
                None => inv,
            },
            Kind::Number(_) => match m.get_one::<i64>(opt.key) {
                Some(v) => inv.with(opt.key, v.to_string()),
                None => inv,
            },
        };
    }
    // Only query PATHS when it is defined: clap panics on unknown ids.
    match spec
        .paths
        .as_ref()
        .and_then(|_| m.get_many::<String>(PATHS))
    {
        Some(paths) => inv.with_paths(paths),
        None => inv,
    }
}

const fn switch(key: &'static str, long: &'static str, help: &'static str) -> Opt {
    Opt {
        key,
        long,
        kind: Kind::Switch,
        help,
        required: false,
    }
}

const fn value(
    key: &'static str,
    long: &'static str,
    name: &'static str,
    help: &'static str,
) -> Opt {
    Opt {
        key,
        long,
        kind: Kind::Value(name),
        help,
        required: false,
    }
}

const fn number(
    key: &'static str,
    long: &'static str,
    name: &'static str,
    help: &'static str,
) -> Opt {
    Opt {
        key,
        long,
        kind: Kind::Number(name),
        help,
        required: false,
    }
}

impl Opt {
    const fn required(mut self) -> Self {
        self.required = true;
        self
    }
}

const fn paths(name: &'static str, help: &'static str) -> Option<PathArg> {
    Some(PathArg {
        name,
        help,
        multiple: true,
        required: false,
    })
}

const fn path(name: &'static str, help: &'static str) -> Option<PathArg> {
    Some(PathArg {
        name,
        help,
        multiple: false,
        required: false,
    })
}

const fn required(arg: Option<PathArg>) -> Option<PathArg> {
    match arg {
        Some(mut a) => {
            a.required = true;
            Some(a)
        }
        None => None,
    }
}

const fn spec(
    name: &'static str,
    about: &'static str,
    paths: Option<PathArg>,
    opts: &'static [Opt],
) -> Spec {
    Spec {
        name,
        about,
        paths,
        opts,
        hidden: false,
    }
}

const fn hidden(mut s: Spec) -> Spec {
    s.hidden = true;
    s
}

/// The repository-locating path most commands take.
const REPO: Option<PathArg> = path(
    "PATH",
    "A path in the working tree [default: current directory]",
);

const CLOSE: Opt = number(
    "closeonend",
    "close-on-end",
    "N",
    "Close the progress dialog when done: 0 = manually, 1 = if no further options, 2 = if no errors",
);
const UNIFIED: Opt = switch("unified", "unified", "Show a unified diff");
const ALTERNATIVE: Opt = switch(
    "alternative",
    "alternative",
    "Use the alternative diff/merge tool",
);
const DROPTARGET: Opt = value("droptarget", "drop-target", "DIR", "Destination folder").required();

/// The implemented TortoiseGitProc commands, alphabetically.
const SPECS: &[Spec] = &[
    spec("about", "Show the About dialog", None, &[]),
    spec(
        "add",
        "Add files to version control",
        paths("PATH", "Files to add, or folders to choose files from"),
        &[CLOSE],
    ),
    spec(
        "bisect",
        "Start or control a bisect",
        REPO,
        &[
            switch("start", "start", "Start a bisect"),
            switch("good", "good", "Mark a revision as good"),
            switch("bad", "bad", "Mark a revision as bad"),
            switch("skip", "skip", "Skip a revision"),
            switch("reset", "reset", "End the bisect"),
            value("ref", "ref", "REV", "Revision to mark [default: HEAD]"),
            CLOSE,
        ],
    ),
    spec(
        "blame",
        "Open KoopaGitBlame for a file",
        required(path("FILE", "File to blame")),
        &[
            value(
                "endrev",
                "end-rev",
                "REV",
                "Blame the file as of this revision",
            ),
            number("line", "line", "N", "Scroll to this line"),
        ],
    ),
    spec(
        "branch",
        "Open the Create Branch dialog",
        REPO,
        &[
            value("name", "name", "NAME", "Pre-fill the branch name"),
            value("rev", "rev", "REV", "Base branch, tag or commit"),
            CLOSE,
        ],
    ),
    spec(
        "cat",
        "Save a file as of a revision, without UI",
        required(path("FILE", "File to save")),
        &[
            value(
                "savepath",
                "save-path",
                "FILE",
                "Where to write the contents",
            )
            .required(),
            value(
                "revision",
                "revision",
                "REV",
                "Revision to read [default: HEAD]",
            ),
        ],
    ),
    spec(
        "cleanup",
        "Open the Clean dialog and remove untracked files",
        path("PATH", "Folder to clean [default: the whole working tree]"),
        &[
            switch("noui", "no-ui", "Skip the dialog and use the options below"),
            number(
                "cleantype",
                "clean-type",
                "N",
                "0 = untracked and ignored, 1 = ignored only, 2 = untracked only",
            ),
            number("dirs", "dirs", "N", "1 = also remove directories"),
            number(
                "nestedrepos",
                "nested-repos",
                "N",
                "1 = also remove nested repositories",
            ),
            number("submodules", "submodules", "N", "1 = also clean submodules"),
            number(
                "recycle",
                "recycle",
                "N",
                "1 = move to the trash, otherwise delete permanently",
            ),
            CLOSE,
        ],
    ),
    spec(
        "clone",
        "Open the Clone dialog",
        path("DIR", "Folder to clone into [default: current directory]"),
        &[
            value("url", "url", "URL", "URL to clone from"),
            switch(
                "exactpath",
                "exact-path",
                "Clone into DIR itself, not a subfolder named after the repository",
            ),
            switch(
                "hasurlhandler",
                "has-url-handler",
                "Started from a URL handler",
            ),
        ],
    ),
    spec(
        "commit",
        "Open the Commit dialog",
        paths(
            "PATH",
            "Files or folders to commit [default: the whole working tree]",
        ),
        &[
            value("logmsg", "log-msg", "MSG", "Pre-fill the commit message"),
            value(
                "logmsgfile",
                "log-msg-file",
                "FILE",
                "Pre-fill the commit message from a file",
            ),
            value("bugid", "bug-id", "ID", "Pre-fill the bug ID"),
        ],
    ),
    spec(
        "commitisonrefs",
        "Show which references contain a commit",
        REPO,
        &[value(
            "rev",
            "rev",
            "REV",
            "Commit to look up [default: HEAD]",
        )],
    ),
    spec(
        "conflicteditor",
        "Open the conflict editor for a conflicted file",
        required(path("FILE", "Conflicted file")),
        &[ALTERNATIVE],
    ),
    spec("daemon", "Run git daemon for the repository", REPO, &[]),
    spec(
        "diff",
        "Diff a file against BASE, a revision or another file",
        required(path(
            "PATH",
            "File to diff; a folder opens Check for Modifications",
        )),
        &[
            value("path2", "path2", "FILE", "Diff against this file instead"),
            value(
                "startrev",
                "start-rev",
                "REV",
                "First revision [default: BASE]",
            ),
            value(
                "endrev",
                "end-rev",
                "REV",
                "Second revision [default: working tree]",
            ),
            UNIFIED,
            ALTERNATIVE,
            number("line", "line", "N", "Scroll to this line"),
            switch("submodule", "submodule", "Treat a folder as a submodule"),
        ],
    ),
    hidden(spec(
        "dropcopyadd",
        "Copy items into a folder and add them (drag and drop)",
        required(paths("PATH", "Items to copy")),
        &[DROPTARGET, CLOSE],
    )),
    hidden(spec(
        "dropmove",
        "Move items into a folder with git mv (drag and drop)",
        required(paths("PATH", "Items to move")),
        &[
            DROPTARGET,
            switch(
                "rename",
                "rename",
                "Ask for a new name when moving one item",
            ),
        ],
    )),
    hidden(spec(
        "dropnewworktree",
        "Create a worktree in a folder (drag and drop)",
        REPO,
        &[DROPTARGET, CLOSE],
    )),
    spec(
        "export",
        "Export a revision to a zip file",
        REPO,
        &[
            value("rev", "rev", "REV", "Revision to export [default: HEAD]"),
            CLOSE,
        ],
    ),
    spec(
        "fetch",
        "Open the Fetch dialog",
        REPO,
        &[value("remote", "remote", "REMOTE", "Remote to preselect")],
    ),
    hidden(spec(
        "firststart",
        "Show the First Start wizard",
        path("PATH", "A path in the working tree"),
        &[],
    )),
    spec(
        "formatpatch",
        "Open the Format Patch dialog",
        REPO,
        &[
            value(
                "startrev",
                "start-rev",
                "REV",
                "Create patches since this revision",
            ),
            value("endrev", "end-rev", "REV", "End of the revision range"),
            value("range", "range", "FROM..TO", "Revision range to export"),
            CLOSE,
        ],
    ),
    spec(
        "ignore",
        "Add items to .gitignore",
        required(paths("PATH", "Files or folders to ignore")),
        &[
            switch("onlymask", "only-mask", "Ignore by extension (*.ext)"),
            switch("delete", "delete", "Also remove the items from the index"),
        ],
    ),
    spec(
        "importpatch",
        "Open the Apply Patches dialog",
        paths("PATCH", "Patch files to apply"),
        &[],
    ),
    hidden(spec(
        "inaccessible",
        "Explain why a repository cannot be opened",
        required(path("DIR", "The repository folder")),
        &[],
    )),
    spec(
        "lfslock",
        "Lock files with git lfs lock",
        required(paths("FILE", "Files to lock")),
        &[CLOSE],
    ),
    spec("lfslocks", "Show the LFS locks", REPO, &[CLOSE]),
    spec(
        "lfsunlock",
        "Unlock files with git lfs unlock",
        required(paths("FILE", "Files to unlock")),
        &[CLOSE],
    ),
    spec(
        "log",
        "Open the Log dialog",
        paths(
            "PATH",
            "File or folder to show the log of [default: the whole repository]",
        ),
        &[
            value("rev", "rev", "REV", "Select this revision"),
            value("range", "range", "RANGE", "Revision range to show"),
            value(
                "startrev",
                "start-rev",
                "REV",
                "Start of the range START..END",
            ),
            value(
                "endrev",
                "end-rev",
                "REV",
                "Show the log of this revision, or the end of the range",
            ),
            value(
                "limit",
                "limit",
                "LIMIT",
                "Number of entries, e.g. '100' or '2 Year'; 0 = no limit",
            ),
            value("findstring", "find-string", "TEXT", "Pre-fill the filter"),
            value(
                "outfile",
                "out-file",
                "FILE",
                "On OK, write the selected commit's hash to this file",
            ),
        ],
    ),
    spec(
        "merge",
        "Open the Merge dialog",
        REPO,
        &[
            switch("abort", "abort", "Open the Abort Merge dialog instead"),
            value("rev", "rev", "REV", "Branch, tag or commit to merge"),
            CLOSE,
        ],
    ),
    spec(
        "newworktree",
        "Open the New Worktree dialog",
        REPO,
        &[CLOSE],
    ),
    spec(
        "pgpfp",
        "Show the release-signing key fingerprint",
        None,
        &[],
    ),
    spec(
        "prevdiff",
        "Diff a file against its previous revision",
        required(path("PATH", "File (or folder) to diff")),
        &[UNIFIED, ALTERNATIVE],
    ),
    spec(
        "pull",
        "Open the Pull dialog",
        REPO,
        &[value("remote", "remote", "REMOTE", "Remote to preselect")],
    ),
    spec(
        "push",
        "Open the Push dialog",
        REPO,
        &[value("branch", "branch", "BRANCH", "Local branch to push")],
    ),
    spec(
        "rebase",
        "Open the Rebase dialog",
        REPO,
        &[
            value("upstream", "upstream", "REV", "Upstream to rebase onto"),
            switch("force", "force", "Turn on Force"),
            switch("cherrypick", "cherry-pick", "Cherry-pick commits instead"),
            value(
                "revs",
                "revs",
                "REVS",
                "Comma-separated commits to cherry-pick",
            ),
        ],
    ),
    spec("refbrowse", "Open the Browse References dialog", REPO, &[]),
    spec(
        "reflog",
        "Open the RefLog dialog",
        REPO,
        &[value(
            "ref",
            "ref",
            "REF",
            "Reference to show [default: HEAD]",
        )],
    ),
    spec(
        "remove",
        "Remove files from version control",
        required(paths("PATH", "Files or folders to remove")),
        &[switch(
            "keep",
            "keep",
            "Keep the local files (git rm --cached)",
        )],
    ),
    spec(
        "rename",
        "Rename or move a file",
        required(path("PATH", "File or folder to rename")),
        &[],
    ),
    spec(
        "repobrowser",
        "Open the Repository Browser",
        REPO,
        &[value(
            "rev",
            "rev",
            "REV",
            "Revision to browse [default: HEAD]",
        )],
    ),
    spec(
        "repocreate",
        "Create a repository (Git Init)",
        path("DIR", "Folder to initialize [default: current directory]"),
        &[],
    ),
    spec(
        "repostatus",
        "Check for modifications",
        paths(
            "PATH",
            "Files or folders to show [default: the whole working tree]",
        ),
        &[],
    ),
    spec(
        "requestpull",
        "Open the Request Pull dialog",
        REPO,
        &[
            value("url", "url", "URL", "URL of the repository to pull from"),
            value("endrev", "end-rev", "REV", "End revision [default: HEAD]"),
        ],
    ),
    spec(
        "resolve",
        "Mark conflicted files as resolved",
        paths("PATH", "Files or folders [default: all conflicted files]"),
        &[
            switch("noquestion", "no-question", "Do not ask for confirmation"),
            switch("silent", "silent", "Run without a progress dialog"),
            CLOSE,
        ],
    ),
    spec(
        "revert",
        "Revert local modifications",
        paths("PATH", "Files to revert, or folders to choose files from"),
        &[CLOSE],
    ),
    spec("revisiongraph", "Show the revision graph", REPO, &[]),
    spec("rtfm", "Open the TortoiseGit manual", None, &[]),
    spec(
        "sendmail",
        "Open the Send Patch dialog",
        paths("PATCH", "Patch files to send"),
        &[CLOSE],
    ),
    spec(
        "settings",
        "Open the Settings dialog",
        path(
            "PATH",
            "A path in a working tree, for its repository settings",
        ),
        &[value(
            "page",
            "page",
            "PAGE",
            "Page to open, e.g. gitconfig",
        )],
    ),
    spec(
        "showcompare",
        "Compare two revisions",
        path(
            "PATH",
            "File or folder to compare [default: the whole working tree]",
        ),
        &[
            value(
                "revision1",
                "revision1",
                "REV",
                "First revision [default: HEAD]",
            ),
            value(
                "revision2",
                "revision2",
                "REV",
                "Second revision [default: working tree]",
            ),
            UNIFIED,
            ALTERNATIVE,
        ],
    ),
    spec(
        "stashapply",
        "Apply a stash",
        REPO,
        &[value(
            "ref",
            "ref",
            "STASH",
            "Stash to apply [default: the latest]",
        )],
    ),
    spec("stashpop", "Apply the latest stash and drop it", REPO, &[]),
    spec(
        "stashsave",
        "Open the Stash Save dialog",
        REPO,
        &[
            value("msg", "msg", "MSG", "Pre-fill the stash message"),
            CLOSE,
        ],
    ),
    spec(
        "subadd",
        "Open the Submodule Add dialog",
        path("PATH", "Folder to add the submodule in"),
        &[CLOSE],
    ),
    spec(
        "subsync",
        "Sync submodule URLs (git submodule sync)",
        paths("PATH", "Submodules [default: all]"),
        &[CLOSE],
    ),
    spec(
        "subupdate",
        "Update submodules (git submodule update)",
        paths("PATH", "Submodules [default: all]"),
        &[CLOSE],
    ),
    spec("svndcommit", "Run git svn dcommit", REPO, &[CLOSE]),
    spec("svnfetch", "Run git svn fetch", REPO, &[CLOSE]),
    spec("svnignore", "Import svn:ignore", REPO, &[CLOSE]),
    spec("svnrebase", "Run git svn rebase", REPO, &[CLOSE]),
    spec(
        "switch",
        "Open the Switch/Checkout dialog",
        REPO,
        &[
            value("rev", "rev", "REV", "Branch, tag or commit to preselect"),
            CLOSE,
        ],
    ),
    spec("sync", "Open the Sync dialog", REPO, &[]),
    spec(
        "tag",
        "Open the Create Tag dialog",
        REPO,
        &[
            value("name", "name", "NAME", "Pre-fill the tag name"),
            value("rev", "rev", "REV", "Branch or commit to tag"),
        ],
    ),
    spec("updatecheck", "Check for updates", None, &[]),
    spec("worktreelist", "Open the Worktree List dialog", REPO, &[]),
];

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use clap::error::ErrorKind;

    use super::*;

    #[test]
    fn detects_upstream_syntax() {
        for args in [
            &["/command:log"][..],
            &["-command:log", "/path:."],
            &["/path:.", "/command:log"],
        ] {
            assert!(is_upstream_syntax(args), "{args:?}");
        }
        for args in [
            &[][..],
            &["log"],
            &["log", "/abs/path"],
            &["--help"],
            &["-h"],
            &["-V"],
            &["--version"],
        ] {
            assert!(!is_upstream_syntax(args), "{args:?}");
        }
    }

    #[test]
    fn builds_the_same_invocation_as_upstream_syntax() {
        let friendly = parse([
            "log",
            "--rev",
            "abc",
            "--find-string=fix",
            "--startrev",
            "v1",
            "a.txt",
            "dir",
        ])
        .unwrap();
        let upstream = Invocation::parse([
            "/command:log",
            "/path:a.txt*dir",
            "/rev:abc",
            "/findstring:fix",
            "/startrev:v1",
        ])
        .unwrap();
        assert_eq!(friendly, upstream);
        assert_eq!(
            friendly.paths().unwrap(),
            [PathBuf::from("a.txt"), PathBuf::from("dir")]
        );
    }

    #[test]
    fn switches_numbers_and_double_dash() {
        let inv = parse([
            "diff",
            "--unified",
            "--line",
            "12",
            "--close-on-end=2",
            "--",
            "-odd",
        ])
        .unwrap_err();
        // diff has no --close-on-end.
        assert_eq!(inv.kind(), ErrorKind::UnknownArgument);

        let inv = parse(["diff", "--unified", "--line", "12", "--", "-odd"]).unwrap();
        assert_eq!(inv.command, "diff");
        assert!(inv.has("unified"));
        assert!(!inv.has("alternative"));
        assert_eq!(inv.get_long("line"), Some(12));
        assert_eq!(inv.get("path"), Some("-odd"));

        let inv = parse(["add", "--close-on-end", "2"]).unwrap();
        assert_eq!(inv.get_long("closeonend"), Some(2));
        assert!(!inv.has("path"));
    }

    #[test]
    fn usage_errors_help_and_version() {
        let kind = |args: &[&str]| parse(args.iter().copied()).unwrap_err().kind();
        assert_eq!(
            kind(&[]),
            ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        );
        assert_eq!(kind(&["--help"]), ErrorKind::DisplayHelp);
        assert_eq!(kind(&["help", "log"]), ErrorKind::DisplayHelp);
        assert_eq!(kind(&["log", "-h"]), ErrorKind::DisplayHelp);
        assert_eq!(kind(&["-V"]), ErrorKind::DisplayVersion);
        assert_eq!(kind(&["lgo"]), ErrorKind::InvalidSubcommand);
        assert_eq!(kind(&["blame"]), ErrorKind::MissingRequiredArgument);
        assert_eq!(kind(&["cat", "f"]), ErrorKind::MissingRequiredArgument);
        assert_eq!(
            kind(&["blame", "f", "--line", "x"]),
            ErrorKind::ValueValidation
        );
        assert_eq!(kind(&["sync", "a", "b"]), ErrorKind::UnknownArgument);
    }

    #[test]
    fn table_matches_the_launcher() {
        command().debug_assert();
        assert!(SPECS.windows(2).all(|w| w[0].name < w[1].name));
        for spec in SPECS {
            assert!(
                crate::COMMANDS.iter().any(|(n, _)| *n == spec.name),
                "{} is not a launcher command",
                spec.name
            );
            for opt in spec.opts {
                assert_eq!(opt.long.replace('-', ""), opt.key, "{}", spec.name);
            }
        }
        // `help` is the CLI's own; `autotexttest` is not implemented.
        for (name, _) in crate::COMMANDS {
            assert!(
                matches!(*name, "help" | "autotexttest") || SPECS.iter().any(|s| s.name == *name),
                "{name} has no subcommand"
            );
        }
    }
}
