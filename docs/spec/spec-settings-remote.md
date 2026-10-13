# TortoiseGit spec: Settings and remote/branch operations

Extracted from upstream TortoiseGit source (src/TortoiseProc, src/Resources/TortoiseProcENG.rc). Citations are relative to the upstream repo root.

## 1. Settings dialog

Paths below are relative to `src/`. `RC` = `Resources/TortoiseProcENG.rc`. Registry keys are under
`HKCU\Software\TortoiseGit\` unless shown otherwise (abbreviated `TG\`). `TO\` = `HKCU\Software\TortoiseOverlays\`
(shared between all Tortoise clients).

### 1.0 Launch and window

- Launched by `TortoiseGitProc /command:settings [/page:<pagename>]` (`TortoiseProc/Commands/SettingsCommand.cpp:28-45`).
- Title string `IDS_PROC_SETTINGS_TITLE` = "Settings" (RC:3776). If cwd is a working tree / bare repo the title is
  decorated with the repo path (`CAppUtils::SetWindowTitle`), otherwise "Settings - TortoiseGit"
  (`TortoiseProc/Settings/Settings.cpp:180-189`).
- Tree-view property sheet (`CTreePropSheet`), tree width 220 px @96 DPI (`SettingsCommand.cpp:39-41`). Each page is a
  property page; OK/Cancel/Apply are standard property-sheet buttons.
- Default page selection (`Settings.cpp:197-215`): if `/page:` matches a page id below it is selected (selecting
  `gitremote` that way sets `m_bNoFetch`, suppressing the "fetch newly added remote?" prompt). Otherwise, if cwd is in a
  repo, the **Git** page is preselected; else **General**.
- Special case: `/page:gitcredential` when running as admin opens only the Credential page in a single-page sheet
  (`SettingsCommand.cpp:32-36`).
- After close, `HandleRestart()` (`Settings.cpp:150-169`): pages may flag `Restart_System` (broadcast
  `WM_SETTINGCHANGE` + message "restart system") or `Restart_Cache` (tell TGitCache to refresh all and end).

### 1.1 Page tree (exact order and nesting)

From `CSettings::AddPropPages` (`Settings.cpp:100-144`). Name = page CAPTION from RC; id = `/page:` value.

| # | Tree label (caption) | `/page:` id | Parent | Class / IDD | Condition |
|---|---|---|---|---|---|
| 1 | General | `main` | (root) | CSetMainPage / IDD_SETTINGSMAIN (RC:342) | |
| 2 | Context Menu | `look` | General | CSetLookAndFeelPage / IDD_SETTINGSLOOKANDFEEL (RC:701) | |
| 3 | Context Menu 2 | `extmenu` | General | CSetExtMenu / IDD_SETTINGSEXTMENU (RC:1759) | |
| 4 | Windows 11 Context Menu | `win11menu` | General | CSetWin11ContextMenu (RC:2292) | Windows 11+ only |
| 5 | Dialogs 1 | `dialog` | General | CSetDialogs (RC:718) | |
| 6 | Dialogs 2 | `dialog2` | General | CSetDialogs2 (RC:760) | |
| 7 | Dialogs 3 | `dialog3` | General | CSetDialogs3 (RC:2039) | |
| 8 | Colors 1 | `color1` | General | CSettingsColors (RC:1412) | |
| 9 | Colors 2 | `color2` | General | CSettingsColors2 (RC:835) | |
| 10 | Colors 3 | `color3` | General | CSettingsColors3 (RC:1441) | |
| 11 | Alternative editor | `alternativeeditor` | General | CSettingsProgsAlternativeEditor (RC:1784) | |
| 12 | Git | `gitconfig` | (root) | CSettingGitConfig / IDD_SETTINGIT_CONFIG (RC:1592) | |
| 13 | Remote | `gitremote` | Git | CSettingGitRemote / IDD_SETTINREMOTE (RC:1633) | only if cwd is a working tree or bare repo |
| 14 | Credential | `gitcredential` | Git | CSettingGitCredential (RC:1995) | |
| 15 | Hook Scripts | `hooks` | (root) | CSetHooks (RC:890) | |
| 16 | Issue Tracker Integration | `bugtraq` | Hook Scripts | CSetBugTraq (RC:993) | |
| 17 | Issue Tracker Config | `bugtraqconfig` | Hook Scripts | CSettingsBugtraqConfig (RC:1005) | only if in repo |
| 18 | Icon Overlays | `overlay` | (root) | CSetOverlayPage (RC:365) | |
| 19 | Icon Set | `overlays` | Icon Overlays | CSetOverlayIcons (RC:652) | |
| 20 | Overlay Handlers | `overlayshandlers` | Icon Overlays | CSetOverlayHandlers (RC:400) | |
| 21 | Network | `proxy` | (root) | CSetProxyPage (RC:420) | |
| 22 | Email | `smtp` | Network | CSettingSMTP (RC:441) | |
| 23 | Diff Viewer | `diff` | (root) | CSettingsProgsDiff (RC:664) | |
| 24 | Merge Tool | `merge` | Diff Viewer | CSettingsProgsMerge (RC:683) | |
| 25 | Saved Data | `save` | (root) | CSetSavedDataPage (RC:854) | |
| 26 | TortoiseGitBlame | `blame` | (root) | CSettingsTBlame (RC:949) | |
| 27 | TortoiseGitUDiff | `udiff` | (root) | CSettingsUDiff (RC:2117) | |
| 28 | Advanced | `advanced` | (root) | CSettingsAdvanced / IDD_SETTINGS_CONFIG (RC:1089) | |

Rendered tree:

```
General
  Context Menu
  Context Menu 2
  Windows 11 Context Menu      (Win11+)
  Dialogs 1
  Dialogs 2
  Dialogs 3
  Colors 1
  Colors 2
  Colors 3
  Alternative editor
Git
  Remote                       (in repo only)
  Credential
Hook Scripts
  Issue Tracker Integration
  Issue Tracker Config         (in repo only)
Icon Overlays
  Icon Set
  Overlay Handlers
Network
  Email
Diff Viewer
  Merge Tool
Saved Data
TortoiseGitBlame
TortoiseGitUDiff
Advanced
```

Pages whose caption is shown in a repo get " - <project path>" appended to the page title by
`InitGitSettings` (`Settings/GitSettings.h:64-68`) — applies to Git, Dialogs 3, Issue Tracker Config.

### 1.2 Storage model: registry vs git config

- **TortoiseGit registry** (per-user, HKCU): everything on General, Context Menu(s), Dialogs 1, Dialogs 2, Colors,
  Alternative editor, Icon Overlays, Icon Set, Overlay Handlers (under `TortoiseOverlays`), Network (proxy is stored
  in registry under `TG\Git\Servers\global\http-proxy-*` — see `SetProxyPage.cpp:33-37`), Diff Viewer, Merge Tool,
  Saved Data, Blame, UDiff, Advanced. For a cross-platform clone this maps to a per-user TortoiseGit config store.
- **git config** (via libgit2, files on disk): Git page (`user.*`, `core.*`, `fetch.prune`), Remote page
  (`remote.<name>.*`, `remote.pushdefault`, always written to **local** `.git/config`), Credential page
  (`credential.*`), Dialogs 3 (`tgit.*` project properties), Issue Tracker Config (`bugtraq.*`).

#### Config-source selector (shared by Git, Dialogs 3, Issue Tracker Config)

Implemented in `class CGitSettings` (`Settings/GitSettings.h:29-273`).

UI: groupbox "Config source" with radio buttons **Effective | Local << Project << Global << System** (Git page omits
Project: `Effective | Local << Global << System`, RC:1594-1601; Dialogs 3 RC:2041-2048). Plus a "Save to:" dropdown.

Enum (`GitSettings.h:36-43`): `EFFECTIVE=0, LOCAL=1, PROJECT=2, GLOBAL=3, SYSTEM=4`. Default `EFFECTIVE`.

Init (`GitSettings.h:49-95`):
- If cwd has a git admin dir or is bare: Local radio enabled; "Save to" gets "Local"; Project radio enabled only if the
  page honors project config (`honorProjectConfig` = true for Dialogs 3 and Issue Tracker Config, false for Git page;
  `SetDialogs3.cpp:97`, `SettingsBugtraqConfig.cpp:121`, `SettingGitConfig.cpp:108`); "Save to" gets "Project" if
  honoring and not bare.
- Else `m_bGlobal = true`, Local/Project radios disabled.
- "Save to" always gets "Global" last; first entry selected.
- Shows one-time info box `IDS_HIERARCHICALCONFIG` ("Git uses the concept of a hierarchical configuration...") with
  "do not show again" stored as `TG\HintHierarchicalConfig` (RC:5028).
- Tooltips: Local ".git/config in the repository root", Project ".tgitconfig in the repository root",
  Global "%HOME%/.gitconfig or %XDGHOME%/git/config" (RC:4989-4994).

Loading (`GitSettings.h:124-170`) builds an in-memory libgit2 config from the selected levels:
- Local `.git/config` if (Effective or Local) and repo present — highest priority.
- `.tgitconfig` at worktree root (bare: read from `HEAD:.tgitconfig` into a temp file) if (Effective and page honors
  project) or Project.
- Global `~/.gitconfig` then XDG `~/.config/git/config` if Effective or Global.
- System gitconfig if Effective or System.
- When **Effective** is selected all editing controls are read-only/disabled (e.g. `SettingGitConfig.cpp:226-244`).

Changing radio (`GITSETTINGS_RADIO_EVENT_HANDLE`, `GitSettings.h:262-273`) reloads and auto-selects the matching
"Save to" entry (Local/Project/Global; System is view-only — no "Save to" target).

Each value has an **inherit** checkbox: checked means "key not set at this level"; on save, inherit ⇒ delete the key at
the target level (`Save(..., deleteKey)`, `GitSettings.h:97-122`). Tri-state checkboxes (indeterminate) likewise mean
"unset". Empty combo entry means "unset".

Saving (`GitSettings.h:186-218`):
- Global target (or `m_bGlobal`): writes to XDG file if it exists and `~/.gitconfig` doesn't; else `~/.gitconfig`.
- Project target: `<worktree>/.tgitconfig`.
- Local target: `.git/config`.
- If the "Save to" target differs from the currently viewed source, confirm `"Are you sure you want to save to "%s"?"`
  with Save/Abort buttons (`WarnUserSafeToDifferentDestination`, `GitSettings.h:172-184`, RC:4996).
- Error: `IDS_PROC_SAVECONFIGFAILED` + libgit2 error.

### 1.3 General (`main`) — RC:342-363, `Settings/SetMainPage.cpp:33-47`

Group "TortoiseGit":

| Label | Storage | Type | Default |
|---|---|---|---|
| &Language: | `TG\LanguageID` | DWORD (LCID) combo | `CLangDll::s_defaultLang` (1033 en-US) |
| Automatically check for &newer versions every week | `TG\VersionCheck` | bool | TRUE |
| C&heck now (button) | — | runs update check | |
| Re-run &First Start Wizard (button) | — | | |
| Create &Library (button) | — | Windows library (Win only) | |

Group "Git for Windows":

| Label | Storage | Type | Default |
|---|---|---|---|
| &Git.exe Path: (+ "..." browse) | `TG\MSysGit` (`REG_MSYSGIT_PATH`, `Git/Git.h:30`) | string (dir of git.exe) | empty = autodetect |
| &Extra PATH: | `TG\MSysGitExtra` (`Git/Git.h:32`) | string | empty |
| "Version:" label + C&heck now | `TG\git_cached_version` (cache) | DWORD | 0 |
| Show Environment Variables (button) | — | | |

### 1.4 Context Menu (`look`) — RC:701-716, `Settings/SetLookAndFeelPage.cpp:140-256`

| Label | Storage | Type | Default |
|---|---|---|---|
| Group "Show the selected items on the first level of the context menu:" + note "Unchecked items will appear in the TortoiseGit submenu, checked items directly in the main context menu." | checklist (one row per menu entry, icon + text with accelerators stripped) | 64-bit mask split: `TG\ContextMenuEntries` (low 32), `TG\ContextMenuEntrieshigh` (high 32) | `defaultTopMenuEntries` = Sync \| CreateRepo \| Clone \| Commit (`TortoiseShell/ShellCache.h:43-46`) |
| Select/deselect &all | tri-state; indeterminate click forced to unchecked (`SetLookAndFeelPage.cpp:110-131`) | | |
| Restore Default | resets mask to default (`:249-256`) | | |
| Hide Menus for unversioned paths | `TG\HideMenusForUnversionedItems` | bool | FALSE |
| Group "Do not show the context menu for the following paths:" (multiline) | `TG\NoContextPaths` | string, newline-separated paths (`*` wildcard suffix per docs) | "" |
| Enable drag context menu | `TG\EnableDragContextMenu` | bool | TRUE |

List rows come from `GetTGitMenuInfo()` (`TortoiseShell/MenuInfo.cpp:261`), excluding separators, submenus,
MergeAbort, Inaccessible and consecutive duplicates of the same menuID (`SetLookAndFeelPage.cpp:30-59`).
Admin policy masks `TG\ContextMenuEntriesMaskLow/High` (HKLM and HKCU, `ShellCache.cpp:56-59`) hide entries entirely.

### 1.5 Context Menu 2 (`extmenu`) — RC:1759-1771, `SetLookAndFeelPage.cpp:290-380`

| Label | Storage | Type | Default |
|---|---|---|---|
| Group "Hide the selected items in the context menu:" + note "Checked items will be hidden in the context menu by default and will only be visible if the shift key is pressed while opening the context menu." | `TG\ContextMenuExtEntriesLow` / `TG\ContextMenuExtEntriesHigh` | 64-bit mask | `defaultExtMenuEntries` = SVNIgnore \| StashApply \| SubmoduleSync (`ShellCache.h:48-50`) |
| Select/deselect &all, Restore Default | as above | | |

### 1.6 Windows 11 Context Menu (`win11menu`, Win11 only) — RC:2292

Group "Top context menu on Windows 11", note "Unchecked items are not shown in the top menu but only in the original
menu". Storage `TG\ContextMenu11Entries` (QWORD), default `defaultWin11TopMenuEntries` = Sync, Commit, Switch, Revert,
Conflicteditor, Diff, Pull, Push, RepoBrowser, Log, Blame, Clone, Merge, StashSave, Settings (`ShellCache.h:27-41`).
Buttons: Select/deselect &all, Restore Default, Register context menu. (Platform-specific; likely a divergence.)

### 1.7 Dialogs 1 (`dialog`) — RC:718-758, `Settings/SetDialogs.cpp:47-68, 204-267`

Group "Log messages":

| Label | Storage | Type | Default |
|---|---|---|---|
| Default limitation of log messages: [scale combo] [number] | `TG\LogDialog\NumberOfLogsScale` (combo index) + `TG\LogDialog\NumberOfLogs` | DWORD, DWORD | scale 0 = "No limitation"; number 1 |
| scale combo items (in order) | "No limitation", "Last selected date", "Last N commit(s)", "Last N year(s)", "Last N month(s)", "Last N week(s)" (RC:5101-5111) | | |
| &Font for log messages: [face] [size] | `TG\LogFontName`, `TG\LogFontSize` | string, DWORD | "Consolas", 9 |
| Short &date/time format in log messages | `TG\LogDateFormat` | bool | TRUE |
| Relative Times in log | `TG\RelativeTimes` | bool | FALSE |
| Show asterisk log prefix | `TG\AsteriskLogPrefix` | bool | TRUE |
| Use system locale for date/time | `TG\UseSystemLocaleForDates` | bool | TRUE |
| Can double-click in log list to compare with previous revision | `TG\DiffByDoubleClickInLog` | bool | FALSE |
| Abbreviate renamings | `TG\AbbreviateRenamings` | bool | FALSE |
| Symbolize ref names | `TG\SymbolizeRefNames` | bool | FALSE |
| Enable log cache | `TG\EnableLogCache` | bool | TRUE |
| Enable Gravatar [URL combo] | `TG\EnableGravatar`, `TG\GravatarUrl` | bool, string | FALSE, `https://gravatar.com/avatar/%HASH%?d=identicon` |
| Gravatar URL presets | `https://gravatar.com/avatar/%HASH%` plus `?d=mm`, `?d=identicon`, `?d=monsterid`, `?d=wavatar`, `?d=retro`, `?d=blank` (`SetDialogs.cpp:257-263`) | | |
| Draw tag/branch labels on right side | `TG\DrawTagsBranchesOnRightSide` | bool | FALSE |
| Display subject and body of commit messages | `TG\FullCommitMessageOnLogLine` | bool | FALSE |
| Display branch revision number | `TG\ShowBranchRevisionNumber` | bool | FALSE |

Sub-group "Describe":

| Label | Storage | Type | Default |
|---|---|---|---|
| Show describe in log | `TG\ShowDescribe` | bool | FALSE |
| Describe Strategy | `TG\DescribeStrategy` | combo index: "Annotated tags", "All tags", "All refs" | 0 (`GIT_DESCRIBE_DEFAULT` = annotated tags) |
| Abbreviated size | `TG\DescribeAbbreviatedSize` | DWORD | 7 (`GIT_DESCRIBE_DEFAULT_ABBREVIATED_SIZE`) |
| Always show long format | `TG\DescribeAlwaysLong` | bool | FALSE |
| Only follow first parent | `TG\DescribeOnlyFollowFirstParent` | bool | FALSE |

### 1.8 Dialogs 2 (`dialog2`) — RC:760-800, `Settings/SetDialogs2.cpp:45-73, 143-152`

Group "Misc":

| Label | Storage | Type | Default |
|---|---|---|---|
| &Autoclose Git.exe dialog: | `TG\AutoCloseGitProgress` | combo, item data: 0 "Close manually", 1 "Auto-close if no further options are available", 2 "Auto-close if no errors" (`ProgressDlg.h:36-38`, RC:4389-4393) | 0 |
| Use recycle bin when reverting | `TG\RevertWithRecycleBin` | bool | TRUE |
| Confirm to kill running git process | `TG\ConfirmKillProcess` | bool | FALSE |
| Randomize Sync Dialog startup position | `TG\SyncDialogRandomPos` | bool | FALSE |
| Hide unchanged refs in Ref Compare List | `TG\RefCompareHideUnchanged` | bool | FALSE |
| Show git.exe execution timings and timestamp | `TG\ShowGitexeTimings` | bool | TRUE |
| Sort tag list in reversed order | `TG\SortTagsReversed` | bool | FALSE |
| Disable sounds | `TG\NoSounds` | bool | FALSE |
| Include FETCH_HEAD in branch selection boxes | `TG\BranchesIncludeFetchHead` | bool | TRUE |
| Use .mailmap | `TG\UseMailmap` | bool | TRUE |

Group "Commit":

| Label | Storage | Type | Default |
|---|---|---|---|
| &Use auto-completion of file paths and keywords | `TG\Autocompletion` | bool | TRUE |
| &Timeout in seconds to stop the auto-completion parsing | `TG\AutocompleteParseTimeout` | DWORD | 5 |
| Max. items to keep in the log message history | `TG\MaxHistoryItems` | DWORD | 25 |
| Select items automatically | `TG\SelectFilesForCommit` | bool | TRUE |
| Do not auto-select "missing" files (deleted, but unstaged) | `TG\AutoselectMissingFiles` | bool | FALSE |
| Strip lines starting with "#" in commit message | `TG\StripCommentedLines` | bool | FALSE |
| (note) "Further options for the commit dialog are on Dialogs 3 page." | | | |

### 1.9 Dialogs 3 (`dialog3`) — RC:2039-2071, `Settings/SetDialogs3.cpp:85-210`

Uses the config-source selector (§1.2) with Project enabled. All keys are git config `tgit.*` project properties
(`TortoiseProc/ProjectProperties.h:35-40`).

| Group / Label | git config key | Type | Default / semantics |
|---|---|---|---|
| Commit: "Select the language this project is using. This settings affects the spell checker used for commit messages." Language: | `tgit.projectlanguage` | combo: "" (unset/inherit), "(auto)" (value 0), "(disable)" (value `-1`), then installed languages (LCID) | unset; Effective view falls back to `TG\LanguageID` |
| Minimum number of chars for a commit message: &Limit: + inherit | `tgit.logminsize` | int | unset (0) |
| Char position where to show a border line in commit text boxes: &Border: + inherit | `tgit.logwidthmarker` | int | unset (0 → no marker) |
| &Warn on missing Signed-Off-By on commit | `tgit.warnnosignedoffby` | combo "", "true", "false" (`GitSettings.h:225-241`) | unset |
| Dialogs: &Overlay Icon: [path] [...] + inherit | `tgit.icon` | string (icon file path) | unset |
| Save to: | — | Local / Project / Global | first entry |

### 1.10 Git (`gitconfig`) — RC:1592-1631, `Settings/SettingGitConfig.cpp`

Config source radios: Effective | Local | Global | System (no Project; `InitGitSettings(this, false, …)` line 108).
On first open, if both user.name and user.email are empty in Effective view, the page switches to **Global** and clears
the inherit checks (`SettingGitConfig.cpp:139-149`).

| Group / Label | git config key | Control | Values / default |
|---|---|---|---|
| User Info: &Name: + inherit | `user.name` | edit | Effective view shows `g_Git.GetUserName()` (honours `GIT_AUTHOR_NAME` env etc.) (`:158-166`) |
| &Email: + inherit | `user.email` | edit | as above with `GetUserEmail()` |
| &Signing key ID: + inherit | `user.signingkey` | edit | |
| Auto CrLf convert: Auto&CrLf: | `core.autocrlf` | dropdown `""`, `false`, `true`, `input` (`:88-91`) | `""` = unset (key deleted on save) |
| Sa&feCrLf: | `core.safecrlf` | dropdown `""`, `false`, `true`, `warn` (`:93-96`) | `""` = unset |
| QuotePath | `core.quotepath` | tri-state check | indeterminate = unset; Effective unset ⇒ shown checked (git default true) (`:189-195`) |
| Prune | `fetch.prune` | tri-state check | indeterminate = unset; Effective unset ⇒ unchecked (`:197-203`) |
| Save to: | — | Local / Global | |

Parsing rule: value read with `git_config_get_bool`; if a bool-true → "true"; if not a bool, string compared
case-insensitively to "input"/"warn", otherwise "false" (`:173-219`).
Save (`SafeDataImpl`, `:254-286`): each key written as string or deleted when inherit/indeterminate/empty.

Buttons (all open in the "alternative editor", `CAppUtils::LaunchAlternativeEditor`):
- "Edit &local .git/config" — enabled when in a repo (`:110-113`); bare repo relabels.
- "Edit .tgitconfig" — disabled outside a repo; bare repo: "view" read-only copy of `HEAD:.tgitconfig` (`:318-340`).
- "&View effective config" — runs `git config --show-origin -l` into a read-only temp file (`:342-360`).
- "Edit gl&obal .gitconfig" (`~/.gitconfig`), "Edit global &XDG git/config" (only shown if XDG dir exists, `:131-135`).
- "Edit s&ystemwide gitconfig" (needs elevation; shield icon when not admin) and "View system&wide gitconfig" (shown
  when not admin) (`:124-129, 362-383`).

### 1.11 Git > Remote (`gitremote`) — RC:1633-1655, `Settings/SettingGitRemote.cpp`

Only present when cwd is a repo. All writes go to **local** `.git/config` (`CONFIG_LOCAL`, `:360-416`).

Layout: left "Remote:" listbox (sorted, from `git remote` list, `:98-104`); right-hand editor:

| Label | Storage | Type | Notes |
|---|---|---|---|
| Remote: [edit] + Rename | remote name | string | Rename runs `git.exe remote rename -- <old> <new>` (`:595`) |
| URL: | `remote.<n>.url` | string | backslashes converted to `/` on save (`:500`) |
| Push URL: | `remote.<n>.pushurl` | string | empty ⇒ unset (`:531-532`) |
| Putty Key: [...] | `remote.<n>.puttykeyfile` | string (file) | TortoiseGit-specific key |
| Tags: | `remote.<n>.tagopt` | combo "Reachable" (unset), "None" (`--no-tags`), "All" (`--tags`) (`:106-109, 274-281, 511-520`) | default Reachable |
| Push Default | `remote.pushdefault` | check: set to this remote's name; unchecking clears it only if it equals this remote (`:426-438`) | |
| Prune | `remote.<n>.prune` | tri-state: true / false / indeterminate = unset (`:285-287, 523-526`) | default indeterminate |
| &Add New/Save | | | |
| &Remove | | | `git.exe remote rm -- <name>` after confirm `IDS_WARN_REMOVE` (`:557-570`) |

Behaviour:
- Empty list + empty name: balloon hint suggesting `origin`; typing a URL with empty name and no remotes auto-fills
  "origin" (`:136-137, 313-316`).
- Warns (balloon) if the name collides with an existing `remote.*.fetch` refspec (`:204-232, 301-302`).
- Adding when name exists: confirm "The remote "%s" already exists.\nDo you want to overwrite it?" (`:173-176`).
- Apply with a new name: errors if name or URL empty (`IDS_PROC_GITCONFIG_REMOTEEMPTY` / `URLEMPTY`). If other remotes
  already exist and Tags ≠ None, asks "To avoid fetching wrong tags, if this is not an official remote,\nyou are
  advised to disable tag fetching for this remote.\nDisable tag fetching?" (stored decision `TagOptNoTagsWarning`)
  (`:461-472`). Then runs `git.exe remote add -- <name> <url>` (`:478`), then saves other changed fields.
- After adding, unless launched with `/page:gitremote`, asks "Do you want to fetch remote branches from the newly added
  remote?" and if yes spawns `TortoiseGitProc /command:fetch /path:"<repo>" /remote:<name>` (`:495-496`).
- Only fields in `m_ChangedMask` are written.

### 1.12 Credential (`gitcredential`) — RC:1995

Labels: "Credential helper:", "Helpers:", "Config type:", "URL:", "Helper:", "Username:", "Use HTTP path component",
"&Add New/Save", "&Remove", "Windows Credential Manager"; admin notice "All or some options are limited..." + "Open
&settings". Stores `credential.helper`, `credential.<url>.helper`, `credential.<url>.username`,
`credential.<url>.useHttpPath` in git config (local/global/system per "Config type").

### 1.13 Icon Overlays (`overlay`) — RC:365-398, `Settings/SetOverlayPage.cpp:43-57, 138-200`

Group "Icon Overlays":

| Label | Storage | Type | Default |
|---|---|---|---|
| Show &overlays and context menu only in explorer | `TG\LoadDllOnlyInExplorer` | bool | FALSE |
| Unversioned files mark parent folder as modified | `TG\UnversionedAsModified` | bool | FALSE (enabled only for cache type Default) |
| Show status of subrepositories in parent folders | `TG\TGitCacheRecurseSubmodules` | bool | FALSE (only for Default) |
| Disable status cache for elevated processes | `TG\ShowOverlaysOnlyNonElevated` | bool | FALSE |

Group "Status cache" — radio, `TG\CacheType` DWORD; default 1, or 2 on remote-desktop sessions:

| Radio | Value | Tooltip |
|---|---|---|
| &Default | 1 (exe) | "Status cache kept in an external process that detects file changes, can show the overlay recursively" |
| Shell Extended | 3 (dllFull) | "Status cache for only one folder, with full status and recursive overlays" |
| Shell | 2 (dll) | "Status cache only for one folder, no recursive overlays" |
| None | 0 | "No status cache. Only versioned folders get an overlay, files don't get an overlay" |

Group "Drive Types" (`TG\DriveMask*`, bool): Drives A: and B: (`DriveMaskFloppy`, F), Re&movable drives
(`DriveMaskRemovable`, F), &Network drives (`DriveMaskRemote`, F), &Fixed drives (`DriveMaskFixed`, **T**), &CD-ROM
(`DriveMaskCDROM`, F), R&AM drives (`DriveMaskRAM`, F), &Unknown drives (`DriveMaskUnknown`, F).

| Label | Storage | Type | Default |
|---|---|---|---|
| E&xclude paths: | `TG\OverlayExcludeList` | multiline, newline-separated, `*` suffix = recursive | "" |
| I&nclude paths: | `TG\OverlayIncludeList` | multiline | "" |
| Show excluded folders as normal | `TG\ShowExcludedAsNormal` | bool | TRUE |

Changing these sets `Restart_Cache`.

### 1.14 Icon Set (`overlays`) — RC:652, `Settings/SetOverlayIcons.cpp:32-40, 319-329`

"&Icon Set:" combo listing subdirectories of `%CommonProgramFiles%\TortoiseOverlays\Icons` (default "XPStyle"), preview
list of the 9 states (Normal, Modified, Conflicted, Deleted, Added, Ignored, Unversioned, "assume-valid" (ReadOnly),
"skip-worktree" (Locked)), radio "&List View" / "&Symbol View". On apply writes `TO\NormalIcon`, `ModifiedIcon`,
`ConflictIcon`, `ReadOnlyIcon`, `DeletedIcon`, `LockedIcon`, `AddedIcon`, `IgnoredIcon`, `UnversionedIcon` =
`<iconpath>\<set>\<Name>Icon.ico`, and requests `Restart_System`.

### 1.15 Overlay Handlers (`overlayshandlers`) — RC:400-418, `Settings/SetOverlayHandlers.cpp:35-40`

Group "Enabled Overlay Handlers"; note "Note: this affects all Tortoise clients, not just TortoiseGit!". All bool,
default TRUE, under `TO\`:

| Label | Key |
|---|---|
| Ignored | `ShowIgnoredOverlay` |
| Unversioned | `ShowUnversionedOverlay` |
| Added | `ShowAddedOverlay` |
| Skip worktree ("Locked" in TortoiseSVN) | `ShowLockedOverlay` |
| Assume valid/unchanged ("Needs-lock" in TortoiseSVN) | `ShowReadonlyOverlay` |
| Deleted | `ShowDeletedOverlay` |

Hint label shows how many `ShellIconOverlayIdentifiers` are registered (Windows limit 15); button "Start registry
editor". Windows-only concept (divergence on other platforms).

### 1.16 Network (`proxy`) — RC:420, `Settings/SetProxyPage.cpp:33-37`

"&Enable Proxy Server", "Server &address:", "&Port:", "&Username:", "&Password:" (registry
`TG\Git\Servers\global\http-proxy-host|port|username|password`; applied to git as `http.proxy`), group "SSH": "&SSH
client:" + "Browse..." → `TG\SSH` (default: bundled TortoiseGitPlink; HKLM fallback).

### 1.17 Diff Viewer (`diff`) — RC:664-681, `Settings/SettingsProgsDiff.cpp`

| Group / Label | Storage | Type | Default |
|---|---|---|---|
| "Configure the program used for comparing different revisions of files": radio "&TortoiseGitMerge" / "&External" + path + "..." | `TG\Diff` | string | "" (= TortoiseGitMerge) |
| "&Advanced..." — per-extension diff tools ("Click on "Advanced" to specify alternate diff programs based on file extension") | `TG\DiffTools\<.ext>` = command (`SetProgsAdvDlg.cpp:27-31`) | string values | none |
| "Configure viewer program for GNU diff files (patch files)": radio "TortoiseGit&UDiff" / "E&xternal" + path + "..." | `TG\DiffViewer` | string | "" (= TortoiseGitUDiff) |

Encoding rule: a value is "external" iff non-empty and not starting with `#` (`SettingsProgsDiff.h:58`). Selecting the
built-in radio keeps the custom path but prefixes it with `#` (`SettingsProgsDiff.cpp:98-111`). External diff
placeholders (tooltip RC:3719): `%base`, `%mine` (plus `%bname`, `%yname`, etc. per docs).

### 1.18 Merge Tool (`merge`) — RC:683, `Settings/SettingsProgsMerge.cpp:33-105`

Radio "&TortoiseGitMerge" / "&External" + path → `TG\Merge` (same `#` rule). "Block TortoiseGit while executing the
external merge tool" and "Trust the exitcode of the external merge tool for auto resolving" → combined
`TG\MergeBlockTrustBehavior` (0 none, 1 block, 2 block+trust). "&Advanced..." → `TG\MergeTools\<.ext>`.

### 1.19 Saved Data (`save`) — RC:854-875, `Settings/SetSavedDataPage.cpp`

Each row: label with item count + "Clear" button (disabled when nothing stored).

| Label | What "Clear" removes |
|---|---|
| URL history | `TG\History\{repoURLS, FormatPatchURLS, PullURLS, PushURLS, SubModuleRepoURLS, SyncURL}` (`:200-210`) |
| Log messages (Input dialog) | `TG\History\commit*` keys (`:212-230`) |
| Log messages (Show log dialog) | `%APPDATA%\TortoiseGit\logcache` dir (`:262-268`) |
| Dialog sizes and positions | `TG\TortoiseProc\ResizableState` (`:232-238`) |
| Authentication data | `TG\Auth\…`, SVN auth dirs, Windows credentials matching `git:*` (`:241-260`) |
| Temp files (including Gravatar images) | TortoiseGit temp files (`:285-353`) |
| Stored decisions | "do not show again" values: OldMsysgitVersionWarning, OpenRebaseRemoteBranchEqualsHEAD, OpenRebaseRemoteBranchUnchanged, OpenRebaseRemoteBranchFastForwards, DaemonNoSecurityWarning, NothingToCommitShowUnversioned, NoJumpNotFoundWarning, HintHierarchicalConfig, HintStagingMode, TagOptNoTagsWarning, NoStashIncludeUntrackedWarning, CommitMergeHint, AskSetTrackedBranch, StashPopShowChanges, StashPopShowConflictChanges, CommitWarnOnUnresolved, CommitAskBeforeCancel, PushAllBranches, CommitMessageContainsConflictHint, MergeConflictsNeedsCommit, CommitMessageTemplateNotEdited; `TortoiseGitMerge\DeleteFileWhenEmpty`; key `TG\approvedhooks` (`:355-398`) |

Group "Action log": "Max. lines in action log" → `TG\MaxLinesInLogfile` DWORD default 4000 (`:38`); "Show" opens
`%LOCALAPPDATA%\TortoiseGit\logfile.txt`; "Clear" deletes it.

### 1.20 Other pages (brief)

- **Colors 1/2/3**: status/action colors (conflict, added, missing/deleted/replaced, merged, modified/copied, renamed),
  revision graph note node / unknown ref types, "Use local branch color for current branch", "Dark theme"; log graph
  colors (Current Branch, Local Branch, Remote Branch, Tag, Filter match); branch line colors LINE1..LINE8, "Line
  width", "Node size". Each page has "Restore Default".
- **Alternative editor**: "&Notepad" / "&Custom" + path (same `#` convention; used for editing config files).
- **Hook Scripts**: list with Add.../Edit.../&Copy/Remove (client-side hooks: start-commit, pre-commit, post-commit,
  pre-push, post-push, etc.).
- **Issue Tracker Integration / Config**: Config page edits git config `bugtraq.url`, `bugtraq.warningifnoissue`,
  `bugtraq.message`, `bugtraq.append`, `bugtraq.label`, `bugtraq.number`, `bugtraq.logregex`, provider uuid
  win32/x64 + parameters, each with inherit; uses config-source selector with Project (RC:1005).
- **Email**: Delivery, SMTP Server, Port, From, Encryption, "SMTP Server requires authentication", Login, "&Store
  credentials...", "&Clear".
- **TortoiseGitBlame / TortoiseGitUDiff**: colors, font, tab size; Blame also: "&Detect moved or copied lines:",
  thresholds "Within a file:" / "Between files:", "Ignore whitespace", "Only consider first parents on blame", "Show
  complete log", "Follow renames".
- **Advanced**: editable list of hidden registry values with a WARNING note (RC:1089).

## 2. Remote operations

Paths are relative to `src/`. `TP/` = `src/TortoiseProc/`, `RC` = `src/Resources/TortoiseProcENG.rc`.
`<WD>` in registry keys = repo working dir with `:` replaced by `_` (e.g. `C_\src\repo`). All registry keys are
under `HKCU\Software\TortoiseGit\...`. `Quote(x)` = `CGit::QuoteParameter` (wraps in `"..."`, rejects illegal
params; the `, true` variant also rejects values starting with `-`). All commands are run in the repo root.

Common conventions:
- "- all -" (`IDS_PROC_PUSHFETCH_ALLREMOTES`, RC:4934) is prepended to the remote combo when >1 remote exists (Push and Fetch only, not Pull).
- "Autoload Putty Key" is only enabled when the configured SSH client is PuTTY/TortoiseGitPlink (`CAppUtils::IsSSHPutty()`); when checked, Pageant is launched with `remote.<name>.puttykeyfile` before running git. On non-Windows this is a divergence candidate.
- Git commands run in a `CProgressDlg` (streams output). The libgit2 path for Fetch/Clone is only used if bit `GIT_CMD_FETCH`/`GIT_CMD_CLONE` is set in `UseLibgit2_mask`; the default mask does NOT include them (`Git/Git.h:34`), so the CLI command lines below are the default behaviour.

### 2.1 Push

Launcher: `/command:push [/branch:<ref>]`; with multiple paths it loops over each WC root (`TP/Commands/PushCommand.cpp:22-49`).
Entry: `CAppUtils::Push` (`TP/AppUtils.cpp:2913-2924`) -> `CPushDlg` -> `CAppUtils::DoPush` (`TP/AppUtils.cpp:2738-2911`).

**Dialog `IDD_PUSH`, caption "Push"** (RC:1186-1223), resizable horizontally, window pos saved as `PushDlg`.

| Group | Control (exact label) | Type | Default / persistence |
|---|---|---|---|
| Ref | `&Push all branches` | checkbox | reg `TortoiseProc\Push\<WD>\AllBranches` (DWORD, FALSE); only loaded if no `/branch` given (`TP/PushDlg.cpp:168-170`) |
| Ref | `&Local:` | editable combo of local branches | `/branch` value (strips `refs/heads/`, keeps `remotes/...`); else current branch; else text `HEAD` if detached and repo not empty (`TP/PushDlg.cpp:255-273`) |
| Ref | (split button right of Local) | menu: "Reference Browser", "Log", "Reflog" (`TP/PushDlg.cpp:186-188`) | picks a ref/hash |
| Ref | `&Remote:` | editable combo (remote branch name) | history `History\RemoteBranch\<WD>` value `branch`; plus `branch.<local>.pushbranch` or tracked branch (`GetRemotePushBranch`) (`TP/PushDlg.cpp:329-331`) |
| Ref | `...` | button | opens remote-ref browser |
| Destination | `Re&mote:` (radio, default selected) + dropdown list of remotes + `Mana&ge` button | | selection: `branch.<local>.pushremote` / tracked remote; else reg `History\PushRemote\<WD>`; if only one remote -> it; "- all -" selected if reg `TortoiseProc\Push\<WD>\AllRemotes` or last saved remote was "- all -" (`TP/PushDlg.cpp:218-333`) |
| Destination | `Arbitrary &URL:` (radio) + URL combo | | disabled until radio chosen; history `History\PushURLS\<WD>` value `url`; when chosen it prefills from clipboard if it contains a `git push` link |
| Options | `Force &with lease` | checkbox | FALSE; disabled when Force or Include Tags checked (`TP/PushDlg.cpp:604-621`) |
| Options | `&Force` | checkbox | FALSE; disabled when Force with lease checked |
| Options | `Include &Tags` | checkbox | FALSE; disabled when Force with lease checked |
| Options | `&Autoload Putty Key` | checkbox | reg `History\PushDlgAutoLoad\<WD>`; forced FALSE if not PuTTY |
| Options | `&Set upstream/track remote branch` | checkbox | auto-checked if current HEAD has no tracked branch (`TP/PushDlg.cpp:275-282`) |
| Options | `Always push to the selected remote archive for this local branch` | checkbox | FALSE; sets `branch.<local>.pushremote` |
| Options | `Always push to the selected remote branch for this local branch` | checkbox | FALSE; sets `branch.<local>.pushbranch` |
| Options | `Recurse submodule` | dropdown: `None` / `Check` / `On demand` (RC:4960-4961) | initial from git config `push.recurseSubmodules` (check=1, on-demand=2, else 0), overridden by reg `History\PushRecurseSubmodules\<WD>` (`TP/PushDlg.cpp:195-207`) |
| Options | `Push &option:` | editable combo | history `History\PushOption\<WD>` value `option` |
| | `OK` / `Cancel` / `Help` | | |

Mutual exclusion (`TP/PushDlg.cpp:336-356, 623-638`): "Set upstream" and the two "Always push to..." boxes are mutually exclusive. "Always push..." are disabled when "- all -" remote, Push all branches, Arbitrary URL, or empty Local. "Set upstream" disabled for Arbitrary URL or (empty Local and not Push-all). Push all branches disables Local/Remote combos and their browse buttons.

OK validation (`TP/PushDlg.cpp:406-499`):
- Remote radio with empty remote -> error `IDS_PROC_GITCONFIG_REMOTEEMPTY`. `AllRemotes = (sel==0 && count>1)`.
- Push all branches -> confirm "Do you really want to push all local branches?" (RC:5175, don't-ask-again key `PushAllBranches`, default No).
- Both Local and Remote empty -> Yes/No "The local branch name and the remote branch name are empty.\nContinue?" (RC:3839).
- Local empty, Remote set -> Yes/No "The local branch/tag name is empty. This results in a remote removal.\nContinue?" (RC:3838).
- Remote branch name invalid -> error `IDS_B_T_INVALID`; local name ambiguous (branch and tag) -> `IDS_B_T_NOT_UNIQUE`.
- If local is a local branch: "Always push ... branch" -> `git config branch.<local>.pushbranch <remoteBranch>` (unset if empty); "Always push ... remote archive" -> `branch.<local>.pushremote <remote>` (unset if empty).
- Saves AllBranches, AllRemotes, AutoLoad, RecurseSubmodules, PushRemote, histories.

**Command line** (`TP/AppUtils.cpp:2738-2856`). Pre-push hook (TortoiseGit client hook) runs first; on failure user can choose to continue.
`arg` is built in this order, each item followed by a space:
1. `--tags` if Include Tags AND NOT Push all branches
2. `--force` if Force
3. `--force-with-lease` if Force with lease
4. `--set-upstream` if Set upstream
5. `--recurse-submodules=no|check|on-demand` only if the combo value differs from git config `push.recurseSubmodules` (0=no,1=check,2=on-demand)
6. `--push-option=<Quote(opt,true)>` if Push option non-empty
7. `--progress`

For each remote (all remotes if "- all -", else the selected remote or arbitrary URL):
- Push all branches: `git.exe push --all <arg> -- <Quote(remote)>`, and if Include Tags additionally `git.exe push --tags <arg> -- <Quote(remote)>`.
- Otherwise: `git.exe push <arg> -- <Quote(remote)> [<Quote(local[:remoteBranch], true)>]` (refspec omitted if both empty; `:remoteBranch` only when Remote non-empty -> `:branch` deletes).
- If reg `ShowBranchRevisionNumber` (default FALSE) and not all-branches: also `git.exe rev-list --count --first-parent --end-of-options <Quote(local)> --`.

Post-actions (`TP/AppUtils.cpp:2862-2908`): post-push hook. On failure: if output contains `! [rejected]` offer **Pull** and **Fetch**; always **Push** (retry). On success: **Request pull**, **Push**, **Switch/Checkout**, and **Commit superproject** if inside a submodule.

### 2.2 Pull

Launcher: `/command:pull` (loops over multiple WC roots; `TP/Commands/PullCommand.cpp`), calls `CAppUtils::Pull(hwnd, showPush=false)`.
Entry: `CAppUtils::Pull` (`TP/AppUtils.cpp:2436-2462`). Refuses if a TortoiseGit rebase is active.

**Dialog `IDD_PULLFETCH`, caption "Pull/Fetch"** (RC:1154-1184); runtime window title becomes "Pull" (`IDS_PROGRS_TITLE_PULL`, RC:4786) + repo path (`TP/PullFetchDlg.cpp:255-260`). Window pos `PullFetchDlg`.

| Group | Control | Type | Default / persistence |
|---|---|---|---|
| Remote | `&Remote:` (radio, default) + dropdown list of remotes | | selection: tracked remote of current branch, else reg `History\PullRemote\<WD>` (`TP/PullFetchDlg.cpp:270-334`) |
| Remote | `Arbitrary &URL:` (radio) + URL combo | | history `History\PullURLS` value `url` (global, not per-repo); prefills from clipboard `git pull`/`git fetch` link |
| Remote | `Remote &Branch:` + editable combo + `...` | | first entry: tracked branch of current branch; else submodule's `submodule.<p>.branch` from parent; else current branch name; history `History\PullRemoteBranch` value `br` |
| Options | `&Squash` | checkbox | unchecked |
| Options | `No Co&mmit` | checkbox | unchecked |
| Options | `Depth` + number edit | checkbox | only visible if repo is shallow (then checked, depth=1) (`TP/PullFetchDlg.cpp:235-246`) |
| Options | `No &Fast Forward` | checkbox | unchecked; mutually exclusive with FF only |
| Options | `Fast Forward O&nly` | checkbox | reg `TortoiseProc\PullFetch\<WD>_1\ffonly` |
| Options | `Tags` | tri-state (indeterminate default) | label right shows "Default: None/All/Reachable" from `remote.<r>.tagopt` (`--no-tags`/`--tags`/other) |
| Options | `Prune` | tri-state (indeterminate default) | label right shows "Default: <value>" from `remote.<r>.prune` else `fetch.prune` |
| | `AutoLoad Putty &Key` | checkbox | reg `TortoiseProc\PullFetch\<WD>_1\autoload` (default IsSSHPutty) |
| | `Manage Remotes` | link | opens Settings > Git > Remote |
| | `&Launch Rebase After Fetch` | checkbox | reg `TortoiseProc\PullFetch\<WD>_1\rebase`; forced checked (and locked) if `branch.<cur>.rebase` or `pull.rebase` is true (`merges` => preserve merges) (`TP/PullFetchDlg.cpp:148-195`); disabled for bare repos and for Arbitrary URL |

OK (`TP/PullFetchDlg.cpp:426-469`): with Remote radio, remote-branch is **cleared** when reg `NamedRemoteFetchAll` (default TRUE) and the chosen remote/branch equals the configured tracking remote/branch (so git updates tracking refs). Rebase checked + empty remote branch -> "You asked to pull, but did not specify a remote branch." (RC:5134).

**Command line, no rebase** (`DoPull`, `TP/AppUtils.cpp:2270-2316`):
```
git.exe pull --progress -v --no-rebase[ --no-tags| --tags][ --no-ff][ --ff-only][ --squash][ --no-commit][ --depth N][ --prune| --no-prune][ --allow-unrelated-histories] -- <Quote(remote)> [<Quote(remoteBranch,true)>]
```
Tags: unchecked -> `--no-tags`, checked -> `--tags`, indeterminate -> nothing. Prune likewise. `--allow-unrelated-histories` only on the "Merge unrelated histories" retry post-action.

**With "Launch Rebase After Fetch"** pull is turned into fetch + rebase (`TP/AppUtils.cpp:2445-2457`): `DoFetch(..., runRebase = configRebase ? 2 : 1, preserveMerges)`; see 2.3. Upstream = `remotes/<remote>/<branch>` (or tracked branch when branch was cleared), else `FETCH_HEAD`.

Post-actions (`TP/AppUtils.cpp:2324-2420`): on failure with conflicts -> hint dialog + **Resolve** and **Commit**; if no merge base with remote ref -> **Merge unrelated histories**; plus **Pull**, **Stash Save**, **Reset** (Reset dialog preselecting `remotes/<trackRemote>/<trackBranch>` with type 2 = Hard; 0=Soft,1=Mixed — `GitReset(hwnd, upstream, 2)`, `TP/AppUtils.cpp:1655`). On success: **Stash Pop** (if invoked with showStashPop), **Pulled Diff** (`/command:showcompare /revision1:<old> /revision2:<new>`), **Pulled Log** (`/command:log /range:<old>..<new>`), **Push** (if showPush), **Submodule Update** (`/command:subupdate /bkpath:`) if the repo has submodules. If output contains `CONFLICT`, asks to open `/command:repostatus`.

### 2.3 Fetch

Launcher: `/command:fetch [/remote:<name>]` (`TP/Commands/FetchCommand.cpp:22-45`). Entry `CAppUtils::Fetch(hwnd, remoteName, allRemotes)` (`TP/AppUtils.cpp:2725-2736`).

Same dialog `IDD_PULLFETCH`, title "Fetch" (RC:4787). Differences from Pull:
- Remote list gets "- all -" first when >1 remote (`TP/PullFetchDlg.cpp:322`); `/remote` preselects.
- Options group, Squash, No FF, FF only, No commit disabled (`TP/PullFetchDlg.cpp:222-229`).
- If `NamedRemoteFetchAll` (default TRUE): Remote Branch combo + `...` disabled for named remotes, and remote branch is always cleared (fetch uses configured refspecs).
- Registry keys use suffix `_0`: `TortoiseProc\PullFetch\<WD>_0\{rebase,ffonly,autoload}`.
- "Launch Rebase After Fetch" -> runRebase = 1.

**Command line** (`DoFetch`, `TP/AppUtils.cpp:2521-2580`):
```
arg = " --progress"[ + " -v" if reg FetchVerbose (default TRUE)][ + " --depth N"][ + " --prune" | " --no-prune"][ + " --tags" | " --no-tags"]
all remotes:  git.exe fetch --all <arg>
otherwise:    git.exe fetch <arg> -- <Quote(remote-or-url)> [<Quote(remoteBranch,true)>]
```
(Prune/Tags tri-state: checked/unchecked add flag, indeterminate adds nothing.) Pageant launched for every remote when all-remotes.

Rebase after fetch (`TP/AppUtils.cpp:2648-2719`), on exit code 0:
- runRebase==1: if HEAD already equals / contains upstream -> ask "current branch up to date, rebase anyway?" (don't-ask key `OpenRebaseRemoteBranchEqualsHEAD`); if upstream unchanged by fetch -> ask (`OpenRebaseRemoteBranchUnchanged`).
- runRebase==1 and HEAD is ancestor of upstream (fast-forward possible): 3-button box **Merge / Rebase / Abort** (`OpenRebaseRemoteBranchFastForwards`); Merge runs `git.exe merge --ff-only -- <Quote(upstream)>` (auto-close if no errors).
- Otherwise opens Rebase dialog with `m_Upstream=upstream`, autostart when runRebase==2 (`RebaseAfterFetch`, `TP/AppUtils.cpp:2464-2519`); its post buttons: Log, Push, Send mail (`git.exe format-patch -o <dir> --end-of-options <up>..<branch>`), Rebase again.

Post-actions: failure -> **Retry**, **Log** (if all remotes). Success -> **Log**, **Reset** (to `remotes/<track>`), **Fetch**, **Rebase** (if not already rebasing and not bare), **Switch/Checkout** (`TP/AppUtils.cpp:2585-2622`).

libgit2 variant (only if enabled in mask): `FetchProgressCommand` (`TP/ProgressCommands/FetchProgressCommand.cpp:24-86`): lookup named remote or anonymous URL, prune/tags mapped from tri-states, refspec added via `git_remote_add_fetch`, title "Fetch".

### 2.4 Sync

Launcher: `/command:sync [/seq:N]` (`TP/Commands/SyncCommand.cpp:26-35`); `/seq` + reg `SyncDialogRandomPos` offsets the window.

**Dialog `IDD_SYNC`, caption "Git Synchronization"** (RC:1679-1711), title set to repo path, resizable, pos `SyncDlg`. Modeless-style: runs git itself and shows output in tabs.

Top area controls:
- `&Local Branch:` dropdown list (local branches, current selected) + `...` (ref browser)
- `&Remote Branch:` editable combo + `...`
- `Remote &URL:` editable combo: history `History\SyncURL\<WD>` value `url` + all remotes; first item selected (`TP/SyncDlg.cpp:1219-1232`) + `Manage` button
- `Autoload Putty &Key` checkbox: reg `TortoiseProc\Sync\<WD>\AutoLoadPutty` (default IsSSHPutty)
- `&Force` checkbox (default unchecked)

Tabs (`TP/SyncDlg.cpp:1023-1133`): `Log` (git output), `In Commits`, `In Changelist`, `Conflicts`, `Out Commits`, `Out Changelist`, `Log` (libgit2 progress), `Ref List`, `Compare tags`. Out Commits auto-populates "N commits ahead "<remote>"" (`IDS_PROC_SYNC_COMMITSAHEAD`) or "no fast-forward" warnings.

Bottom split buttons (last choice persisted in `TortoiseProc\Sync\<WD>\{Pull,Push,Submodule}`):

| Button | Entries (index: label) |
|---|---|
| Pull | 0 `&Pull`, 1 `Fetc&h`, 2 `Fetch && Re&base`, 3 `Fetch all refs`, 4 `Remote Update`, 5 `Clean up stale remote branches`, 6 `Compare tags` (RC:3584-3589, 5136, 5182) |
| Push | 0 `Pus&h`, 1 `Push ta&gs`, 2 `Push notes` (RC:4876, 3583, 4970) |
| Submodule | 0 `Submodule Update`, 1 `Submodule Init`, 2 `Submodule Sync` |
| Stash | 0 Stash Save, 1 Stash Pop, 2 Stash Apply (menu strings) |
| `&Apply Patch`, `&Email Patch`, `Show &log`, `&Commit`, `Close`, `Help` | plain buttons |

Shift+click on Pull(0)/Fetch(1) opens the regular Pull/Fetch dialog; Shift+Push(0) opens the Push dialog; Shift+Stash(0) runs `/command:stashsave`.

Pull button behaviour (`TP/SyncDlg.cpp:137-512`):
- Entry 0 requires Local Branch == current branch; else asks "Pull requires the selected local branch to be the current branch..." (RC:4863) with `Switch to "<b>"` / Abort, running `git.exe checkout --end-of-options <Quote(b)> --`.
- Empty URL -> `IDS_PROC_GITCONFIG_URLEMPTY`.
- If named remote, Remote Branch set, entry 0 (or Push entry 0), and reg `AskSetTrackedBranch` (default TRUE) and no tracking configured: asks to set `branch.<b>.remote`/`branch.<b>.merge` (RC:5137).
- `force = " --force"` if Force.
- Entry 0 is redirected to Fetch&Rebase-like flow when `branch.<cur>.rebase`/`pull.rebase` is true (`m_iPullRebase`, 2 for `merges`).
- **Pull (0)**: remoteBranch is cleared if it equals the configured tracking branch for the named remote.
  `git.exe pull -v --progress[ --force] -- <Quote(url)> [<Quote(remoteBranch)>]` (`TP/SyncDlg.cpp:390`)
- **Fetch (1) / Fetch&Rebase (2) / Fetch all refs (3)**: refspec = empty for entry 3; = `<remoteBranch>` for URL or empty branch; else `<rb>:remotes/<url>/<rb>` if that tracking ref exists, else `<rb>`.
  `git.exe fetch --progress[ -v if FetchVerbose][ --force] -- <Quote(url)> [<Quote(refspec)>]` (`TP/SyncDlg.cpp:457`). After entry 2 (or pull.rebase), FetchComplete does the same unchanged/FF checks and `merge --ff-only` / Rebase dialog flow as 2.3 (`TP/SyncDlg.cpp:582-676`).
- **Remote Update (4)**: `git.exe remote update` (Pageant for all remotes).
- **Clean up stale (5)**: `git.exe remote prune -- <Quote(url)>`.
- **Compare tags (6)**: no git process; fills tag comparison list via ls-remote equivalent.

Push button (`TP/SyncDlg.cpp:711-818`): pre-push hook, then refName = local branch (entry 2: notes ref `refs/notes/commits` from `GetNotesRef`); entry 1 adds ` --tags`; `:remoteBranch` appended unless entry 2.
`git.exe push -v --progress[ --tags][ --force] -- <Quote(url)> <Quote(refName[:remoteBranch])>`
Submodule (`TP/SyncDlg.cpp:1773-1786`): `git.exe submodule update --init --recursive[ --force]` / `git.exe submodule init` / `git.exe submodule sync --recursive`.
Stash (`TP/SyncDlg.cpp:1823-1834`): `git.exe stash save` / `git.exe stash pop` / `git.exe stash apply`.
Apply Patch: `git.exe am -- <Quote(file)>` per selected patch (`TP/SyncDlg.cpp:839`). Email Patch: `git.exe format-patch -o <Quote(repo)> --end-of-options <Quote(url)>/<Quote(rb)>..<Quote(local)>` then Send Mail dialog (`TP/SyncDlg.cpp:915`).

### 2.5 Clone

Launcher: `/command:clone [/path:<dir>] [/url:<url>] [/exactpath] [/hasurlhandler]` (`TP/Commands/CloneCommand.cpp:72-89`). Directory defaults to `/path` (else CWD unless `/hasurlhandler`).

**Dialog `IDD_CLONE`, caption "Git clone - TortoiseGit"** (RC:1098-1137), pos `CloneDlg`.

| Group | Control | Default / persistence |
|---|---|---|
| Clone Existing Repository | `&URL:` combo | `/url`; else clipboard `git clone <url>` link (quotes stripped); else first history item. History `History\repoURLS` value `url` |
| | split button `&Browse...` / `&Open` (repo browser) | last choice reg `TortoiseProc\CloneBrowse` (0) |
| | `&Directory:` edit + `Bro&wse...` | from launcher; else parent of reg `TortoiseProc\CloneDir`; else Documents folder (`TP/CloneDlg.cpp:143-156`). On URL edit, the module name (last path segment of URL, lowercased check for trailing `.git` stripped, also splits on `:`/`@`) replaces the previous module suffix: `<dir>\<module>` (`TP/CloneDlg.cpp:355-392`) — skipped when `/exactpath` |
| | `Depth` + number edit | unchecked, 1 |
| | `Recursive` | reg `TortoiseProc\CloneRecursive` (FALSE) |
| | `Clone into Bare Repo` | unchecked; disables & clears Recursive, No Checkout, Origin Name |
| | `No Checkout` | unchecked |
| | `Branch` + edit | unchecked; must be a valid branch name |
| | `Origin Name` + edit | unchecked; must be non-empty unless SVN |
| | `Load Putty &Key` + key-file combo + `...` | reg `TortoiseProc\CloneUseSSHKey` (TRUE) && IsSSHPutty; auto-unchecked when URL contains `http://`, `https://`, `git://`; key history `History\puttykey` value `key` |
| From SVN Repository | `From &SVN Repository` | unchecked; enables the following |
| | `&Trunk:` + edit | unchecked, `trunk` |
| | `Ta&gs:` + edit | unchecked, `tags` |
| | `Branc&h:` + edit | unchecked, `branches` |
| | `&From:` + number | unchecked, 0 |
| | `User&name:` + edit | unchecked |

Tooltips: Depth (RC:4735), Recursive "Submodules are initialized and cloned using their default settings.", Bare "Clone into a git repository without the working tree.", Branch (RC:4000).
OK: URL or Directory empty -> `IDS_PROC_CLONE_URLDIREMPTY`; saves URL/key history, `CloneDir`, `CloneUseSSHKey`, `CloneRecursive` (`TP/CloneDlg.cpp:225-262`).

**Command line** (`TP/Commands/CloneCommand.cpp:91-148`). URL starting with `\\` has `\` -> `/` (after `@` only if present).
```
git.exe clone --progress[ --recursive][ --bare][ --no-checkout][ --branch <Quote(branch)>][ --origin <Quote(origin)>][ --depth N] -v -- <Quote(url)> <Quote(dir)>
```
(`--origin` only when not SVN.) Process runs with CWD = nearest existing ancestor of target dir (else temp) (`CloneCommand.cpp:31-48, 273-275`).

SVN clone (`CloneCommand.cpp:173-216`): local paths converted to `file:///C/...` or `file://` UNC.
```
git.exe svn clone[ --prefix <Quote(origin + "/")> | --prefix ""][ -T <trunk>][ -b <branches>][ -t <tags>][ -r <from>:HEAD][ --username <user>] -- <Quote(url)> <Quote(dir)>
```
Post: if Putty key used, write `remote.<origin|"origin">.puttykeyfile = <key>` to new repo config; bare -> set folder icon. Post-actions: failure **Retry**; success **Show log** (`/command:log /path:<dir>`), **Explore to** (`CloneCommand.cpp:150-171`).

libgit2 variant (only if enabled in mask): `CloneProgressCommand` with url, bare, checkout-branch, remote name, no-checkout (`CloneCommand.cpp:219-245`).

## 3. Branch, stash, reset and rebase operations

Paths are relative to `src/TortoiseProc/` unless noted; RC = `src/Resources/TortoiseProcENG.rc`.
`Quote(x)` = `CGit::QuoteParameter(x)` (wraps in `"..."`, rejects illegal values with an error box).
`Fix(x)` = `CGit::FixBranchName(x)`: only rewrites the literal `FETCH_HEAD` to the dereferenced hash (`../Git/Git.cpp:1561-1573`).
Registry keys are under `HKCU\Software\TortoiseGit\`. "Progress dialog" = `CProgressDlg` that runs one git command, streams output and offers post-command buttons.

### 3.0 Shared "choose version" block (`ChooseVersion.h`)

Used by Switch, Create Branch/Tag, Merge, Reset. It has a radio group with combo boxes:
- `&HEAD` (only in Create Branch/Tag; label is suffixed at runtime with ` (<current branch>)`, `CreateBranchTagDlg.cpp:119-122`)
- `&Branch` + drop-down list of branches (local and `remotes/<remote>/<branch>`; FETCH_HEAD is included if `BranchesIncludeFetchHead` DWORD, default 1, `ChooseVersion.h:211`) + `...` button (opens the Browse References dialog)
- `&Tag` + drop-down list of tags
- `&Commit` + editable combo + `...` button (opens a log dialog so the user can pick a commit)

Resolving the selection to `m_VersionName` (`ChooseVersion.h:121-140`): HEAD gives `HEAD`. A branch gives its name, or `refs/heads/<name>` when the short name is ambiguous (`IsBranchTagNameUnique`). A tag gives its name, or `refs/tags/<name>` when ambiguous. A commit gives the typed text verbatim.

### 3.1 Switch/Checkout (`/command:switch [/rev:<ref>]`)

Entry: `Commands/SwitchCommand.cpp` → `CAppUtils::Switch(hwnd, rev)` (`AppUtils.cpp:1266`).
Dialog `IDD_GITSWITCH`, caption **"Switch/Checkout"**. The window title is prefixed with the repo path through `SetWindowTitle` (RC:1259).

| Control | Label | Default |
|---|---|---|
| Group | `Switch To` | radio `&Branch` selected (`GitSwitchDlg.cpp:107`) |
| Group | `Option` | |
| Check + edit | `Create &New Branch` + name edit | unchecked; edit/override enabled only when checked |
| Check | `Overwrite working tree changes (&force)` | unchecked |
| Check | `&Merge` | unchecked. Tooltip: "Merge between original branch, working tree contents, and the branch to switch to" |
| Tri-state check | `T&rack` | indeterminate, disabled (`:38`, `:110`). Tooltip: "If checked --track is passed to git, if unchecked --no-track is passed to git, else neither ..." |
| Check | `&Override branch if exists` | unchecked |
| Buttons | `OK`, `Cancel`, `Help` | |

Auto-default logic (`SetDefaultName`, `GitSwitchDlg.cpp:177-226`), run whenever the selected ref changes:
- If the ref starts with `origin/` or `remotes/<r>/`, the new-branch name is the text after the remote. `Create New Branch` is checked, `Track` is enabled and set to indeterminate (2).
- Otherwise the name is `Branch_<version>`; for a commit the version is the hash shortened to the short-hash length. `Create New Branch` is unchecked, `Track` is disabled and unchecked.
- For the Commit radio, `Create New Branch` is forced on if `SwitchToCommitNewBranch` is set (DWORD, default 1). For the Tag radio, the key is `SwitchToTagNewBranch` (DWORD, default 1). On OK, the current checkbox state is saved back to the matching key (`:40-41`, `:164-167`).
- When `Create New Branch` is off, Override is cleared and Track is disabled and unchecked.

OK validation (`:136-171`): an invalid branch name shows the balloon "IDS_B_T_NOTEMPTY". An existing branch without Override shows the "branch exists, choose a different name or override" balloon. If a tag has the same name, a Continue/Abort prompt appears.
Before running, a `refs/heads/` prefix on the version is stripped unless Override is checked (`AppUtils.cpp:1277-1278`).

**Command** (`PerformSwitch`, `AppUtils.cpp:1288-1310`):
```
git.exe checkout[ -f][ --track| --no-track][ -B|-b "<newbranch>"][ --merge] --end-of-options "<Fix(ref)>" --
```
- `-f` if force is checked.
- `--track` / `--no-track` only when a new branch is given and Track is checked / unchecked (indeterminate adds neither).
- `-B` if Override is checked, otherwise `-b`.
- `--merge` if Merge is checked.

Post-command buttons on success (`:1316-1345`): `Update submodules` (if the repo has submodules; runs `/command:subupdate /bkpath:`), `Merge` (merge the previous branch), `Pull` (if now on a branch), `Commit`.
On failure: `Resolve` (only with --merge and conflicts), `Stash Save` (only without --merge), `Retry`, `Checkout with merge` (without --merge; reruns with `bMerge=true`). With `--merge`, exit code 0 but conflicts present is treated as a failure with "Has merge conflict" (`:1365-1372`).

### 3.2 Create Branch (`/command:branch [/rev:<ref>] [/name:<name>]`) and Create Tag (`/command:tag ...`)

Entry: `Commands/BranchCommand.cpp`, `Commands/TagCommand.cpp` → `CAppUtils::CreateBranchTag(hwnd, isTag, rev|null, false, name)` (`AppUtils.cpp:1124`).
Dialog `IDD_NEW_BRANCH_TAG` (RC:1287, caption "New Branch/Tag"). The runtime title is **"Create Branch"** or **"Create Tag"** (`CreateBranchTagDlg.cpp:89,101`).

| Control | Label | Default / notes |
|---|---|---|
| Group `&Name` | static `Branch:`, or `Tag` in tag mode | name edit; prefilled from `/name:` |
| Group `Base On` | `&HEAD (<cur>)` / `&Branch` / `&Tag` / `&Commit` | HEAD (`:79`); `/rev:` preselects |
| Group `Options` | `Trac&k` (tri-state) | indeterminate. Enabled only when the base is a `remotes/...` branch in branch mode (`:212-238`). Selecting a remote branch auto-fills the name with the text after `remotes/<r>/` if the name is empty or still the previous default. Editing the name to something else unchecks Track (`:241-251`). |
| | `&Force` | unchecked. Tooltip "Force creation of branch/tag - even if one with same name already exists." |
| | `&Switch to new branch` | branch mode only and not in a bare repo; default = `NewBranchSwitchTo` (DWORD, default 0), saved on OK (`:37,105-109,204`) |
| | `Si&gn` | tag mode only; enabled only when git config `user.signingkey` is non-empty (`:91`). Tooltip "GPG signs the tag, see manual for details" |
| | `&Push` | tag mode only; default = `PushTag` (DWORD, default 0), saved on OK (`:38,93-97,206`) |
| Group `&Message` (branch mode: `Description`) | multi-line edit | empty |

Validation (`:162-209`):
- Sign with an empty message is rejected ("IDS_COMMITDLG_NOMESSAGE").
- An invalid name shows a balloon.
- If the name exists and Force is unchecked, a balloon appears; for the current branch, the balloon says forcing the current branch is impossible.
- A branch and a tag with the same name trigger a Continue/Abort prompt.

**Tag command** (`AppUtils.cpp:1141-1166`):
```
git.exe tag[ -f][ -s][ -F "<tempfile>"] -- "<name>" "<Fix(base)>"
```
`-F` is added only when the trimmed message is non-empty; the message is written to a temp file as UTF-8 (`SaveCommitUnicodeFile`). No `-a` flag is used: `-F` alone makes an annotated tag.

**Branch command** (`:1170-1190`):
```
git.exe branch[ -f][ --track| --no-track] -- "<name>" "<Fix(base)>"
```
Track: checked gives `--track`, unchecked gives `--no-track`, indeterminate adds nothing.

Both run synchronously (`g_Git.Run`); on failure the output is shown in a message box. Afterwards (`:1197-1206`):
- Branch with Switch checked: `PerformSwitch(name)` (= `git.exe checkout --end-of-options "<name>" --`).
- Branch with a non-empty message: git config `branch.<name>.description` is set to the message with CR removed and trimmed; it is unset if empty (`UpdateBranchDescription`, `:1107-1122`).
- Tag with Push checked: opens the Push dialog with ref `refs/tags/<name>`.

### 3.3 Merge (`/command:merge`, `/command:merge /abort`)

Entry: `Commands/MergeCommand.cpp`. `/abort` calls `MergeAbort`; otherwise `CAppUtils::Merge` (`AppUtils.cpp:3378`) runs. Merge first requires user.name/email (`CheckUserData`) and refuses while a TortoiseGit rebase is active.
Dialog `IDD_MERGE`, caption **"Merge"** (RC:1370).

| Control | Label | Default |
|---|---|---|
| Static + read-only edit | `Current branch:` | current branch |
| Group `From` | `&Branch` / `&Tag` / `&Commit` choose-version | Branch. The initial ref is the push remote branch `remotes/<pr>/<pb>` of the current branch, if any (`MergeDlg.cpp:150-159`). The current branch is excluded from the list (`m_bSkipCurrentBranch`, `:47`). |
| Group `Option` | `&Squash` | unchecked. Checking it disables the message editor and `No Fast Forward` |
| | `Messages` + number edit | unchecked; the number defaults to git config `merge.log` if > 0, else 20 (`:44-46`); the edit is enabled only when checked |
| | `No &Fast Forward` | unchecked; mutually disables `Fast Forward O&nly` and `Squash` |
| | `Fast Forward O&nly` | unchecked; disables `No Fast Forward` |
| | `No Co&mmit` | unchecked |
| | `Strategy` combo | empty; items `resolve`, `recursive`, `ours`, `subtree` (`:133-136`) |
| | strategy-option combo | disabled unless the strategy is `recursive`; items `ours`, `theirs`, `patience`, `ignore-space-change`, `ignore-all-space`, `ignore-space-at-eol`, `renormalize`, `no-renormalize`, `rename-threshold`, `subtree` |
| | strategy-param edit | enabled only for `rename-threshold` or `subtree` |
| Group `Merge &Message` | Scintilla editor | text `<Auto Generated by Git>`; if left unchanged it is treated as empty (`:184-186`). Non-empty messages go into the commit-message history (max `MaxHistoryItems`, default 25). |

Validation: Branch radio with no selection is rejected. On OK, the option is dropped if the strategy is not `recursive`, and the param is dropped unless the option is `rename-threshold`/`subtree` (`:198-201`).

**Command** (`DoMerge`, `AppUtils.cpp:3231-3278`):
```
git.exe merge[ --no-ff| --ff-only][ --squash][ --no-commit][ --allow-unrelated-histories][ --log=<n>][ --strategy=<s>[ --strategy-option=<o>[=<p>]]][ -F "<tempfile>"] -- "<Fix(version)>"
```
- `--allow-unrelated-histories` is added only by the post-failure button "&Merge unrelated history". That button appears when `HEAD` and the version have no merge base (`:3313-3318`).

Post-command buttons:
- Failure with conflicts: an info box (do-not-show key `MergeConflictsNeedsCommit`), then `Resolve`, `Commit`, `Merge unrelated history` (when the histories are unrelated) and `Stash Save`. That Stash Save then offers Merge again.
- Success: `Stash Pop` (if called with showStashPop).
  - With `--no-commit` or `--squash`: only `Commit`.
  - Otherwise, when a branch was merged: `Remove branch` (not for `remotes/...`; confirms, then `git.exe branch -D -- "<version>"`, `:3350`) and `Push`.
  - Also `SVN DCommit` if git-svn is present.
- Cancelling the progress dialog while conflicts exist opens Merge Abort (`:3369-3373`).

**Merge Abort** dialog `IDD_MERGEABORT`, caption **"Abort Merge"** (RC:2020). Reminder text: "In order to abort a merge progress a reset (to HEAD) is needed."
- Group `Reset Type`. Radios: `M&erge: Resets the index and try to reconstruct the pre-merge state` (default, `MergeAbortDlg.cpp:35`), `&Mixed: Leave working tree untouched, reset index`, `&Hard: Reset working tree and index (discard all local changes)`.
- Button `Show modified files in &working tree`.
- Mapping: Merge gives reset type 3, which runs `git.exe reset --merge`. Mixed and Hard run `git.exe reset --mixed|--hard --end-of-options "HEAD" --` (`AppUtils.cpp:3400`, `:1558-1561`).

### 3.4 Stash

Command names (`Commands/Command.cpp:219-222`): `stashsave`, `stashapply`, `stashpop`, `stashlist`. The shell's "Stash List" actually launches `/command:reflog /ref:refs/stash` (`../TortoiseShell/ContextMenu.cpp:1464-1466`; also `GitLogListAction.cpp:795`).

**Stash Save** (`/command:stashsave [/msg:<text>]`, `Commands/StashCommand.cpp`). Requires user.name/email.
Dialog `IDD_STASH`, caption **"Stash"** (RC:1812):
- Group `Stash &Message`: single-line edit, prefilled from `/msg:`.
- Group `Options`:
  - `include &untracked` (default off; checking it disables `--all`).
  - `--&all` (default off; checking it disables `include untracked`) (`StashSave.cpp:31-32,94-104`).
- On OK with include untracked checked, a warning offers Continue/Abort: "You have checked "include untracked". This includes a "git clean -fd" call, so ignored files/directories are removed w/o using recycle bin." Do-not-show key: `NoStashIncludeUntrackedWarning`; only Continue is remembered (`StashSave.cpp:80-88`).

Command (`AppUtils.cpp:106-133`):
```
git.exe stash push[ --include-untracked| --all][ -m "<message>"]
```
It runs in a progress dialog. Post buttons on success:
- `Pull` / `Merge`, only when a caller asked for them.
- If `refs/stash` changed: `Stash Pop` and `Stash Apply`.

**Stash Apply** (`/command:stashapply`) → `StashApply(hwnd, "")` (`AppUtils.cpp:161-208`):
```
git.exe stash apply <ref>
```
- The ref is empty from the command, which applies the latest stash. It is unquoted.
- A `refs/` prefix is stripped, and `stash{N}` becomes `stash@{N}` (from the reflog list, `GitLogListAction.cpp:799`).
- It runs synchronously with a modeless "running" box.
- Exit code 1 with `CONFLICT` in the output still counts as success: "Stash Apply successful", or the conflicts variant. The user is then asked whether to see the changes; Yes opens `/command:repostatus`.

**Stash Pop** (`/command:stashpop`) → `StashPop(hwnd, 1)` (`AppUtils.cpp:210-251`):
```
git.exe stash pop
```
- Same conflict handling as Apply.
- The see-changes prompt is a check box with a remembered answer: key `StashPopShowChanges`, or `StashPopShowConflictChanges` when there are conflicts.
- `showChanges`: 0 = silent unless conflicts, 1 = ask, >1 = plain info box.
- Quirk: `StashPopCommand::Execute` returns `!StashPop(...)` (`Commands/StashCommand.cpp`), which inverts the process exit status.

**Stash List** = RefLog dialog (`IDD_REFLOG`, caption **"RefLog"**, RC:1555) with `&Ref:` set to `refs/stash`.
- `&Clear stash` button: visible only for `refs/stash` and enabled when the list is non-empty. It confirms "Do you really want to delete ALL %d stash?" and then runs `git.exe stash clear` (`RefLogDlg.cpp:117-131,164-170`).
- Context menu:
  - Stash Apply.
  - Delete selected: entries are processed from the end; `stash{N}` is normalized to `stash@{N}` and runs `git.exe stash drop -- "stash@{N}"`. Other reflogs run `git.exe reflog delete -- "<ref>"` (`GitLogListAction.cpp:829-860`).
  - Stash Pop applies to the top entry only (`:789-792`).

### 3.5 Reset (from Log/Repo-browser context menu: "Reset <branch> to this...")

No `/command:` exists for it. `CAppUtils::GitReset(hwnd, ref, type=1)` (`AppUtils.cpp:1655`; `AppUtils.h:171`).
Dialog `IDD_RESET`, caption **"Reset"** (RC:1470):
- `Current branch:` (read-only).
- Group `Reset active branch`: `&Branch`/`&Tag`/`&Commit` choose-version, prefilled with the ref.
- Group `Reset Type`:
  - `&Soft: Leave working tree and index untouched`
  - `&Mixed: Leave working tree untouched, reset index` (default, `ResetDlg.cpp:36`)
  - `&Hard: Reset working tree and index (discard all local changes)`
- Button `Show modified files in &working tree`: opens a file diff of HEAD against the working tree.
- In a bare repo, Mixed and Hard are disabled and Soft is forced (`ResetDlg.cpp:80-85`).

Command (`Reset`, `AppUtils.cpp:1544-1652`):
```
git.exe reset --soft|--mixed|--hard --end-of-options "<ref>" --
```
- When libgit2 is configured for reset (`UsingLibGit2(GIT_CMD_RESET)`), it uses `ResetProgressCommand` (`ProgressCommands/ResetProgressCommand.cpp`) instead: a `git_reset` with checkout progress, reported as "Reset <type> <rev>".
- Post buttons on failure: `Retry`.
- Post buttons on success:
  - `Update submodules` (hard reset with submodules only).
  - `Bisect good/bad/skip/reset` (while bisecting).
  - `Clean up` (`/command:cleanup`, hard only).

### 3.6 Rebase (`/command:rebase [/upstream:<ref>] [/force]`)

Entry: `Commands/RebaseCommand.cpp`. It loops showing `CRebaseDlg`. The post button has two entries: `Show Log`, which runs `/command:log`, and `Restart Rebase`, which reopens the dialog.
Dialog `IDD_REBASE`, caption **"Rebase"** (RC:1499). The same dialog is also used for cherry-pick, with title "Cherry Pick".

Controls:
- `&Branch:` combo: local branches, defaulting to the current branch.
- Swap button: an icon that reverses branch and upstream.
- `&Upstream:` combo: all branches including FETCH_HEAD, plus tags.
  - Default: `/upstream:` if given.
  - Otherwise `remotes/<remote>/<branch>` of HEAD's tracked branch if it exists, else no selection (`RebaseDlg.cpp:492-507`).
- `...` browse button.
- `&Onto` toggle button: picks an extra ref through Browse Refs. It is ignored if it equals the upstream or the branch (`:3015-3036`).
- Commit list: `upstream..branch` in topo order, excluding merges unless Preserve merges is on (`:599-604`). Each row has an action: Pick/Squash/Edit/Skip.
- Split button with the entries `&Pick ALL`, `&Squash ALL`, `&Edit ALL`, `S&kip unselected`, `S&quash unselected`, `E&dit unselected`.
- `&Up`, `&Down`, `&Add` buttons.
- `&Preserve merges` (default off).
- `&Force Rebase` (default off; set by `/force`). Tooltip: "Force the rebase even if the current branch is a descendant of the commit you are rebasing onto..."
- `&add "cherry picked from"`: shown in cherry-pick mode; remembered in `TortoiseProc\CherrypickAddCherryPickedFrom`.
- Tabs for revision files, log message and log output.
- Progress bar and status text.
- Buttons: `Continue` (its text changes per stage: Start Rebase / Continue / Finish / Done), `Abort`, `Help`.

Pre-checks (`CheckRebaseCondition`, `:946-1095`):
- **Fast-forward** (branch is an ancestor of upstream, no Onto): the dialog shows the FF text. On run (`:1253-1310`):
  - If the branch is local: `git.exe checkout --no-track -f -B "<branch>" --end-of-options "<upstream>" --`
  - Then: `git.exe reset --hard --end-of-options "<Fix(upstream)>" --`
- **Up to date** (merge base equals upstream, no Force, no Onto): Continue is disabled.
- **Dirty work tree:** if git config `rebase.autostash` is true (not for cherry-pick), it stashes silently. Otherwise it asks "The current working tree is not clean. Do you want to stash the changes?" (Stash/Abort). The stash runs `git.exe stash`.
  - At the end (`CheckRestoreStash`, `:998-1003`), autostash runs `StashPop(showChanges=0)`; otherwise it asks "Do you want to stash pop now?".
- Requires user.name/email.

Start (`:1040-1086`):
```
git.exe update-ref ORIG_HEAD <HEAD hash>
git.exe checkout -f <hash of (Onto or Upstream)> --
```
Per commit (`DoRebase`, `:2040-2320`):
```
git.exe cherry-pick [-x |--ff ][--allow-empty ][-m <n> ]<nocommit> <hash>
```
- `-x` comes from add "cherry picked from" (`m_bAddCherryPickedFrom`). `--ff` is used in rebase mode when committing normally (issue #1833).
- `--allow-empty` is added if the commit is empty. `-m <n>` is used only for cherry-picking a merge commit, where the user picks the parent.
- `<nocommit>` is `--no-commit` for squash/edit.

Other per-commit commands:
- Squash/edit commit: `git.exe commit [--allow-empty ]<author/date params>-F "<tmp>"` or `git.exe commit --amend [--allow-empty ]-F "<tmp>"` (`:1672,1681`).
  - The squash date comes from `SquashDate` (DWORD, default 0); value 2 means use the date of the first commit.
  - Squashed messages are concatenated with blank lines between them.
- Preserve-merges re-creates merges with `git.exe merge --no-ff[ --no-commit] -- <parents>` (`:2216`).
- Failed pick: Skip, Retry or Cancel, with "do same for rest". Skip runs `git.exe reset --hard`.

Finish (`:1155-1180`):
- If the branch is local: `git.exe checkout -f -B "<branch>" <newhead> --`
- Then: `git.exe reset --hard <newhead> --`

Abort (`:2613-2690`) restores the original state:
- `git.exe reset --hard <orig>`
- `git.exe checkout -f -B <branch> <origBranchHash> --`, or `git.exe branch -f -- <branch> <origBranchHash>` when the rebased branch was not the checked-out branch.
- Then restores the stash.
