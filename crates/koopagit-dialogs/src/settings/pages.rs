//! Settings page definitions: tree order, labels, storage keys and defaults
//! (`CSettings::AddPropPages` and the individual property pages).

/// Where an option's value lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Store {
    /// Display only.
    None,
    /// TortoiseGit registry value (KoopaGit settings file): key and default.
    /// Booleans are `0`/`1`; combos without explicit values store the index.
    Reg(&'static str, &'static str),
    /// A git config key, read/written at the page's config scope.
    Git(&'static str),
    /// The built-in/external radio of a program setting (`#` rule).
    ProgMode(&'static str),
    /// The path of a program setting (`#` rule).
    ProgPath(&'static str),
    /// One half of `MergeBlockTrustBehavior`: `false` = block, `true` = trust.
    BlockTrust(bool),
    /// A field of the remote edited on the Remote page.
    Remote(RemoteField),
    /// A field of the hook edited on the Hook Scripts page.
    Hook(HookField),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookField {
    Enabled,
    Type,
    /// "Run for this repository": the path becomes the current work tree.
    ThisRepository,
    Path,
    Command,
    Wait,
    Hide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteField {
    Name,
    Url,
    PushUrl,
    PuttyKey,
    Tags,
    PushDefault,
    Prune,
}

/// Button actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    EditLocalConfig,
    EditProjectConfig,
    ViewEffectiveConfig,
    EditGlobalConfig,
    EditXdgConfig,
    ViewSystemConfig,
    RemoteSave,
    RemoteRemove,
    RemoteRename,
    ShowActionLog,
    ClearActionLog,
    /// "Restore Default": removes the page's stored registry values.
    RestoreDefaults,
    /// "..." / "Browse...": pick a folder for the registry value.
    BrowseFolder(&'static str),
    /// "..." / "Browse...": pick a file for the registry value.
    BrowseFile(&'static str),
    HookSave,
    HookRemove,
    HookCopy,
    HookBrowsePath,
    HookBrowseCommand,
    /// "Re-run First Start Wizard".
    FirstStartWizard,
}

/// Saved Data rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClearTarget {
    UrlHistory,
    LogMessages,
    LogCache,
    DialogSizes,
    Authentication,
    TempFiles,
    StoredDecisions,
}

impl ClearTarget {
    pub fn label(self) -> &'static str {
        match self {
            ClearTarget::UrlHistory => "URL history",
            ClearTarget::LogMessages => "Log messages (Input dialog)",
            ClearTarget::LogCache => "Log messages (Show log dialog)",
            ClearTarget::DialogSizes => "Dialog sizes and positions",
            ClearTarget::Authentication => "Authentication data",
            ClearTarget::TempFiles => "Temp files (including Gravatar images)",
            ClearTarget::StoredDecisions => "Stored decisions",
        }
    }
}

/// History keys removed by "URL history: Clear" (`SetSavedDataPage.cpp:200-210`).
pub const URL_HISTORY_KEYS: &[&str] = &[
    r"History\repoURLS",
    r"History\FormatPatchURLS",
    r"History\PullURLS",
    r"History\PushURLS",
    r"History\SubModuleRepoURLS",
    r"History\SyncURL",
];

/// "Do not show again" decisions removed by "Stored decisions: Clear".
pub const STORED_DECISIONS: &[&str] = &[
    "OldMsysgitVersionWarning",
    "OpenRebaseRemoteBranchEqualsHEAD",
    "OpenRebaseRemoteBranchUnchanged",
    "OpenRebaseRemoteBranchFastForwards",
    "DaemonNoSecurityWarning",
    "NothingToCommitShowUnversioned",
    "NoJumpNotFoundWarning",
    "HintHierarchicalConfig",
    "HintStagingMode",
    "TagOptNoTagsWarning",
    "NoStashIncludeUntrackedWarning",
    "CommitMergeHint",
    "AskSetTrackedBranch",
    "StashPopShowChanges",
    "StashPopShowConflictChanges",
    "CommitWarnOnUnresolved",
    "CommitAskBeforeCancel",
    "PushAllBranches",
    "CommitMessageContainsConflictHint",
    "MergeConflictsNeedsCommit",
    "CommitMessageTemplateNotEdited",
    r"TortoiseGitMerge\DeleteFileWhenEmpty",
    "approvedhooks",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Group,
    Note,
    Check,
    TriState,
    Text,
    Multiline,
    Number,
    /// `values: None` stores the selected index.
    Combo {
        labels: Vec<String>,
        values: Option<Vec<String>>,
    },
    Radio {
        labels: Vec<String>,
        values: Vec<String>,
    },
    Button(Action),
    Clear(ClearTarget),
    /// A colour stored as a Windows `COLORREF` DWORD (`0x00BBGGRR`), edited as `#RRGGBB`.
    Color,
}

/// One row of a property page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opt {
    pub label: String,
    pub kind: Kind,
    pub store: Store,
    /// Show the "inherit" checkbox (git config pages).
    pub inherit: bool,
    /// Only enabled while the repository is available.
    pub needs_repo: bool,
}

impl Opt {
    fn new(label: &str, kind: Kind, store: Store) -> Self {
        Opt {
            label: label.to_owned(),
            kind,
            store,
            inherit: false,
            needs_repo: false,
        }
    }

    fn inherit(mut self) -> Self {
        self.inherit = true;
        self
    }

    fn needs_repo(mut self) -> Self {
        self.needs_repo = true;
        self
    }
}

fn group(label: &str) -> Opt {
    Opt::new(label, Kind::Group, Store::None)
}
fn note(label: &str) -> Opt {
    Opt::new(label, Kind::Note, Store::None)
}
fn check(label: &str, key: &'static str, default: bool) -> Opt {
    Opt::new(
        label,
        Kind::Check,
        Store::Reg(key, if default { "1" } else { "0" }),
    )
}
fn text(label: &str, key: &'static str, default: &'static str) -> Opt {
    Opt::new(label, Kind::Text, Store::Reg(key, default))
}
fn number(label: &str, key: &'static str, default: &'static str) -> Opt {
    Opt::new(label, Kind::Number, Store::Reg(key, default))
}
fn multiline(label: &str, key: &'static str) -> Opt {
    Opt::new(label, Kind::Multiline, Store::Reg(key, ""))
}
fn combo_index(label: &str, key: &'static str, default: &'static str, items: &[&str]) -> Opt {
    Opt::new(
        label,
        Kind::Combo {
            labels: strings(items),
            values: None,
        },
        Store::Reg(key, default),
    )
}
fn git_text(label: &str, key: &'static str) -> Opt {
    Opt::new(label, Kind::Text, Store::Git(key)).inherit()
}
fn git_combo(label: &str, key: &'static str, labels: &[&str], values: &[&str]) -> Opt {
    Opt::new(
        label,
        Kind::Combo {
            labels: strings(labels),
            values: Some(strings(values)),
        },
        Store::Git(key),
    )
}
fn button(label: &str, action: Action) -> Opt {
    Opt::new(label, Kind::Button(action), Store::None)
}
fn prog(group_label: &str, key: &'static str, builtin: &str) -> Vec<Opt> {
    vec![
        group(group_label),
        Opt::new(
            "",
            Kind::Radio {
                labels: strings(&[builtin, "External"]),
                values: strings(&["0", "1"]),
            },
            Store::ProgMode(key),
        ),
        Opt::new("", Kind::Text, Store::ProgPath(key)),
    ]
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_owned()).collect()
}

/// Page-specific behaviour beyond a list of options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Special {
    None,
    /// Checklist of context-menu entries; `true` = the Shift-only mask.
    Checklist(bool),
    /// Remote list on the left of the options.
    Remote,
    /// Editable list of every stored value.
    Advanced,
    /// Not implemented in KoopaGit.
    Stub,
    /// Hook script list on the left of the options.
    Hooks,
}

#[derive(Debug, Clone)]
pub struct Page {
    pub id: &'static str,
    pub label: &'static str,
    pub depth: u8,
    pub special: Special,
    /// `Some(honor_project)` when the page has the config-source selector.
    pub config_source: Option<bool>,
    pub options: Vec<Opt>,
}

/// What the page builder needs to know about the environment.
#[derive(Debug, Clone, Default)]
pub struct Env {
    pub in_repo: bool,
    pub git_version: String,
    pub xdg_config_exists: bool,
    pub is_windows: bool,
}

/// The settings tree in `CSettings::AddPropPages` order.
pub fn pages(env: &Env) -> Vec<Page> {
    let mut v = Vec::new();
    let mut add = |id, label, depth, special, config_source, options| {
        v.push(Page {
            id,
            label,
            depth,
            special,
            config_source,
            options,
        })
    };
    add("main", "General", 0, Special::None, None, general(env));
    add(
        "look",
        "Context Menu",
        1,
        Special::Checklist(false),
        None,
        vec![
            check(
                "Hide Menus for unversioned paths",
                "HideMenusForUnversionedItems",
                false,
            ),
            multiline(
                "Do not show the context menu for the following paths:",
                "NoContextPaths",
            ),
            check("Enable drag context menu", "EnableDragContextMenu", true),
        ],
    );
    add(
        "extmenu",
        "Context Menu 2",
        1,
        Special::Checklist(true),
        None,
        vec![],
    );
    add("dialog", "Dialogs 1", 1, Special::None, None, dialogs1());
    add("dialog2", "Dialogs 2", 1, Special::None, None, dialogs2());
    add(
        "dialog3",
        "Dialogs 3",
        1,
        Special::None,
        Some(true),
        dialogs3(),
    );
    add("color1", "Colors 1", 1, Special::None, None, colors1());
    add("color2", "Colors 2", 1, Special::None, None, colors2());
    add("color3", "Colors 3", 1, Special::None, None, colors3());
    add(
        "alternativeeditor",
        "Alternative editor",
        1,
        Special::None,
        None,
        alternative_editor(env),
    );
    add("gitconfig", "Git", 0, Special::None, Some(false), git(env));
    if env.in_repo {
        add("gitremote", "Remote", 1, Special::Remote, None, remote(env));
    }
    add(
        "gitcredential",
        "Credential",
        1,
        Special::Stub,
        None,
        vec![],
    );
    add("hooks", "Hook Scripts", 0, Special::Hooks, None, hooks(env));
    add(
        "bugtraq",
        "Issue Tracker Integration",
        1,
        Special::Stub,
        None,
        vec![],
    );
    if env.in_repo {
        add(
            "bugtraqconfig",
            "Issue Tracker Config",
            1,
            Special::None,
            Some(true),
            bugtraq_config(),
        );
    }
    add(
        "overlay",
        "Icon Overlays",
        0,
        Special::None,
        None,
        overlays(),
    );
    add("overlays", "Icon Set", 1, Special::Stub, None, vec![]);
    add("proxy", "Network", 0, Special::None, None, network());
    add("smtp", "Email", 1, Special::None, None, email());
    add("diff", "Diff Viewer", 0, Special::None, None, diff_viewer());
    add("merge", "Merge Tool", 1, Special::None, None, merge_tool());
    add("save", "Saved Data", 0, Special::None, None, saved_data());
    add("blame", "KoopaGitBlame", 0, Special::None, None, blame());
    add("udiff", "KoopaGitUDiff", 0, Special::None, None, udiff());
    add(
        "advanced",
        "Advanced",
        0,
        Special::Advanced,
        None,
        vec![note(
            "WARNING: Only change these values if you know what you are doing!",
        )],
    );
    v
}

/// Default page (`Settings.cpp:197-215`): `/page:` if it names a page, else
/// Git inside a repository, else General.
pub fn default_page(pages: &[Page], requested: Option<&str>, in_repo: bool) -> usize {
    if let Some(i) = requested.and_then(|r| pages.iter().position(|p| p.id == r)) {
        return i;
    }
    let want = if in_repo { "gitconfig" } else { "main" };
    pages.iter().position(|p| p.id == want).unwrap_or(0)
}

fn general(env: &Env) -> Vec<Opt> {
    vec![
        group("KoopaGit"),
        Opt::new(
            "Language:",
            Kind::Combo {
                labels: strings(&["English (United States)"]),
                values: Some(strings(&["1033"])),
            },
            Store::Reg("LanguageID", "1033"),
        ),
        check(
            "Automatically check for newer versions every week",
            "VersionCheck",
            true,
        ),
        button("Re-run First Start Wizard", Action::FirstStartWizard),
        group(if env.is_windows {
            "Git for Windows"
        } else {
            "Git"
        }),
        text("Git.exe Path:", "MSysGit", ""),
        button("Browse...", Action::BrowseFolder("MSysGit")),
        text("Extra PATH:", "MSysGitExtra", ""),
        note(&format!("Version: {}", env.git_version)),
    ]
}

fn dialogs1() -> Vec<Opt> {
    vec![
        group("Log messages"),
        combo_index(
            "Default limitation of log messages:",
            r"LogDialog\NumberOfLogsScale",
            "0",
            &[
                "No limitation",
                "Last selected date",
                "Last N commit(s)",
                "Last N year(s)",
                "Last N month(s)",
                "Last N week(s)",
            ],
        ),
        number("N:", r"LogDialog\NumberOfLogs", "1"),
        text("Font for log messages:", "LogFontName", "Consolas"),
        number("Font size:", "LogFontSize", "9"),
        check(
            "Short date/time format in log messages",
            "LogDateFormat",
            true,
        ),
        check("Relative Times in log", "RelativeTimes", false),
        check("Show asterisk log prefix", "AsteriskLogPrefix", true),
        check(
            "Use system locale for date/time",
            "UseSystemLocaleForDates",
            true,
        ),
        check(
            "Can double-click in log list to compare with previous revision",
            "DiffByDoubleClickInLog",
            false,
        ),
        check("Abbreviate renamings", "AbbreviateRenamings", false),
        check("Symbolize ref names", "SymbolizeRefNames", false),
        check("Enable log cache", "EnableLogCache", true),
        check("Enable Gravatar", "EnableGravatar", false),
        text(
            "Gravatar URL:",
            "GravatarUrl",
            "https://gravatar.com/avatar/%HASH%?d=identicon",
        ),
        check(
            "Draw tag/branch labels on right side",
            "DrawTagsBranchesOnRightSide",
            false,
        ),
        check(
            "Display subject and body of commit messages",
            "FullCommitMessageOnLogLine",
            false,
        ),
        check(
            "Display branch revision number",
            "ShowBranchRevisionNumber",
            false,
        ),
        group("Describe"),
        check("Show describe in log", "ShowDescribe", false),
        combo_index(
            "Describe Strategy",
            "DescribeStrategy",
            "0",
            &["Annotated tags", "All tags", "All refs"],
        ),
        number("Abbreviated size", "DescribeAbbreviatedSize", "7"),
        check("Always show long format", "DescribeAlwaysLong", false),
        check(
            "Only follow first parent",
            "DescribeOnlyFollowFirstParent",
            false,
        ),
    ]
}

fn dialogs2() -> Vec<Opt> {
    vec![
        group("Misc"),
        combo_index(
            "Autoclose Git.exe dialog:",
            "AutoCloseGitProgress",
            "0",
            &[
                "Close manually",
                "Auto-close if no further options are available",
                "Auto-close if no errors",
            ],
        ),
        check(
            "Use recycle bin when reverting",
            "RevertWithRecycleBin",
            true,
        ),
        check(
            "Confirm to kill running git process",
            "ConfirmKillProcess",
            false,
        ),
        check(
            "Randomize Sync Dialog startup position",
            "SyncDialogRandomPos",
            false,
        ),
        check(
            "Hide unchanged refs in Ref Compare List",
            "RefCompareHideUnchanged",
            false,
        ),
        check(
            "Show git.exe execution timings and timestamp",
            "ShowGitexeTimings",
            true,
        ),
        check("Sort tag list in reversed order", "SortTagsReversed", false),
        check("Disable sounds", "NoSounds", false),
        check(
            "Include FETCH_HEAD in branch selection boxes",
            "BranchesIncludeFetchHead",
            true,
        ),
        check("Use .mailmap", "UseMailmap", true),
        group("Commit"),
        check(
            "Use auto-completion of file paths and keywords",
            "Autocompletion",
            true,
        ),
        number(
            "Timeout in seconds to stop the auto-completion parsing",
            "AutocompleteParseTimeout",
            "5",
        ),
        number(
            "Max. items to keep in the log message history",
            "MaxHistoryItems",
            "25",
        ),
        check("Select items automatically", "SelectFilesForCommit", true),
        check(
            "Do not auto-select \"missing\" files (deleted, but unstaged)",
            "AutoselectMissingFiles",
            false,
        ),
        check(
            "Strip lines starting with \"#\" in commit message",
            "StripCommentedLines",
            false,
        ),
        note("Further options for the commit dialog are on Dialogs 3 page."),
    ]
}

fn dialogs3() -> Vec<Opt> {
    vec![
        group("Commit"),
        note(
            "Select the language this project is using. This settings affects the spell checker used for commit messages.",
        ),
        git_combo(
            "Language:",
            "tgit.projectlanguage",
            &["", "(auto)", "(disable)", "English (United States)"],
            &["", "0", "-1", "1033"],
        ),
        Opt::new(
            "Minimum number of chars for a commit message: Limit:",
            Kind::Number,
            Store::Git("tgit.logminsize"),
        )
        .inherit(),
        Opt::new(
            "Char position where to show a border line in commit text boxes: Border:",
            Kind::Number,
            Store::Git("tgit.logwidthmarker"),
        )
        .inherit(),
        git_combo(
            "Warn on missing Signed-Off-By on commit",
            "tgit.warnnosignedoffby",
            &["", "true", "false"],
            &["", "true", "false"],
        ),
        group("Dialogs"),
        git_text("Overlay Icon:", "tgit.icon"),
    ]
}

fn git(env: &Env) -> Vec<Opt> {
    let mut v = vec![
        group("User Info"),
        git_text("Name:", "user.name"),
        git_text("Email:", "user.email"),
        git_text("Signing key ID:", "user.signingkey"),
        group("Auto CrLf convert"),
        git_combo(
            "AutoCrLf:",
            "core.autocrlf",
            &["", "false", "true", "input"],
            &["", "false", "true", "input"],
        ),
        git_combo(
            "SafeCrLf:",
            "core.safecrlf",
            &["", "false", "true", "warn"],
            &["", "false", "true", "warn"],
        ),
        Opt::new("QuotePath", Kind::TriState, Store::Git("core.quotepath")),
        Opt::new("Prune", Kind::TriState, Store::Git("fetch.prune")),
        group("Edit config files"),
        button("Edit local .git/config", Action::EditLocalConfig).needs_repo(),
        button("Edit .tgitconfig", Action::EditProjectConfig).needs_repo(),
        button("View effective config", Action::ViewEffectiveConfig),
        button("Edit global .gitconfig", Action::EditGlobalConfig),
    ];
    if env.xdg_config_exists {
        v.push(button("Edit global XDG git/config", Action::EditXdgConfig));
    }
    v.push(button(
        "View systemwide gitconfig",
        Action::ViewSystemConfig,
    ));
    v
}

fn remote(env: &Env) -> Vec<Opt> {
    let mut v = vec![
        Opt::new("Remote:", Kind::Text, Store::Remote(RemoteField::Name)),
        button("Rename", Action::RemoteRename),
        Opt::new("URL:", Kind::Text, Store::Remote(RemoteField::Url)),
        Opt::new("Push URL:", Kind::Text, Store::Remote(RemoteField::PushUrl)),
    ];
    if env.is_windows {
        v.push(Opt::new(
            "Putty Key:",
            Kind::Text,
            Store::Remote(RemoteField::PuttyKey),
        ));
    }
    v.extend([
        Opt::new(
            "Tags:",
            Kind::Combo {
                labels: strings(&["Reachable", "None", "All"]),
                values: None,
            },
            Store::Remote(RemoteField::Tags),
        ),
        Opt::new(
            "Push Default",
            Kind::Check,
            Store::Remote(RemoteField::PushDefault),
        ),
        Opt::new("Prune", Kind::TriState, Store::Remote(RemoteField::Prune)),
        button("Add New/Save", Action::RemoteSave),
        button("Remove", Action::RemoteRemove),
    ]);
    v
}

fn overlays() -> Vec<Opt> {
    vec![
        group("Icon Overlays"),
        check(
            "Unversioned files mark parent folder as modified",
            "UnversionedAsModified",
            false,
        ),
        check(
            "Show status of subrepositories in parent folders",
            "TGitCacheRecurseSubmodules",
            false,
        ),
        check(
            "Disable status cache for elevated processes",
            "ShowOverlaysOnlyNonElevated",
            false,
        ),
        group("Status cache"),
        Opt::new(
            "",
            Kind::Radio {
                labels: strings(&["Default", "Shell Extended", "Shell", "None"]),
                values: strings(&["1", "3", "2", "0"]),
            },
            Store::Reg("CacheType", "1"),
        ),
        multiline("Exclude paths:", "OverlayExcludeList"),
        multiline("Include paths:", "OverlayIncludeList"),
        check(
            "Show excluded folders as normal",
            "ShowExcludedAsNormal",
            true,
        ),
        group("Enabled Overlay Handlers"),
        check("Ignored", "ShowIgnoredOverlay", true),
        check("Unversioned", "ShowUnversionedOverlay", true),
    ]
}

fn diff_viewer() -> Vec<Opt> {
    let mut v = prog(
        "Configure the program used for comparing different revisions of files",
        "Diff",
        "KoopaGitMerge",
    );
    v.push(note(
        "External diff tools receive %base and %mine (plus %bname, %yname) placeholders.",
    ));
    v.extend(prog(
        "Configure viewer program for GNU diff files (patch files)",
        "DiffViewer",
        "KoopaGitUDiff",
    ));
    v
}

fn merge_tool() -> Vec<Opt> {
    let mut v = prog(
        "Configure the program used for resolving conflicted files",
        "Merge",
        "KoopaGitMerge",
    );
    v.push(Opt::new(
        "Block KoopaGit while executing the external merge tool",
        Kind::Check,
        Store::BlockTrust(false),
    ));
    v.push(Opt::new(
        "Trust the exitcode of the external merge tool for auto resolving",
        Kind::Check,
        Store::BlockTrust(true),
    ));
    v
}

fn saved_data() -> Vec<Opt> {
    let mut v: Vec<Opt> = [
        ClearTarget::UrlHistory,
        ClearTarget::LogMessages,
        ClearTarget::LogCache,
        ClearTarget::DialogSizes,
        ClearTarget::Authentication,
        ClearTarget::TempFiles,
        ClearTarget::StoredDecisions,
    ]
    .into_iter()
    .map(|t| Opt::new(t.label(), Kind::Clear(t), Store::None))
    .collect();
    v.push(group("Action log"));
    v.push(number(
        "Max. lines in action log",
        "MaxLinesInLogfile",
        "4000",
    ));
    v.push(button("Show", Action::ShowActionLog));
    v.push(button("Clear", Action::ClearActionLog));
    v
}

fn color(label: &str, key: &'static str, default: &'static str) -> Opt {
    Opt::new(label, Kind::Color, Store::Reg(key, default))
}

/// Colors 1 (`IDD_SETTINGSCOLORS_1`, `Colors.cpp:25-53`). Defaults are `COLORREF`s.
fn colors1() -> Vec<Opt> {
    vec![
        group("Status and action colors"),
        color(
            "possible or real conflict/obstructed",
            r"Colors\Conflict",
            "255",
        ),
        color("added files", r"Colors\Added", "6553700"),
        color("missing/deleted/replaced", r"Colors\Deleted", "100"),
        color("merged", r"Colors\Merged", "25600"),
        color("modified/copied", r"Colors\Modified", "10498560"),
        color("renamed", r"Colors\Renamed", "16711680"),
        group("Revision graph"),
        color("Note node", r"Colors\NoteNode", "41120"),
        color("Unknown ref-types", r"Colors\OtherRef", "14737632"),
        check(
            "Use local branch color for current branch",
            r"TortoiseProc\Graph\RevGraphUseLocalForCur",
            false,
        ),
        check("Dark theme", "DarkTheme", false),
        button("Restore Default", Action::RestoreDefaults),
    ]
}

/// Colors 2 (`IDD_SETTINGSCOLORS_2`).
fn colors2() -> Vec<Opt> {
    vec![
        group("Log Graphic"),
        color("Current Branch", r"Colors\CurrentBranch", "200"),
        color("Local Branch", r"Colors\LocalBranch", "49920"),
        color("Remote Branch", r"Colors\RemoteBranch", "11197951"),
        color("Tag", r"Colors\Tag", "65535"),
        color("Filter match", r"Colors\FilterMatch", "200"),
        button("Restore Default", Action::RestoreDefaults),
    ]
}

/// Colors 3 (`IDD_SETTINGSCOLORS_3`, `SettingsColors3.cpp:89-92`).
fn colors3() -> Vec<Opt> {
    vec![
        group("Log Branch Line"),
        color("LINE1", r"Colors\BranchLine1", "0"),
        color("LINE2", r"Colors\BranchLine2", "255"),
        color("LINE3", r"Colors\BranchLine3", "65280"),
        color("LINE4", r"Colors\BranchLine4", "16711680"),
        color("LINE5", r"Colors\BranchLine5", "8421504"),
        color("LINE6", r"Colors\BranchLine6", "32896"),
        color("LINE7", r"Colors\BranchLine7", "8421376"),
        color("LINE8", r"Colors\BranchLine8", "8388736"),
        number("Line width", r"TortoiseProc\Graph\LogLineWidth", "2"),
        number("Node size", r"TortoiseProc\Graph\LogNodeSize", "10"),
        button("Restore Default", Action::RestoreDefaults),
    ]
}

/// Alternative editor (`IDD_SETTINGSPROGSALTERNATIVEEDITOR`).
fn alternative_editor(env: &Env) -> Vec<Opt> {
    vec![
        note("Configure alternative editor. Preferably one which can handle *nix line endings."),
        Opt::new(
            "",
            Kind::Radio {
                labels: strings(&[
                    if env.is_windows {
                        "Notepad"
                    } else {
                        "System default editor"
                    },
                    "Custom",
                ]),
                values: strings(&["0", "1"]),
            },
            Store::ProgMode("AlternativeEditor"),
        ),
        Opt::new("", Kind::Text, Store::ProgPath("AlternativeEditor")),
        button("...", Action::BrowseFile("AlternativeEditor")),
    ]
}

/// Hook Scripts (`IDD_SETTINGSHOOKS` + `IDD_SETTINGSHOOKCONFIG`).
fn hooks(env: &Env) -> Vec<Opt> {
    let types: Vec<&str> = koopagit_core::hooks::HookType::ALL
        .iter()
        .map(|t| t.label())
        .collect();
    let mut this_repo = Opt::new(
        "Run for this repository",
        Kind::Check,
        Store::Hook(HookField::ThisRepository),
    );
    this_repo.needs_repo = true;
    let _ = env;
    vec![
        Opt::new("Enable", Kind::Check, Store::Hook(HookField::Enabled)),
        Opt::new(
            "Hook Type:",
            Kind::Combo {
                labels: strings(&types),
                values: None,
            },
            Store::Hook(HookField::Type),
        ),
        this_repo,
        Opt::new(
            "Run when working tree path is under:",
            Kind::Text,
            Store::Hook(HookField::Path),
        ),
        button("...", Action::HookBrowsePath),
        Opt::new(
            "Command Line To Execute:",
            Kind::Text,
            Store::Hook(HookField::Command),
        ),
        button("...", Action::HookBrowseCommand),
        Opt::new(
            "Wait for the script to finish",
            Kind::Check,
            Store::Hook(HookField::Wait),
        ),
        Opt::new(
            "Hide the script while running",
            Kind::Check,
            Store::Hook(HookField::Hide),
        ),
        button("Add New/Save", Action::HookSave),
        button("Copy", Action::HookCopy),
        button("Remove", Action::HookRemove),
        note("Use * as path to run a hook script for every working tree."),
    ]
}

/// Issue Tracker Config (`IDD_SETTINGSBUGTRAQ_CONFIG`).
fn bugtraq_config() -> Vec<Opt> {
    let yes_no = |label: &str, key: &'static str| {
        git_combo(label, key, &["", "true", "false"], &["", "true", "false"])
    };
    vec![
        group("BugTraq"),
        git_text("bugtraq.url", "bugtraq.url"),
        yes_no("bugtraq.warningifnoissue", "bugtraq.warningifnoissue"),
        git_text("bugtraq.message", "bugtraq.message"),
        yes_no("bugtraq.append", "bugtraq.append"),
        git_text("bugtraq.label", "bugtraq.label"),
        yes_no("bugtraq.number", "bugtraq.number"),
        git_text("bugtraq.logregex", "bugtraq.logregex"),
        group("IBugTraqProvider"),
        git_text("Provider uuid win32:", "bugtraq.provideruuid"),
        git_text("Provider uuid x64:", "bugtraq.provideruuid64"),
        git_text("Provider parameters:", "bugtraq.providerparams"),
    ]
}

/// Keys of the proxy settings (`SetProxyPage.cpp:33-37`). The "enabled" flag
/// is koopagit's own: upstream derives it from `http.proxy` being set.
pub const PROXY_ENABLED: &str = r"Git\Servers\global\http-proxy-enabled";
pub const PROXY_HOST: &str = r"Git\Servers\global\http-proxy-host";
pub const PROXY_PORT: &str = r"Git\Servers\global\http-proxy-port";
pub const PROXY_USER: &str = r"Git\Servers\global\http-proxy-username";
pub const PROXY_PASSWORD: &str = r"Git\Servers\global\http-proxy-password";
pub const SSH_CLIENT: &str = "SSH";

/// Network (`IDD_SETTINGSPROXY`).
fn network() -> Vec<Opt> {
    vec![
        check("Enable Proxy Server", PROXY_ENABLED, false),
        group("Proxy Settings"),
        text("Server address:", PROXY_HOST, ""),
        number("Port:", PROXY_PORT, ""),
        text("Username:", PROXY_USER, ""),
        text("Password:", PROXY_PASSWORD, ""),
        group("SSH"),
        text("SSH client:", SSH_CLIENT, ""),
        button("Browse...", Action::BrowseFile(SSH_CLIENT)),
    ]
}

/// Email (`IDD_SETTINGSMTP`, `SettingSMTP.cpp:36-40`).
fn email() -> Vec<Opt> {
    vec![
        Opt::new(
            "Delivery:",
            Kind::Combo {
                labels: strings(&[
                    "SMTP, directly to destination server",
                    "MAPI",
                    "Use configured server",
                ]),
                values: Some(strings(&["0", "1", "2"])),
            },
            Store::Reg(r"TortoiseProc\SendMail\DeliveryType", "0"),
        ),
        text("SMTP Server:", r"TortoiseProc\SendMail\Address", ""),
        number("Port:", r"TortoiseProc\SendMail\Port", "25"),
        combo_index(
            "Encryption",
            r"TortoiseProc\SendMail\Encryption",
            "0",
            &["none", "STARTTLS", "SSL/TLS"],
        ),
        check(
            "SMTP Server requires authentication",
            r"TortoiseProc\SendMail\AuthenticationRequired",
            false,
        ),
        note(
            "Credentials: git send-email asks for the password, or uses sendemail.smtpUser and sendemail.smtpPass from the git config.",
        ),
    ]
}

/// TortoiseGitBlame (`IDD_SETTINGSTBLAME`, `SettingsTBlame.cpp:43-54`).
fn blame() -> Vec<Opt> {
    vec![
        group("Colors"),
        color("Recently modified lines", "BlameNewColor", "5308415"),
        color("Older lines", "BlameOldColor", "16777215"),
        button("Restore Default", Action::RestoreDefaults),
        group("Font"),
        text("Font:", "BlameFontName", "Consolas"),
        number("Font size:", "BlameFontSize", "10"),
        number("Tab size:", "BlameTabSize", "4"),
        group("Blame"),
        combo_index(
            "Detect moved or copied lines:",
            r"TortoiseGitBlame\Workspace\DetectMovedOrCopiedLines",
            "0",
            &[
                "Disabled",
                "Within file",
                "From modified files",
                "At file creation",
                "From existing files",
            ],
        ),
        note("Number of characters required for moved or copied line detection:"),
        number(
            "Within a file:",
            r"TortoiseGitBlame\Workspace\DetectMovedOrCopiedLinesNumCharactersWithinFile",
            "20",
        ),
        number(
            "Between files:",
            r"TortoiseGitBlame\Workspace\DetectMovedOrCopiedLinesNumCharactersFromFiles",
            "40",
        ),
        check(
            "Ignore whitespace",
            r"TortoiseGitBlame\Workspace\IgnoreWhitespace",
            false,
        ),
        check(
            "Only consider first parents on blame",
            r"TortoiseGitBlame\Workspace\OnlyFirstParent",
            false,
        ),
        group("Log"),
        check(
            "Show complete log",
            r"TortoiseGitBlame\Workspace\ShowCompleteLog",
            true,
        ),
        check(
            "Follow renames",
            r"TortoiseGitBlame\Workspace\FollowRenames",
            false,
        ),
    ]
}

/// TortoiseGitUDiff (`IDD_SETTINGSUDIFF`, `SettingsTUDiff.cpp:33-49`,
/// `UDiffColors.h`; upstream's system colours become black/white).
fn udiff() -> Vec<Opt> {
    vec![
        group("Colors"),
        color(
            "Diff command (foreground)",
            "UDiffForeCommandColor",
            "3548170",
        ),
        color(
            "Diff command (background)",
            "UDiffBackCommandColor",
            "16777215",
        ),
        color(
            "Diff position (foreground)",
            "UDiffForePositionColor",
            "255",
        ),
        color(
            "Diff position (background)",
            "UDiffBackPositionColor",
            "16777215",
        ),
        color("Diff header (foreground)", "UDiffForeHeaderColor", "128"),
        color(
            "Diff header (background)",
            "UDiffBackHeaderColor",
            "8454143",
        ),
        color(
            "Diff comment (foreground)",
            "UDiffForeCommentColor",
            "32768",
        ),
        color(
            "Diff comment (background)",
            "UDiffBackCommentColor",
            "16777215",
        ),
        color("Diff added lines (foreground)", "UDiffForeAddedColor", "0"),
        color(
            "Diff added lines (background)",
            "UDiffBackAddedColor",
            "13434828",
        ),
        color(
            "Diff removed lines (foreground)",
            "UDiffForeRemovedColor",
            "0",
        ),
        color(
            "Diff removed lines (background)",
            "UDiffBackRemovedColor",
            "14540287",
        ),
        button("Restore Default", Action::RestoreDefaults),
        group("Font"),
        text("Font:", "UDiffFontName", "Consolas"),
        number("Font size:", "UDiffFontSize", "10"),
        number("Tab size:", "UDiffTabSize", "4"),
        note(
            "Note: These settings also apply to the Patch Viewer dialog.\nTo select whether you would like to use the build-in or any alternative diff viewer program go to \"Diff Viewer\" preferences section in the leftward tree.",
        ),
    ]
}

/// `COLORREF` (`0x00BBGGRR`) to `#RRGGBB`.
pub fn colorref_to_hex(v: u32) -> String {
    format!(
        "#{:02X}{:02X}{:02X}",
        v & 0xFF,
        (v >> 8) & 0xFF,
        (v >> 16) & 0xFF
    )
}

/// `#RRGGBB` (or `RRGGBB`) to `COLORREF`.
pub fn hex_to_colorref(s: &str) -> Option<u32> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    let (r, g, b) = (v >> 16, (v >> 8) & 0xFF, v & 0xFF);
    Some(r | (g << 8) | (b << 16))
}

/// `TG\Diff` style value split by the `#` rule (`SettingsProgsDiff.h:58`):
/// `(external, path)`.
pub fn split_prog(value: &str) -> (bool, String) {
    let external = !value.is_empty() && !value.starts_with('#');
    (external, value.trim_start_matches('#').to_owned())
}

/// Inverse of [`split_prog`]: the built-in choice keeps the path behind `#`.
pub fn join_prog(external: bool, path: &str) -> String {
    match (external, path.is_empty()) {
        (_, true) => String::new(),
        (true, false) => path.to_owned(),
        (false, false) => format!("#{path}"),
    }
}

/// `MergeBlockTrustBehavior`: 0 none, 1 block, 2 block + trust.
pub fn block_trust(value: u32) -> (bool, bool) {
    (value >= 1, value >= 2)
}

pub fn block_trust_value(block: bool, trust: bool) -> u32 {
    match (block, trust) {
        (false, _) => 0,
        (true, false) => 1,
        (true, true) => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_order_and_conditions() {
        let env = Env {
            in_repo: false,
            ..Default::default()
        };
        let p = pages(&env);
        let ids: Vec<&str> = p.iter().map(|p| p.id).collect();
        assert_eq!(
            ids,
            [
                "main",
                "look",
                "extmenu",
                "dialog",
                "dialog2",
                "dialog3",
                "color1",
                "color2",
                "color3",
                "alternativeeditor",
                "gitconfig",
                "gitcredential",
                "hooks",
                "bugtraq",
                "overlay",
                "overlays",
                "proxy",
                "smtp",
                "diff",
                "merge",
                "save",
                "blame",
                "udiff",
                "advanced"
            ]
        );
        let in_repo = pages(&Env {
            in_repo: true,
            ..Default::default()
        });
        let remote = in_repo.iter().position(|p| p.id == "gitremote").unwrap();
        assert_eq!(in_repo[remote - 1].id, "gitconfig");
        assert!(in_repo.iter().any(|p| p.id == "bugtraqconfig"));
        assert_eq!(default_page(&p, None, false), 0);
        assert_eq!(p[default_page(&in_repo, None, true)].id, "gitconfig");
        assert_eq!(p[default_page(&p, Some("save"), true)].id, "save");
        assert_eq!(p[default_page(&p, Some("nope"), false)].id, "main");
    }

    #[test]
    fn colorref_round_trip_and_no_more_stubs() {
        assert_eq!(colorref_to_hex(255), "#FF0000");
        assert_eq!(colorref_to_hex(11197951), "#FFDDAA");
        assert_eq!(hex_to_colorref("#FFDDAA"), Some(11197951));
        assert_eq!(hex_to_colorref("00ff00"), Some(65280));
        assert_eq!(hex_to_colorref("nope"), None);
        let stubs: Vec<&str> = pages(&Env {
            in_repo: true,
            ..Default::default()
        })
        .iter()
        .filter(|p| p.special == Special::Stub)
        .map(|p| p.id)
        .collect();
        assert_eq!(stubs, ["gitcredential", "bugtraq", "overlays"]);
    }

    #[test]
    fn prog_and_block_trust_rules() {
        assert_eq!(split_prog(""), (false, String::new()));
        assert_eq!(
            split_prog("#/usr/bin/meld"),
            (false, "/usr/bin/meld".into())
        );
        assert_eq!(
            split_prog("meld %base %mine"),
            (true, "meld %base %mine".into())
        );
        assert_eq!(join_prog(false, "meld"), "#meld");
        assert_eq!(join_prog(true, "meld"), "meld");
        assert_eq!(join_prog(true, ""), "");
        assert_eq!(block_trust(2), (true, true));
        assert_eq!(block_trust_value(false, true), 0);
        assert_eq!(block_trust_value(true, false), 1);
    }
}
