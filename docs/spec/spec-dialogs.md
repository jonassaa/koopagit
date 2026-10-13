# TortoiseGit behavioural spec: launcher + core dialogs

Source: TortoiseGit upstream checkout (`src/…`, `doc/…`). All paths below are relative to `src/` unless they start with `doc/`.
Citation format is `file:line`. The `.rc` line numbers point at the English resource string or dialog template.

Abbreviations:
- **RC** = `Resources/TortoiseProcENG.rc`
- **SHELLRC** = `TortoiseShell/resourceshell.rc`
- **LLRC** = `Resources/TortoiseLoglistCommon.rc2`
- **WC_REF** = `GitRev::GetWorkingCopyRef()` = `GIT_REV_ZERO` (40 zeros) (`Git/GitRev.h:101`)
- **reg** = `HKCU\Software\TortoiseGit\…` (this is a settings key; a clone stores it in its own config file)

Window titles: most dialogs call `CAppUtils::SetWindowTitle(hwnd, path, dialogname)`, which builds
`<path compacted to (80 - len(dialogname)) chars> - <dialog caption> - TortoiseGit` (`TortoiseProc/AppUtils.cpp:3518-3565`).
If reg `DialogTitles`=1, the path is shown relative to the parent of the repository root (`AppUtils.cpp:3523-3533`).
Below, "title" means the dialog caption that goes into this pattern.

---

## 1. Launcher (`TortoiseGitProc`) CLI

### 1.1 Tokenizer (`Utils/CmdLineParser.cpp:35-180`)
- An option starts with `/` or `-` (`m_sDelims = L"-/"`, :24). Key and value are separated by `:` or by one or more spaces (`m_sValueSep = L" :"`, :26). So `/path:x`, `/path x` and `-path x` are all accepted. The docs list all three forms (`doc/source/en/TortoiseGit/tgit_app_automation.xml`).
- Keys are lower-cased, so lookup is case-insensitive (:68, :76, :181-186).
- Values may be quoted with `"`. Inside quotes, `""` is an escaped `"` (:120-135). Unquoted values end at the next space.
- An option with no value (e.g. `/noquestion`) has an empty value. Use `HasKey` for flags and `HasVal` for "present and non-empty".
- The map is a `std::map` and `insert` does not overwrite, so **if a key is repeated, the first occurrence wins** (:57 header; :72, :148, :160).
- `GetLongVal` uses `_wtol`: a missing or unparseable value gives 0 (:202-208).
- `EscapeValue` wraps a value in `"…"` and doubles any embedded `"` (:240-256). The launcher uses it whenever it builds a sub-command.

### 1.2 Startup sequence (`TortoiseProc/TortoiseProc.cpp:147-388`)
1. `/hwnd:<hex>`: the parent window (the file manager). The launcher walks up to the top-level window and ignores it if it is invalid (:149-157).
2. reg `Debug`=1: show the raw command line in a message box (:161-162).
3. `/urlhandler:<url>`: rewritten to `/command:clone /url:<url> /hasurlhandler`. Accepted prefixes are `tgit://clone/`, `github-windows://openRepo/`, `x-github-client://openRepo/` (query string stripped), `smartgit://cloneRepo/` and `git://`. Any other prefix shows `IDS_ERR_INVALIDPATH` (:186-213).
4. The command is looked up by the exact, case-sensitive `/command:` value (`Commands/Command.cpp:303-316`). An unknown or missing name falls back to `about`. A known name that is not compiled in shows "Command not implemented" (:446-448).
5. Commands that need neither git nor a path run now (:221-225).
6. If git.exe is missing, show `IDS_PROC_NOMSYSGIT` "No git.exe found.\n\nTortoiseGit requires a git.exe…" (RC:4893). The buttons are Set path / Go to website / Abort (:228-239). Then the git version is checked (:242).
7. **`/path` and `/pathfile` are mutually exclusive.** If both are given, show `IDS_ERR_INVALIDPATH` "The parameters '/path' and '/pathfile' are mutually exclusive.\nYou must only specify one of them." (RC:3570; TortoiseProc.cpp:258-262).
8. `/pathfile:<file>`: one path per line, blank lines skipped. The shell extension writes this file as UTF‑16LE (binary CStdioFile in a Unicode build) (`Git/TGitPath.cpp:1464-1493, 1495-1525`). `/deletepathfile` deletes the file after it has been read. `cmdLinePath` (the "primary path") stays **empty** when `/pathfile` is used (:268-283).
9. `/path:<p1>*<p2>*…`: an asterisk-separated list, each item passed through GetLongPathname (`TGitPath.cpp:1526-1537`). If `/path` is present with an empty value, show `IDS_ERR_INVALIDPATH` (:288-292). The primary path is the first item (:294-295).
10. With no paths at all, the path list becomes `[CWD]` (:298-300).
11. Repo discovery: `SetCurrentDir(primary, /submodule)` walks up to the worktree root. If that fails, each item in the list is tried in turn (:303-310). With `/submodule`, the walk stops at the submodule root. The process CWD is then set to the repo root (:312-313).
12. `CheckRepo` (`Commands/Command.cpp:452-488`), driven by each command's `PathRequirement` (`Command.h:24-31`; the default is `WorkingTreeRequired`, `Command.h:93`):
    - `WorkingTreeRequired` with no `.git` found: show `IDS_NOWORKINGCOPY` "No working directory found." (RC:4757).
    - `WorkingTreeOrBareRepoRequired` (log): show `IDS_NOGITREPO` "No valid git repository found." (RC:3991).
    - If libgit2 cannot open the repo, run the `inaccessible` command instead.
    - Requirements per command in this spec: log = WorkingTreeOrBare (`LogCommand.h:32`); diff and repocreate = PathRequired, which means no check (`DiffCommand.h:32`, `CreateRepositoryCommand.h:32`); every other command = WorkingTreeRequired.
13. `/groupuuid:<id>` groups taskbar icons. Otherwise the id is the SHA‑1 of the lower-cased worktree root (reg `GroupTaskbarIconsPerRepo`, default 3) (:266-267, :318-345).
14. Every git config file is parsed once. On a parse error, show the error with buttons Edit local config / Edit global config / Abort (:351-381).
15. `SetPaths(pathList, cmdLinePath)` (`Command.h:47-73`) does the following. `orgCmdLinePath`/`orgPathList` keep the absolute paths. `cmdLinePath`/`pathList` are rewritten **relative to the repo root** with the `\` separator. A path that is not under the root is kept absolute.
16. `Execute()`, then delete temp files older than a threshold (:386-392).

### 1.3 Global parameters
| Param | Meaning | Ref |
|---|---|---|
| `/command:<name>` | command name, see table below | Command.cpp:200-277 |
| `/path:<a*b*c>` | targets, separated by `*` | TortoiseProc.cpp:287 |
| `/pathfile:<file>` + `/deletepathfile` | targets read from a file, one per line | :268-279 |
| `/hwnd:<hex>` | parent window | :150 |
| `/closeonend:N` | progress-dialog auto-close, see 1.5 | GitProgressDlg.cpp:31-46, ProgressDlg.cpp:57-73 |
| `/submodule` | stop repo discovery at the submodule root | :303 |
| `/groupuuid:<s>` | taskbar grouping id | :267 |
| `/urlhandler:<url>` | URL protocol handler, becomes clone | :186 |
| `/notempfile` | obsolete and ignored (docs) | automation.xml |

### 1.4 Command names (`Commands/Command.cpp:200-277`)
about, add, autotexttest, blame, branch, cat, cleanup, clone, commit, conflicteditor, crash(*), diff, dropcopy(*), dropcopyadd, dropmove, fetch, formatpatch, export, help, ignore, importpatch, log, merge, pastecopy(*), pastemove(*), prevdiff, pull, push, rtfm, rebuildiconcache(*), remove, rebase, rename, repocreate, repostatus, resolve, revert, sendmail, settings, showcompare, switch, tag, unignore(*), updatecheck, stashsave, stashapply, stashpop, stashlist(*), subadd, subupdate, subsync, reflog, refbrowse, svndcommit, svnrebase, svnfetch, svnignore, sync, requestpull, bisect, repobrowser, revisiongraph, daemon, pgpfp, commitisonrefs, lfslocks, lfslock, lfsunlock, newworktree, worktreelist, dropnewworktree, registerwin11contextmenu, inaccessible, firststart.
(*) = the name is reserved but the command is compiled out (`#if 0`). It shows "Command not implemented".

### 1.5 `/closeonend` semantics
The value is read **directly from the process command line** by every progress dialog (`GitProgressDlg.cpp:31-46`, `ProgressDlg.cpp:57-73`). It overrides reg `AutoCloseGitProgress` (default 0):
- `0` (and any other value): `AUTOCLOSE_NO`. The dialog stays open until the user clicks OK/Close.
- `1`: `AUTOCLOSE_IF_NO_OPTIONS`. Close automatically on success **and** only when no post-action buttons are offered (`m_PostCmdList.empty()`).
- `2`: `AUTOCLOSE_IF_NO_ERRORS`. Close automatically on exit code 0.
- Values 3 and 4 appear in the docs but are commented out and not implemented (docs + enum `ProgressDlg.h:35-39`).
- The close is triggered by posting IDOK when the git status is 0 (`ProgressDlg.cpp:448-452`; `GitProgressDlg.cpp:310`).
- The commit dialog forces `AUTOCLOSE_IF_NO_ERRORS` when the split button is "ReCommit" or "Commit & Push" (`CommitDlg.cpp:902`).

### 1.6 Per-command parameters (automation.xml table, verified against the code)
| Command | Params |
|---|---|
| `commit` | `/logmsg:"…"` or `/logmsgfile:<file>` (both → `IDS_ERR_TWOLOGPARAMS` "Both /logmsg and /logmsgfile were specified!\nOnly one of those can be specified." RC:3768), `/bugid:"…"`, `/wholeproject` (CommitCommand.cpp:27-55) |
| `log` | `/rev:<sha>` highlight+scroll; `/startrev:<ref>` (alias `/revstart`), `/endrev:<ref>` (alias `/revend`) → range `start..end`, WC_REF treated as empty; `/range:<gitrev>` overrides; `/limit:"N SCALE"` SCALE∈commit/year/month/week (`/limit:0` = no limit; a bare N>0 = commits); `/findstring:`, `/findtext`, `/findregex`, `/findtype:N` (bitmask 1 msg, 2 path, 4 authors, 8 revs, 32 bugID, 64 subject; 0 = all); `/outfile:<file>` → dialog in single-select mode, selected SHA written on close (LogCommand.cpp:25-100) |
| `diff` | `/path`, `/path2`, `/startrev`, `/endrev`, `/unified`, `/line:NN`, `/alternative` (use the alternative diff tool), `/submodule` (DiffCommand.cpp:30-121) |
| `add` | `/noui` (no-op, the code is compiled out) (AddCommand.cpp:30) |
| `revert` | none |
| `remove` | `/keep` = remove from the index only (RemoveCommand.cpp:118) |
| `resolve` | `/noquestion` (skip the dialog), `/silent` (run `git add -f` directly, no progress dialog), `/resolvemsghwnd:<n>` (window to notify, `GITSLNM_NEEDSREFRESH`) (ResolveCommand.cpp:27-80). TortoiseGitMerge calls `/command:resolve /path:… /closeonend:1 /noquestion /skipcheck /silent` (TortoiseMerge/MainFrm.cpp:2273) |
| `cleanup` | none |
| `ignore` | `/onlymask` (ignore `*.ext`), `/delete` (also remove from the index/WT) (IgnoreCommand.cpp:31-38) |
| `repostatus` | none |
| `repocreate` | none (folder = `/path`, else repo dir, else CWD) |

---

## 2. Dialogs

### 2.1 Commit (`/command:commit`)
**Flow** (`CommitCommand.cpp:40-55` → `CAppUtils::Commit`, `AppUtils.cpp:3062-3133`):
1. `CheckUserData` (`AppUtils.cpp:3039-3060`): while user.name, user.email, committer name or committer email is empty, ask `IDS_PROC_NOUSERDATA` "User name and email must be set before commit.\nDo you want to set these now?" (RC:4841). Yes opens Settings at the `gitconfig` page. No aborts.
2. Show CCommitDlg with the bugid, `/wholeproject`, the message and the paths. reg `SelectFilesForCommit` (default TRUE) controls whether files are pre-checked.
3. On OK, run the post-command the user picked in the progress dialog: DCommit → svn dcommit dlg, Push → push dlg, CreateTag → create-tag dlg, Pull → pull dlg (`AppUtils.cpp:3099-3115`).

**Dialog template** `IDD_COMMITDLG` (RC:245-297). Caption **"Commit"**. Resizable. The title path is the repo root when whole-project is on, otherwise the single path or the common dir of the list (`CommitDlg.cpp:1551-1565`).
Controls in layout order:
1. `Commit to:` label + read-only edit showing the current branch (`g_Git.GetCurrentBranch()`, CommitDlg.cpp:1433). A hidden edit takes its place for a new branch name.
2. `new branch` checkbox. When checked it swaps the read-only branch edit for the new-branch edit and focuses it (:3026-3041). It is disabled while a merge is in progress (:476-482).
3. `Bug-ID/Issue-Nr:` label + edit + `{BugTraq}` button (provider link text). Label and edit are shown only if the project property `bugtraq.message` is set. The label text can be overridden by `bugtraq.label` (:300-319).
4. Group `&Message:` with a Scintilla message editor (spell-check, autocompletion, snippets, history in the context menu). The text info at the top right shows the cursor column.
5. `Amend &Last Commit` checkbox. Starts disabled and is enabled after status loads unless this is an initial repo or a cherry-pick is in progress (:1468-1479).
6. `Show diff to last commit` checkbox. Hidden until amend is checked. Disabled when HEAD has no parent (then forced TRUE) (:1488-1497).
7. `Set author &date` checkbox + date picker + time picker. The pickers are hidden until checked. Allowed range is 1970-01-01..2099-12-31 (:220-222). Initial value is now, or HEAD's author date when amending (:2986-3018).
8. `Reset` checkbox (`IDC_COMMIT_AS_COMMIT_DATE`). Visible only when amend **and** set-date are both on. Tooltip "Use the current time by setting the author date to the commit date." (RC:4449). When checked it disables the pickers and emits `--date=now`.
9. `Set au&thor` checkbox + author edit. The edit is read-only unless checked. Default text is `Name <email>` from config, or HEAD's author when amending (:3081-3097).
10. `Add &Signed-off-by` button. Appends `Signed-off-by: <user.name> <user.email>` (newlines stripped from both) if the line is not already present. The message is trimmed on the right first. A blank line is inserted before the trailer unless the last line already contains `-by: `. The result ends with `\r\n` (:2715-2745).
11. Splitter.
12. Group `Changes made (F5: refresh, double-click on file for diff):`.
13. `Check:` followed by link-labels `&All` `&None` `Unversioned` `Versioned` `Added` `Deleted` `Modified` `Files` `Submodules`. Clicking one checks the matching rows. Shift+click unchecks them instead (:2504-2528). Each link is enabled only if it has matching rows (:3068-3079).
14. File list (GitStatusListCtrl, see §3). Default visible columns: Path, Extension, Status, Lines added, Lines removed (:244).
15. `Staging support (EXPERIMENTAL)` checkbox. Persisted in git config `tgit.commitstagingsupport` (:445-450, 3099-3138). When on, rows get tri-state checkboxes and the "Partial Staging>>" / "Partial Unstaging>>" links replace "View Patch>>".
16. `Show &Unversioned Files` checkbox. Default from reg `AddBeforeCommit` (TRUE) (:207-208).
17. Statistics label `%1!d! files selected, %2!d! files total` (RC:4180).
18. `Do not autoselect submodules` checkbox. reg `DoNotAutoselectSubmodules`, default FALSE (:217-218).
19. `View Patch>>` link. Opens a patch side-pane. It auto-opens if git config `tgit.commitshowpatch` is true (:451-457).
20. `Show &Whole Project` checkbox. Per-repo reg `TortoiseProc\ShowWholeProject\<repo>`, default FALSE (:210-213). It is forced on (`m_bWholeProject2`) when the path list is empty or is the root, and is then disabled (:224-228, :1460).
21. `Message onl&y` checkbox. Disables the file list and commits with `--allow-empty` (:2791-2796). Tooltip RC:4125.
22. Merge-active icon (shown during a merge).
23. **Split button `C&ommit`** (IDOK, disabled initially). Entries `C&ommit` / `&ReCommit` / `Commit && &Push` (RC:5130-5132). The last-used entry is remembered in reg `CommitLastAction` (:491-507, :923-924). Then `Cancel` and `Help`.

**Initial message** (:195-205): if no `/logmsg` was given, load the template from git config `commit.template` (a `~/` prefix is expanded, `/d/…` becomes `d:/…`) (:156-188). Then `.git/SQUASH_MSG` and `.git/MERGE_MSG` are loaded if they exist; each overwrites the previous one. A **start-commit hook** runs. If it fails, a task dialog appears and the dialog may abort (:3140-3162).

**OK-button enable rule** (:2530-2542): the trimmed message must be non-empty and its length ≥ `bugtraq.logminsize`, **and** at least one of the following holds: message-only is on, at least one file is checked, amend with diff-to-last-commit is on, or a merge is active.

**File list population** (StatusThread, :1356-1549):
- `git update-index --refresh` equivalent (`RefreshGitIndex`).
- Status from `GetWorkingTreeChanges` (§3.3) with `includedStaged=true`, so staged files outside the selected paths are listed too.
- Amend mode diffs against `HEAD~1` instead of HEAD, unless "Show diff to last commit" is on.
- If "Show unversioned" is on, unversioned files are added.
- Show mask: versioned-but-not-normal + in-changelist + direct files (+ unversioned).
- Rows are pre-checked if `SelectFilesForCommit`, but **unversioned files are never pre-checked**.
- If the list ends up empty but unversioned files exist and no merge is active, ask `IDS_COMMITDLG_NOTHINGTOCOMMITUNVERSIONED` "No files were changed or added since\nthe last commit.\nDo you want to see the unversioned files?" (RC:4189).
- Empty-list text: `IDS_COMMITDLG_NOTHINGTOCOMMIT` "No files were changed or added since\nthe last commit." (RC:3638).
- During a merge, a message box with the `IDS_COMMIT_MERGE_HINT` text and a "do not show again" checkbox is shown (:476-482).

**Amend toggle** (:2747-2789): on check, if the stored amend text is empty, load HEAD's `subject\nbody`. The current text is saved as "NoAmendStr" and the amend text is put in the editor. Unchecking swaps them back. The author and date fields are re-derived from HEAD.

**OnOK validation, in order** (:569-695):
1. If any file is conflicted, ask `IDS_PROGRS_CONFLICTSOCCURRED` "One or more files are in a conflicted state." with buttons OK / Ignore (+ do-not-ask). OK selects the first unmerged row and aborts the commit.
2. If new branch is checked:
   - Invalid name → balloon `IDS_B_T_NOTEMPTY` "Branch/Tag name must not be empty or is invalid."
   - A branch with that name exists → "A branch with this name already exists. Delete the existing branch or use a different name."
   - A tag with that name exists → `IDS_B_SAMETAGNAMEEXISTS` with buttons Continue / Abort.
3. Bug-ID fails the `bugtraq.number`/regex check → balloon "Only numbers (optionally separated by commas)\nare allowed!" (RC:4181).
4. Message empty → `IDS_COMMITDLG_NOMESSAGE` "You must enter a log message for the commit" (RC:4196).
5. `bugtraq.warnifnoissue` is set and there is no issue → Yes/No "You haven't entered an issue number!\nAre you sure you want to commit without an issue number?"
6. The message still equals the template → "You did not edit the message template.\nProceed without individual commit message?" with buttons Proceed / No (+ remember).
7. Project property `tgit.warnnosignedoffby` is set and the sign-off line is missing → "You haven't entered your Signed-Off-By line!" with buttons `&Add it` / `&Commit w/o` / Abort.
8. `MessageContainsConflictHints`: warn if the message still contains `Conflicts:` lines.
9. Pre-commit hook.
10. For each checked **directory** entry (a submodule): if it is unversioned, run `git status --porcelain` inside it. Otherwise run `git diff -- <path>` and check whether the output ends with `-dirty\n`. If the submodule is dirty, ask `IDS_COMMITDLG_SUBMODULEDIRTY` with buttons Commit (opens a commit dialog for the submodule) / Ignore / Cancel (:697-743).

**Index preparation when staging support is off** (`PrepareIndexForCommitWithoutStagingSupport`, :1100-1329). A progress dialog "Preparing commit..." / "Updating index" is shown when there are ≥25 rows. There are two paths, libgit2 and CLI. The CLI path (:1247-1316) builds these "massive" tasks. When the operation is add/rm/reset/checkout, each task runs as `git <args> --pathspec-from-file=<tmp> --pathspec-file-nul`; otherwise it is batched argv `git <args> -- <files>` (`Git/MassiveGitTaskBase.cpp:87-160`).
- checked + (unversioned, or a directory that is not deleted) → `git add -f`
- checked + deleted → `git update-index --force-remove`
- checked + other → `git update-index`
- checked + renamed (has an old path) → `git rm --ignore-unmatch <old>`
- unchecked + added/renamed → `git rm -f --cache <path>`, and re-add after the commit with `git add --ignore-errors -f`. If renamed: `git reset [HEAD~1] <old>`, and re-delete after with `git rm --cached --ignore-unmatch`.
- unchecked + other versioned → `git reset [HEAD~1] -- <path>` (`HEAD~1` when amending without diff-to-last). A deleted-but-present file is re-deleted from the index after the commit.
- Task order: add, update-index --force-remove, update-index, rm, rm --cache, reset (:1300).
- After the commit, the re-add and re-delete run if reg `ReaddUnselectedAddedFilesAfterCommit` is TRUE (the default) (:965-970).
- When staging support is on, the index is used exactly as it is.

**New branch**: `git branch -- <name>`, then `git checkout --end-of-options <name> --` (:775-793).
**Detached HEAD** (reg-gated `m_bWarnDetachedHead`): ask Yes/No/Cancel "Current HEAD is detached, you are working on (no branch).\nDo you want to create a branch now?" (:2957-2972).

**Commit command** (:836-896). The message is written to a temp file (UTF‑8). Then:
```
git commit [--author=<quoted "Name <mail>">] [--date=YYYY-MM-DDTHH:MM:SS | --date=now] [--amend] [--allow-empty] -F <tmpfile>
```
- `--allow-empty` is added only for Message-only.
- `--date=now` is used when amend + "Reset" are both on.
- A date before the epoch gives the error "Invalid time".
- The command is skipped, with "Nothing to commit" / "Commit Finish" (RC:4722-4723), unless message-only is on, or something is checked, or staging is on, or amend is on, or a merge is active (:829, :1063-1067).
- It runs in the "Git Command Progress" dialog with the command text hidden.

**Post-commit progress buttons** (:877-889): when successful and post-actions are allowed, offer:
- `SVN DCommit...` (only for git-svn repos)
- `Pus&h...`
- `&Pull...`
- `&ReCommit`
- `Create &Tag...`

**After the commit**:
- Run "restore after commit" files. The user is asked "You marked some files as "Restore after commit"…" with buttons `&Restore old state` / `&Keep current state` (:3043-3066).
- Run the post-commit hook.
- Prune changelists.
- Push the message to the history (reg `MaxHistoryItems`=25).

ReCommit, or a failed commit (:990-1047), keeps the dialog open. On success it also resets the message to the template and clears amend, set-date and set-author.

Commit & Push (:1079-1082, :531-545):
- With an upstream configured and not amending: push directly to `remote/remotebranch`.
- Otherwise (amend, or no upstream): open the Push dialog.

**Cancel** (:1567-1615): if the message changed or the list is non-empty, ask "Are you sure you want to cancel?\n\nHint: Recently used commit messages are available using the context menu of the text field." Yes/No (+ do-not-ask). A changed message is saved to the history.
**Keyboard**: Ctrl+Enter = OK (doc dug_commit.xml), F5 = refresh, Alt+<accel> picks a split-button entry (:508-529).

### 2.2 Log (`/command:log`)
**Template** `IDD_LOGMESSAGE` (RC:210-243). Caption **"Log Messages"**. Resizable, app-window style. The title path is the original path, or the repo root when whole-project is on (`LogDlg.cpp:610-619`).
Controls in layout order:
1. Ref label (`IDC_STATIC_REF`, initial text "Branch"). Shows the current ref (`git symbolic-ref HEAD` stripped, or `<No branch>`), or `<All Branches>` / `<Basic Refs>` / `<Local Branches>` (RC:3576, 5126, 5033, 3577) (:3291-3335).
   - Left-click opens the Browse References picker.
   - Right-click menu: `Browse References` / `HEAD -> "<branch>"` / `FETCH_HEAD` / `All` / `All basic refs` / `All local branches`, then the ref history (reg `MaxRefHistoryItems`=5) (:1368-1453).
2. `From:` date picker and `To:` date picker. Right-click on From gives `No limitation` / `Last %s commit(s)|year(s)|month(s)|week(s)` / `Configure default` (:1455-1510).
3. Jump text edit + jump-type combo: Author Email, Committer Email, Merge Point, Parent 1, Parent 2, Tag, Tag (FF), Branch, Branch (FF), Selection History (:420-431). Then the ▲ / ▼ jump buttons.
4. The revision list (owner-data listview, §2.2.1).
5. Splitter. The message view (RichEdit) shows the full message, notes and links, with the gravatar to its right.
6. Splitter. The changed-files list (GitStatusListCtrl): Path, Extension, Status, Lines added, Lines removed (:314). It has its own "Filter paths" filter edit (:466-470).
7. Info line `Showing %1!ld! revision(s), from revision %2!s! to revision %3!s! - %4!ld! revision(s) selected, %5!ld! file(s) selected` (RC:3575).
8. `Show &Whole Project` checkbox. Same per-repo reg key as the commit dialog (:65-66, :3543-3562).
9. `&All Branches` **3-state** checkbox. Per-repo reg `LogDialog\AllBranch\<repo>`, values None / AllBranches / AllLocalBranches / AllBasicRefs. Indeterminate means local or basic refs (:51-63). Clicking toggles only between unchecked and checked (`--all`) (:3181-3209). Any explicit range other than HEAD forces it off (:233-237).
10. Filter edit. Its cue text is "Filter by <selected fields>" (:2341-2410). The info-icon menu offers, in order: Subject, Messages, Paths, Authors, Emails, SHA-1, Refname, Tag Info, Notes, [Bug-IDs], separator, Toggle filters, All, separator, Use regular expression, Case-sensitive. The selection persists in reg `SelectedLogFilters`, `UseRegexFilter` and `FilterCaseSensitively` (:2204-2310). The filter is applied after a 1s timer.
11. `Help`.
12. `Refresh`, `S&tatistics` (disabled until loaded), `Walk Be&havior` (menu button), `&View` (menu button), `OK`, `Cancel` + a hidden progress bar.
   - In view mode (no `/outfile`), OK is hidden and **Cancel is relabelled "OK"** (:440-448).

**Walk Behavior menu** (:3359-3416):
- `First Parent` (`--first-parent`)
- `No &merges` (`--no-merges`)
- `&Follow renames` (`--follow`; enabled only for a single file)
- `Full history` (`--full-history`, otherwise `--parents`)
- separator
- `&Compressed Graph` (`--simplify-by-decoration`-like)
- `Show la&beled commits only`

The button shows as pressed while any of these is active.

**View menu** (:3418-3524):
- `H&ide Unrelated Changed Paths`, `&Gray Unrelated Changed Paths` (gray is the default, :291)
- `Show &Unversioned Files`
- submenu `&Labels` → `&Tags`, `&Local branches`, `&Remote branches`, `Other refs` (each per-repo reg, default TRUE)
- `G&ravatar` (reg `EnableGravatar`, default FALSE)
- `View Patch` (auto-on with git config `tgit.logshowpatch`)

**Defaults**:
- The selection starts on `/rev` or HEAD (:452-461).
- The first row is a pseudo-commit "Working tree changes" (LLRC) when reg `LogIncludeWorkingTreeChanges` (TRUE) is on and the repo is not bare (:300).
- Limit: reg `LogDialog\NumberOfLogsScale` (default `SHOW_NO_LIMIT`), `LogDialog\NumberOfLogs` (default 1) (`GitLogListBase.cpp:65, 78`).
- Order: reg `LogOrderBy`, default `--topo-order` (`GitLogListBase.cpp:2677`).
- Double-click on a revision does nothing unless reg `DiffByDoubleClickInLog` is set; then it runs Compare with previous (`GitLogListBase.cpp:2760-2766`).

**Git log arguments** (`Git/Git.cpp:1028-1150`, executed via gitdll/libgit, not git.exe):
`-z [--numstat] [--raw] [--all | --branches --tags --remotes --glob=stas[h] --glob=bisect | --branches] [-C<n>%] [-M<n>%] [--first-parent] [--no-merges] (--full-history | --parents) [--follow] [-c] [--full-diff] [--simplify-by-decoration] [-n<N> | --max-age=<epoch>] [--min-age=<epoch>] (--topo-order|--date-order|--author-date-order) --end-of-options <range> -- <path>`
- All-branch modes empty the range.
- The similarity threshold comes from reg `DiffSimilarityIndexThreshold`, default 50 (Git.cpp:47).
- Year = 365 days, month = 30 days, week = 7 days, counted from today 00:00 (:1104-1123).

#### 2.2.1 Revision list columns (`TortoiseProc/GitLogListBase.cpp:321-336`; labels LLRC:19-41)
| Column | Label | Visible by default | Notes |
|---|---|---|---|
| Graph | `Graph` | yes | |
| (rebase) | `REBASE` | rebase dialog only | |
| ID | `ID` | only in ID-replace mode | right-aligned |
| Hash | `SHA-1` | no | |
| Actions | `Actions` | yes | icons for A/M/D/R/conflict |
| Message | `Message` | yes | subject + ref labels |
| Author | `Author` | yes | |
| Date | `Date` | yes | author date; relative dates if reg set |
| Email | `Email` | no | |
| Commit Name | `Commit Name` | no | |
| Commit Email | `Commit Email` | no | |
| Commit Date | `Commit Date` | no | |
| Bug-ID | `Bug-ID` | yes, only if bugtraq is configured | |
| SVN Rev | `SVN Rev` | yes, only if git-svn | |

Column visibility, order and widths are saved per list (`ColumnManager`, version 6, :340-341).

#### 2.2.2 Commit context menu (`GitLogListBase.cpp:1640-2360`), in order, each item gated by its condition

**Single selection:**
- Working-tree row only: `Compare with &working tree`, or for the pseudo-row: `C&ommit...` and `Abort &Merge` (when a merge is active).
- `Blame previous revision` (with a parent submenu for merges).
- `Show changes as &unified diff`. For merges this becomes `&Unified diff with` with submenu `All Parents` / `Only Merged Files` / `Show extra changes after merge` / `Parent N`.
- `&Compare with previous revision` (parent submenu for merges).
- `Blame`.
- Working-tree row only: `Stash changes`, `Stash Pop`, `Stash List`.
- Bisect: `Bisect good` / `Bisect bad` / `Bisect skip`, `Bisect reset`.
- Working-tree row only: `&Pull...`, `F&etch...`, `Submodule Update`, `&Clean up...`.
- `Show log...`, `&Browse repository`.
- `&Merge to "<branch>"...`.
- `R&eset "<branch>" to this...`.
- `&Switch/Checkout to` (branch submenu).
- `Sw&itch/Checkout to this...`.
- `Create Br&anch at this version...`, `Create &Tag at this version...`.
- `Rebase "<branch>" onto this...(&G)`.
- `E&xport this version...`.
- `&Revert change by this commit` (parent submenu for merges).
- `Edit &Notes`.

**Two or more selected:**
- `&Compare revisions` (when 2 are selected or the range is contiguous).
- `Show changes as &unified diff` (2 selected).
- `Show log of A..B`, `Show log of B..A`, `Show log of A...B`.
- `Comp&are change sets`.
- `&Revert changes by these commits`.
- `Bisect skip`.

**Then:**
- `Com&bine to one commit` (contiguous range ending at HEAD).
- `Cherr&y Pick this commit...` / `Cherr&y Pick selected commits...`.
- `&Format Patch...`.
- `Bisect start` (2 selected).

**Single selection, continued:**
- `Pus&h "<ref>"...`, `SVN DCommit...`.
- `&Pull...` (on HEAD).
- `&Delete branch/tag` (with a submenu plus `All` when there are several refs).
- `Collapse revisions` / `Expand collapsed revisions`.

**Always:**
- Submenu `Copy to clipboard`: Full data / Full commit data without changed paths / SHA-1 / Authors (full) / Author names / Author email addresses / Subjects / Messages / [Tag/branch names].
- `Search &log messages...`.
- `Show branches this commit is &on`.

The message view's own context menu: `Cop&y`, `Copy all information to clipboard`, `Edit &Notes` (`LogDlg.cpp:1512-1590`).

### 2.3 Diff (`/command:diff`) (`Commands/DiffCommand.cpp:30-121`)
No dialog of its own. The behaviour depends on the arguments:
- **No `/path2`.** Requires a worktree or bare repo.
  - Path is a directory (and no `/submodule`): open the **Check for modifications** dialog (§2.10) for that dir (:40-46).
  - `/startrev` + `/endrev`: diff `startrev` against `endrev` with the external tool, or as a unified diff with `/unified` (:52-58).
  - Otherwise, compare the working tree file with HEAD (:101-105). If the file is index-new (`GIT_STATUS_INDEX_NEW`), the code tries rename detection first: `git diff-index --raw HEAD -M<n>% -C<n>% -z --`. If an old name is found, it diffs the WT file against the old path at HEAD. If not, it diffs against an empty file (`DiffNull`) (:60-99).
- **With `/path2`.** If both revs are given and path2 is inside the repo, it is a rev diff between the two repo paths. Otherwise the two files on disk go straight to the external diff tool (:108-120).
- `CGitDiff::Diff` (`TortoiseProc/GitDiff.cpp:361+`) works as follows:
  - Revs are resolved with `<rev>^{}`.
  - The blob is extracted to a temp file with `GetOneFile`.
  - Titles are `<path>: <shorthash>`, or `%s: Working Tree` (RC:4114) for the working tree.
  - Errors: `Failed to checkout file "%1!s!" of revision %2!s! to "%3!s!".` (RC:4062). A missing working tree file asks `The file "%s" does not exist in your working tree.\nDo you want to select another file to diff?` (RC:3828).
  - Submodule paths use the submodule diff dialog instead.

### 2.4 Add (`/command:add`) (`Commands/AddCommand.cpp:27-79`)
- If **all** paths are files, skip the dialog and add them directly through the progress dialog.
- Otherwise show `IDD_ADD` (RC:174-185). Caption **"Add"**, title path = common root (`AddDlg.cpp:72`). Controls:
  - File list. Default columns are Path + Extension; Extension, Last Modified and Size are allowed (`AddDlg.cpp:63`).
  - `Select/deselect &all` (tri-state).
  - `Include &ignored files`.
  - `OK` / `Cancel` / `Help`.
- The list shows unversioned files plus direct files plus removed-and-present files. Ignored files are listed only when the checkbox is on. Unversioned rows are pre-checked (`AddDlg.cpp:148-153`).
- Empty text: "There is nothing to add.\nAll the files and folders are either under version control,\nhave been ignored or the global ignore configuration setting." (RC:4087).
- OK writes the checked paths (`AddDlg.cpp:100-109`).
- Git operation (`ProgressCommands/AddProgressCommand.cpp`):
  - libgit2: `git_index_add_bypath` per path, then write the index.
  - CLI: `git add -f --pathspec-from-file=… --pathspec-file-nul`.
  - Each row is reported as `Added`.
- Progress title `Add`.
- Post-buttons on success:
  - `&Commit...`, which launches `/command:commit /path:"<repo>"`. It appears only when the "show commit button after add" flag is set.
  - `Add as Executable (+x)` (index mode 100755)
  - `Add as Symlink` (mode 120000) (RC:3984, 3989).

### 2.5 Revert (`/command:revert`) (`Commands/RevertCommand.cpp:26-43`)
**Dialog** `IDD_REVERT` (RC:568-579). Caption **"Revert"**, title path = common root. Controls:
- File list. Columns: Path, Extension, Status, Lines added, Lines removed.
- `Select/deselect &all`.
- Hidden note `Note: the folder contains unversioned items`. It is shown only if reg `UnversionedAsModified` is set and unversioned items exist.
- `OK` / `Cancel` / `Help`.

**List contents** (`RevertDlg.cpp:115-140`): first `RefreshGitIndex`, then versioned non-normal entries are shown. Only directly selected files and added files are pre-checked. Empty text "No files or folders were modified. There is nothing for TortoiseGit to revert!" (RC:4091).

**Operation** (`ProgressCommands/RevertProgressCommand.cpp`), with revision = `HEAD` by default:
1. If reg `RevertWithRecycleBin` (TRUE), move every selected path except added ones to the recycle bin first. Renamed files are excluded too, unless reg `RevertRenamedFilesWithRecycleBin` (TRUE).
2. Added files: `git rm -f --cached`.
3. Renamed file whose new path still exists: `git rm --ignore-unmatch <new>` and `git checkout -f --end-of-options HEAD -- <old>`.
4. Renamed file whose new path is gone: `git rm -f --cached <new>` + checkout of the new path.
5. Renamed submodule: `git mv [-f] -- <new> <old>`, then a checkout.
6. Everything else: `git checkout -f --end-of-options HEAD -- <paths>`.
7. Deleted files are additionally run through `git add -f`.
8. The order is unstage, delete, checkout, add.
9. If an old name is now a directory, the error is "Revert failed:\nCannot revert renaming of "%s". A directory with the old name "%s" exists."
10. Progress title `Revert`, rows `Reverted`. If submodules were involved, a post-button `Handle submodules` (RC:4014) opens a diff for each one.

### 2.6 Remove (`/command:remove`) (`Commands/RemoveCommand.cpp:27-167`)
No dialog, only a confirmation box with buttons `&Remove` / `&Abort` (RC:5100, 3512):
- One path: `Do you really want to remove "%s"?` (RC:4161). With `/keep`: `Do you really want to remove "%s" from the index?` (RC:5098).
- Several paths: `Do you really want to remove the %d selected files/directories?` (RC:4461). With `/keep`: `…from the index?` (RC:4462).

Then, **one git call per path**: `git rm -r -f -- <p>`, or with `/keep` `git rm -r -f --cached -- <p>` (:127-131). On error the git output is shown with buttons `&Ignore` / `A&bort`. At the end, an info box shows `%d files removed.` (RC:4761) with the count of paths that succeeded.

### 2.7 Resolve (`/command:resolve`) (`Commands/ResolveCommand.cpp:27-81`)
**Dialog** `IDD_RESOLVE` (RC:800-811). Caption **"Resolve"**, title path = repo root. Controls:
- File list (conflicted only, all checked) (`ResolveDlg.cpp:143-145`).
- `Select/deselect &all`.
- Static `Reminder: Commit your change after resolve`.
- `OK` / `Cancel` / `Help`.

`/noquestion` skips the dialog and uses all given paths.
- **Silent** (`/silent`): run `git add -f -- <p>` per path and delete the temp merge files (`.BASE/.LOCAL/.REMOTE`…). Then post `GITSLNM_NEEDSREFRESH` to `/resolvemsghwnd` if reg `RefreshFileListAfterResolvingConflict` (TRUE).
- **Otherwise**: the progress dialog runs `ResolveProgressCommand(Current)`. Paths that are not unmerged are reported `Skipped`. The rest go through `git add -f` (massive, pathspec-from-file) and are reported `Resolved`. Progress title `Resolve`. Post-button `&Commit...` (`ResolveProgressCommand.cpp`). Info text "You need commit your change after resolve conflict".
- Resolve-with-theirs/mine (used from the status-list context menu) works like this:
  - `git checkout-index -f --stage=3` (theirs) or `--stage=2` (mine), then `git add -f`.
  - If the chosen side was deleted: `git rm -f`.
  - For submodules: `git update-index --replace --cacheinfo 0160000,<sha>,<path>`, or a reset inside the submodule.

### 2.8 Cleanup (`/command:cleanup`) (`Commands/CleanupCommand.cpp:280-292`)
**Dialog** `IDD_CLEAN` (RC:1713-1735). Caption **"Clean"**, fixed size. Title path = the single path or the common root. Controls:
- Group `Clean Type` with radios:
  - `Remove &all untracked files (-fx)`
  - `Remove &non-ignored untracked files (-f)`
  - `Remove ignored files (-fX)`
  - Default comes from per-repo reg `History\CleanType\<repo>` = 0, i.e. -fx.
- `Remove untracked directories (-d)`. Per-repo reg `History\CleanDir\<repo>`, default 1 (checked).
- `Remove unmanaged directories with .git folder (-f)`. Indented, enabled only when -d is on, default unchecked.
- `Do not use recycle bin`. Default is `!reg RevertWithRecycleBin`, i.e. unchecked.
- `Dry run`. Unchecked.
- `Submodules`. Unchecked.
- Static `Attention: This command affects the whole working tree!`.
- `OK` / `Cancel` / `Help`.
- Type and dir are saved on OK (`CleanTypeDlg.cpp:31-104`).

**Command** (`CleanupCommand.cpp:134-160`): `git clean [-n] [-d] (-fx|-f|-fX) [-f] [-- <dir>]`.
- `-n` is added for a dry run, **or whenever the recycle bin is used**. In that case TortoiseGit takes the "Would remove …" list (quotepath-aware) and recycles the files itself (:68-100, :229-275).
- The pathspec is the selected directory, or the containing directory of a selected file. The WC root passes no pathspec.
- Submodules: the same command is run in every initialised submodule below the selection, recursively, using libgit2 `git_submodule_foreach`.
- Dry run / no recycle bin: runs in "Git Command Progress".
- Post-buttons:
  - On failure: `&Retry`.
  - After a dry run: `Clean to recycle bin` and `Clean permanently` (RC:5074-5075). The order depends on the no-recycle-bin option.
- Recycle-bin path: a modal "Cleaning up." progress, then the files are deleted (to the recycle bin).

### 2.9 Ignore (`/command:ignore`) (`Commands/IgnoreCommand.cpp`, `AppUtils.cpp:1424-1546`)
**Dialog** `IDD_IGNORE` (RC:1870-1890). Caption **"Ignore"**. Controls:
- Group `Ignore Type`:
  - `Ignore item(s) only in the containing folder(s)` (default)
  - `Ignore item(s) recursively`
- Group `Ignore File`:
  - `.gitignore in the repository root`
  - `.gitignore in the containing directories of the items`
  - `.git/info/exclude`
- `OK` / `Cancel` / `Help`.
- Effective defaults: type = only-in-folder, file = **repository-root .gitignore**. The constructor sets `m_IgnoreFile=1`, but `OnInitDialog` checks the file radio using `m_IgnoreType` (0). This is an upstream quirk (`IgnoreDlg.cpp:32-33, 56`).

**Pattern construction** (AppUtils.cpp:1486-1510):
- Prefix (only for the "containing folder" type): `/<dir-relative-to-root>/` for the root .gitignore or exclude, and `/` for a per-directory .gitignore.
- Body: `*<ext>` with `/onlymask` (files without an extension are skipped), otherwise the file or dir name.
- `[` and `]` are escaped with a backslash.
- The pattern is appended only if it is not already present.
- The file's existing EOL style is kept. A newline is added first if the file does not end with one.
- If the file cannot be opened: `<file> Open Failure`.
- `.git/info` is created if it is missing.

**`/delete`**: ask Yes/No "Keep file locally?". Yes runs `git rm --cache -r -f -- <p>`, No runs `git rm -r -f -- <p>`, one call per path. On error the output is shown with OK/Cancel. At the end: `%d files removed.`.

### 2.10 Check for modifications (`/command:repostatus`) (`Commands/RepoStatusCommand.cpp`, `ChangedDlg.cpp`)
**Template** `IDD_CHANGEDFILES` (RC:187-208). Caption **"Working Tree"**. Resizable. The title is the repo root (whole project), otherwise the path or common dir (:250-260). Controls:
1. Branch hyperlink (`<a>branch</a>`). Clicking it opens the log.
2. File list. Default columns: Path, Extension, Status, Lines added, Lines removed, Last Modified (:101).
3. `Show un&versioned files`. reg `AddBeforeCommit`, default TRUE.
4. `Show ignore local changes flagged files` (assume-unchanged / skip-worktree). Default off.
5. `Show i&gnored files`. Default off.
6. `Show un&modified files`. Hidden.
7. `Show all &staged files`. reg `TortoiseProc\ChangedFilesIncludeStaged`, default TRUE.
8. `Show &Whole Project`. Per-repo reg.
9. Info/summary labels.
10. Buttons:
    - `Save unified diff`: launches `/command:showcompare /unified /path:<commondir> /revision1:HEAD /revision2:<WC_REF>`.
    - `Stash` menu button: Stash changes / Stash Pop / Stash Apply / Stash List.
    - `Commit`: opens the commit dialog with the paths, then refreshes.
    - `&Refresh`.
    - `OK`. Labelled "Cancel" while loading.

The unversioned and ignored checkboxes are enabled only when a single directory is targeted. Empty text: "No files to show with the current setting.\nCheck one or more settings below to see unversioned, ignored and/or unmodified files." (RC:4691).
Status source: same as §3.3, plus ignored files (`ls-files … --others -i`) when requested.

### 2.11 Create repository (`/command:repocreate`) (`Commands/CreateRepositoryCommand.cpp:45-93`)
1. Folder = `/path`, else the repo dir, else CWD.
2. If the folder is a drive root, a UNC server, Desktop, Profile, Documents, Windows, System or Program Files: warn `The target folder \n%s\nis a special folder and is not supposed to be a repository root!\nAre you sure you want to initialize a git repository inside that folder?` with buttons `A&bort` / `&Proceed` (RC:5042) (:28-58). This is the place where a port substitutes platform-equivalent folders.
3. **Dialog** `IDD_CREATEREPO` (RC:1771-1782). Caption **"Git Init"**, title = folder. Controls:
   - `Make it Bare (No working directories)` checkbox. Auto-checked when the folder name ends with `.git` (`CreateRepoDlg.cpp:56-62`).
   - Description: "If you plan to work inside this folder, leave this unchecked. Typically a bare repo can only have changes 'pushed' to it. (By convention, a bare repo folder should have a name that ends with '.git')".
   - `OK` / `Cancel` / `Help`.
4. If bare is checked and the folder exists and is not empty: warn `The target folder \n%s\nis not empty!\nAre you sure you want to initialize…` with Abort/Proceed (RC:3858).
5. `git_repository_init_ext` with flags `MKPATH | EXTERNAL_TEMPLATE` (+ `BARE`). `GIT_TEMPLATE_DIR` is honoured. The CLI equivalent is `git init [--bare] <folder>`.
6. On error: libgit2 message "Could not initialize a new repository.".
7. On success: an info box `Initialized empty Git repository in %s.` (RC:4971). A bare repo gets a folder icon (desktop.ini) on Windows.

### 2.12 Progress dialogs (shared)
- `IDD_SVNPROGRESS` (RC:465-478). Caption "Progress". Used by add, revert and resolve.
  - Contents: an `Action | Path` list (RC:3641-3642), an info text, a progress bar, `Show &log...` (hidden), `OK`, `Cancel`.
  - The title becomes `<title> Finished!` or `<title> Failed!` (`GitProgressList.cpp:464-470`).
  - Final line: `%1!s! in %2!s!`, using `%1!ld! minute(s) and %2!ld! second(s)`.
  - Action words: `Added`, `Reverted`, `Resolved`, `Skipped` (RC:3521, 3525, 3555, 3564).
- `IDD_GITPROGRESS` (RC:1139-1152). Caption **"Git Command Progress"**. Used for commit and clean.
  - Controls: an animation, a current-line label, a progress bar, a RichEdit log of git output, a post-action menu button at the bottom left (`IDC_PROGRESS_BUTTON1`), `Close` and `Abort`.

---

## 3. File status list control (`Git/GitStatusListCtrl.*`)

### 3.1 Columns (`GitStatusListCtrl.cpp:302-312`; labels RC)
| # | Column | Label | Cell text (:1030-1083) |
|---|---|---|---|
| 0 | Name | `Path` (RC:4256) | repo-relative path with `/`. Renames and copies show `new (from %s)` (RC:4914), or the abbreviated form `pfx{old => new}sfx` (`TGitPath.cpp:1823-1881`) |
| 1 | Filename | `Filename` (RC:4268) | the last component |
| 2 | Extension | `Extension` (RC:4259) | `.ext` |
| 3 | Status | `Status` (RC:4257) | `GetActionName()`, see 3.2 |
| 4 | Add | `Lines added` (RC:4275) | from `--numstat` |
| 5 | Del | `Lines removed` (RC:4276) | from `--numstat` |
| 6 | LastModificationDate | `Last Modified` (RC:4941) | short date+time. Empty for deleted entries |
| 7 | FileSize | `Size` (RC:4942) | `StrFormatByteSize64`. Empty for dirs |
| 8 | LFSLock | `LFS Lock Owner` (RC:4269) | only when the repo uses LFS |

Path is always visible. Each dialog passes default-visible (`dwColumns`) and allowed column masks. The user's layout is stored per dialog key (CommitDlg, LogDlg, ChangedDlg…).

### 3.2 Status column text (`Git/TGitPath.cpp:1782-1811`; strings LLRC:118-133)
Flags (`TGitPath.h:49-66`): ADDED 0x1, MODIFIED 0x2, REPLACED (rename) 0x4, DELETED 0x8, UNMERGED 0x10, COPY 0x40, MERGED 0x80, ASSUMEVALID 0x200, SKIPWORKTREE 0x400, MISSING 0x1000, IGNORE 0x40000000, UNVER 0x80000000.
The first flag that matches decides the text:
1. UNMERGED → `Conflict`
2. ADDED → `Added`
3. MISSING → `Missing`
4. DELETED → `Deleted`
5. MERGED → `Merged`
6. MODIFIED → `Modified`
7. REPLACED → `Rename`
8. COPY → `Copy`
9. ASSUMEVALID → `Assume valid/unchanged`
10. SKIPWORKTREE → `Skip worktree`
11. IGNORE → `Ignored`
12. anything else, **including unversioned** → `Unknown`

Statistics tooltip words (SHELLRC:260-359): `normal`, `non-versioned`, `modified`, `added`, `deleted`, `replaced` (used as the "renamed" count), `conflicted`, `ignored`. Formats:
- `line: %d(+) %d(-) files: %s = %ld\n…` (RC:4056/4058) (`GitStatusListCtrl.cpp:3181-3217`).
- Label: `%1!d! files selected, %2!d! files total`.

### 3.3 How status is computed (`Git/Git.cpp:3460-3540`, `TGitPath.cpp:1120-1160`)
- Initial repo (no HEAD): `GetInitAddList`, i.e. index entries are shown as added.
- Otherwise, with head = `HEAD`, or `HEAD~1` when amending:
  1. `git diff-index --cached --raw <head> --numstat -C -M -z --` gives the staged changes. These are **unfiltered** when `includedStaged` is set or there is no filter.
  2. `git diff-index --raw <head> --numstat -C<n>% -M<n>% -z -- <path>` gives the worktree vs head changes, one call per filter path.
  3. Cygwin/MSYS2 git first runs `git status --no-ahead-behind -- <path>` to refresh stat info.
  4. Staging status (commit dialog with staging on): also `git diff-files --raw --numstat -C -M -z --`. A file appearing in both outputs is "partially staged".
- Conflicts: `git ls-files -u -t -z [-- <path>]`. Missing files: `git ls-files -d -z` (Git.cpp:3582-3628).
- Unversioned: `git ls-files --exclude-standard --full-name --others -z [-- <path>]`. Add `-i` for ignored.
- The similarity threshold comes from reg `DiffSimilarityIndexThreshold`, default 50.

### 3.4 Context menu (`GitStatusListCtrl.cpp:1639-2020`), in order, each item filtered by the dialog's `GITSLC_POP*` mask and by entry state
- Conflict entries: `&Edit conflicts`, `Resolved`, `Resolve conflict using "%s"` (×2, mine/theirs; RC:5181).
- Unversioned entries: `Add`, `Add as Executable (+x)`, `Add as Symlink`, `Commit...`.
- `Compare with b&ase` (or `Three way diff` for conflicts), `Show changes as &unified diff`, `Compare with &working tree`, `Compare parent with working tree…`.
- Two selected: `&Compare revisions`, unified diff, `Compare two files`.
- `Commit...`, `Re&vert...`.
- `Skip worktree`, `Assume Unchanged`, `Unflag as skip-worktree and assume-unchanged`.
- `Restore after commit` / `Restore`.
- `R&evert to this revision`, `Revert to parent revision`.
- `Show &log`, `Show log of submodule`, `Show log &before rename/copy`, `&Blame`.
- `Export selection to...`, `Save revision &to...`, `View revision in alternative editor`, `&Open`, `Open with...`, `Explore to`.
- `Mark for comparison` / compare with marked.
- `&Delete`.
- Ignore submenu (by name / by `*.ext`).
- `Copy paths to clipboard` submenu: `full paths`, `relative paths`, `file/foldernames`, plus `Copy all information to clipboard` and per-column copy.
- `Remove from changelist`.
- LFS `Lock` / `Unlock`.

Dialog masks:
- Commit: everything except Commit, Save-as and Prepare-diff (`CommitDlg.cpp:244`).
- Add: everything except Add, Commit, Changelists and Prepare-diff (`AddDlg.cpp:63`).
- Revert: everything except Commit, Restore and Changelists (`RevertDlg.cpp:65`).
- Resolve: everything except Ignore, Add, Commit, Export, Restore, Save-as, Prepare-diff and Changelists (`ResolveDlg.cpp:60`).
- ChangedDlg: everything except Save-as, Restore and Prepare-diff (`ChangedDlg.cpp:101`).
- Log: everything except Restore and Changelists (`LogDlg.cpp:314`).

### 3.5 Upstream quirks a faithful clone should replicate or record
- Unversioned rows show `Unknown` in the Status column (3.2).
- The Ignore dialog's default file radio comes from `m_IgnoreType` (2.9).
- `/closeonend` values 3 and 4 are documented but behave like 0 (1.5).
- Repeated CLI keys: the first one wins (1.1).
- Unknown `/command` falls back to the About dialog (1.2).
