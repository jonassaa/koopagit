# TortoiseGit behavioural spec: additional dialogs

Extracted from upstream TortoiseGit. Paths are relative to `src/`; `TP/` =
`src/TortoiseProc/`, `RC` = `Resources/TortoiseProcENG.rc`, `LLRC` =
`Resources/TortoiseLoglistCommon.rc2`. Window titles follow
`CAppUtils::SetWindowTitle` (`<path> - <caption> - TortoiseGit`, see
`spec-dialogs.md`). The "choose version" block (HEAD/Branch/Tag/Commit) is
described in `spec-settings-remote.md` §3.0.

## 1. Changed Files / File Diff (`CFileDiffDlg`)

Opened by (`TP/GitDiff.cpp:485-535`, `TP/Commands/ShowCompareCommand.cpp:24-42`,
`TP/Commands/PrevDiffCommand.cpp:28-41`):

- `/command:showcompare /revision1:<a> /revision2:<b> [/path:<p>]` without `/unified`:
  path empty (repo root) or a directory → File Diff with `SetDiff(path, rev1, rev2)`;
  a file → two-way diff of that file. `/unified` shows a unified diff instead.
- `/command:prevdiff` on a directory → File Diff of `HEAD~1` against the working tree.
- Log "Compare revisions" / "Compare change sets" (two commits).

Template `IDD_DIFFFILES`, caption **"Changed Files"** (RC:813-832):

| Control | Label / behaviour |
|---|---|
| Static | `Difference between` |
| Push-like check | `Diff Options` → menu (`FileDiffDlg.cpp:1359-1404`): `Ignore space at EOL`, `Ignore space change`, `Ignore all space`, `Ignore blank lines`, separator, `Use common ancestor (A...B diff)` (RC:3917-3920, 3993). Each toggles and reloads; the button looks pressed while any is on. |
| Button | `Show log` → `/command:log /range:<rev1>..<rev2>`; disabled if either side is the working tree (`:401-410, 1406-1411`) |
| Icon button | switch left/right, tooltip "Switches the comparison left<->right" (RC:4555); swaps revs and edits, then reloads (`:809-828`); disabled when rev2 is the working tree (`:259`) |
| Group | `Version 1 (Base)` — edit + menu button `HEAD` with entries `Reference Browser`, `Log`, `RefLog` (`:246-252`); read-only line `<short hash>: <subject>` |
| Group | `Version 2` — same |
| | Group captions become `Version 1 (Base, Newer commit time)` / `Version 2 (Newer commit time)` when that side's committer date is newer (`:830-856`, RC:4846-4849) |
| Edit | filter, cue `Filter paths` (RC:4686); initial text = the path given to `SetDiff` (`:165-166`) |
| List | columns `File`, `Extension`, `Action`, `Lines added`, `Lines removed` (`:172-182`, RC:4427, 4964, 4428, 3826-3827). Renames show `new (from old)` (`:358-377`). While loading: `Please wait while differences are obtained...`; empty: `No differences found!` (RC:4429, 4995) |
| Link | `View Patch>>` / `Hide Patch<<` (RC:4169-4170) |

Rev text that doesn't resolve shows `"%s" is invalid.` (RC:4843).

**Git command** (`Git/Git.cpp:1435-1470`), with `ignore` = the chosen
`--ignore-space-at-eol --ignore-space-change --ignore-all-space --ignore-blank-lines`:

- one side is the working tree: `git diff -r --raw -C50% -M50% --numstat -z <ignore> --end-of-options <rev> --` (with `-R` when rev1 is the working tree);
- otherwise: `git diff-tree -r --raw -C50% -M50% --numstat -z <ignore> --end-of-options <rev2> <rev1> --`;
- "Use common ancestor": rev1 replaced by `merge-base(rev1, rev2)` (`:297-317`).
- Rows whose numstat is empty (only ignored changes) are hidden (`:1193-1211`).

Double-click (`DoDiff`, `:412-437`): added → diff against nothing at rev2;
deleted → diff against nothing at rev1; renamed → old path at rev1 vs new path
at rev2; otherwise two-way diff `rev1` vs `rev2`.

## 2. Export (`/command:export [/rev:<ref>]`)

`TP/Commands/ExportCommand.cpp:24-32` → `CAppUtils::Export` (`TP/AppUtils.cpp:1055-1104`).
Template `IDD_EXPORT`, caption **"Export"** (RC:924-946):

| Control | Label / default |
|---|---|
| Group `Export Zip File` | `Zip File` label, edit, `...` (save-file dialog, filter `ZIP archive (*.zip)`, `TAR archive (*.tar)`, `TAR GZIP archive (*.tar.gz)`, `All Files (*.*)`, RC:3954; proposed name = version name). Tooltip "Location where the contents of the\nselected revision of the repository will be saved to." (RC:3930) |
| Group `Revision` | choose-version: `HEAD (<branch>)`, `Branch`, `Tag`, `Commit`; default HEAD, Tag if `/rev` starts `refs/tags/` (`ExportDlg.cpp:87-92`) |
| Check | `Whole Project`; checked and disabled when the path is the repo root (`:80-84`) |
| Buttons | `OK` (enabled only with a file name), `Cancel`, `Help` |

OK (`:120-150`): no version → "Invalid revision number!"; no extension → `.zip`
appended; existing folder → "You selected a folder.\nExports are only possible to a
(zip) file."; existing file → "The file\n%s\nalready exists! Do you want to
overwrite it?" (Yes/No, default No); empty → "You must select a filename for the zip-file!".

Command: `git archive --output=<file> --verbose --end-of-options <version>`, run
in the selected sub-folder unless Whole Project (so only that subtree is
archived). Post button on success: `Explore to`.

## 3. Format Patch (`/command:formatpatch [/startrev] [/endrev]`)

`TP/Commands/FormatPatchCommand.cpp:28-92`. Template `IDD_FORMAT_PATCH`, caption
**"Format Patch"** (RC:1339-1367):

| Control | Label / default |
|---|---|
| Group `Output Directory` | `Directory:` combo (history `History\FormatPatchURLS`/`path`, plus the repo dir) + `...` folder picker |
| Group `Version` | radio `Since` + combo of all branches (default `History\FormatPatch\<WD>\Since`) + `...` (ref browser) · radio `Number Commits` + number (min 1, default 1) · radio `Range` with `From:` / `To:` combos (histories `FormatPatchFromURLS`, `FormatPatchToURLS`) + `...` (log picker) |
| Checks | `Send Mail after create` (`TortoiseProc\FormatPatch\SendMail`, 0), `No a/ and b/ prefixes` (`TortoiseProc\FormatPatch\NoPrefix`, 0) |
| Buttons | `Save unified diff since HEAD` (unified diff HEAD vs working tree), `OK`, `Cancel`, `Help` |

Radio enables only its own controls (`FormatPatchDlg.cpp:226-252`). `/startrev`
alone → Since=start, From=start~1, To=start; `/startrev`+`/endrev` → Range.
OK disabled in an empty repository.

Command: `git format-patch[ --no-prefix] -o <dir> <range> --` where range is
`--end-of-options <since>`, `-<n>`, or `--end-of-options <from>..<to>`. Runs in a
progress dialog; Send Mail afterwards if checked.

## 4. Apply Patches (`/command:importpatch`)

`TP/Commands/ImportPatchCommand.cpp:26-78` adds every non-directory `/path` to
the list. Template `IDD_APPLY_PATCH_LIST`, caption **"Apply Patches"** (RC:1318-1336):

| Control | Label / default |
|---|---|
| List | patch files (no header); checkbox per row; status per row (applying / success / fail / skip) |
| Buttons right | `&Add` (open-file, multi-select, filter "Patchfiles (*.diff, *.patch)" RC:4142), `&Up`, `&Down`, `&Remove` |
| Checks | `&3 way merge` (checked), `&ignore space change` (checked), `Add "&Signed-off-by"` (unchecked), `&Keep CR` (checked) (`ImportPatchDlg.cpp:36-42`); tooltips RC:4744-4745 |
| Tabs | `Patch` (selected patch's content), `Log` (RC:4763-4764) |
| Buttons | `A&pply`, `&Cancel` (becomes `A&bort` while running), `Help`; Apply becomes `&OK` when all patches are done |

Apply (`:360-535`), per checked patch in order, starting at the first
not-yet-applied one:

1. while `.git/rebase-apply` exists: ask "previous rebase directory rebase-apply
   still exists but mbox given.\n\nDo you want to" with `A&bort` / `&Skip` /
   `&Resolved` → `git am --abort` / `--skip` (and skip this row) / `--resolved`.
2. `git am [--signoff ][--3way ][--ignore-space-change ][--keep-cr ]-- <patch>`.
   Failure → row "fail", log output + `Fail`, stop. Success → `Success`.
3. Unchecked rows log `Skip Patch: %s`.

Cancel when idle and `git am` still active: "\"git am\" is still in apply
mode.\nDo you want to abort?" Yes → `git am --abort`, No → close, Cancel → stay.
Requires user name/email.

## 5. Worktrees

### New Worktree (`/command:newworktree`, `/command:dropnewworktree /droptarget:<dir>`)

`TP/Commands/WorktreeCommand.cpp:24-48`, `CAppUtils::CreateWorktree`
(`TP/AppUtils.cpp:1213-1262`). Template `IDD_WORKTREE_CREATE`, caption **"New
Worktree"** (RC:2305-2332):

| Control | Label / default |
|---|---|
| Group `&Location` | `Directory:` edit + `Bro&wse...`. Default `<repo>-worktree`, or the repo path without `.git` for a bare `*.git` repo (`CreateWorktreeDlg.cpp:89-95`). Drop: `<target>/<repo name without .git>` |
| Group `Base On` | choose-version, `HEAD (<branch>)` default |
| Group `Options` | `Create &New Branch` + name, `&Checkout` (checked), `&Force` (tooltip RC:4081), `&Detach` |

Version change (`:124-159`): HEAD → name = directory leaf, branch off; remote
branch (`origin/x`, `remotes/r/x`) → name `x`, branch on; otherwise
`Branch_<version>` (commit: short hash), branch off; Commit or Tag force branch
on. Branch off with Commit/Tag/remote branch → Detach checked and disabled.
OK: empty dir → "This field is required and must not be empty."; invalid branch
name → "Branch/Tag name must not be empty or is invalid.".

Command: `git worktree add[ --no-checkout][ --force][ --detach | -b <name>] -- <dir> [<version>]`
(version omitted for HEAD). Post button `Update Submodules` →
`/command:subupdate /bkpath:<dir>` when the new worktree has submodules.

### Worktree List (`/command:worktreelist`)

Template `IDD_WORKTREE_LIST`, caption **"Worktree List"** (RC:2335-2344). Columns
`Path`, `SHA-1`, `Branch` (`detached HEAD` when detached), `Locked`
(`Locked`/`Unlocked`, empty for the main worktree), `Reason`
(`WorktreeListDlg.cpp:76-80, 225-264`). Buttons `&Add` (New Worktree), `&Prune`
(`git worktree prune`), `OK`, `Help`; F5 refreshes; double-click opens the folder.

Context menu (`:279-430`): one row → `Explore to` (default); `Lock` if not locked,
`Unlock` if locked (both for multi-select); `&Remove`, `Force remove` unless the
only row is the main worktree. Remove asks "Do you really want to delete the
selected worktree(s)?" then `git worktree remove[ --force] -- <path>` per row (main
worktree skipped; failure offers `Force remove`). Lock/Unlock report "Locked %d
worktree(s)." / "Successfully unlocked %d worktree(s).".

## 6. Submodule Add (`/command:subadd`)

`TP/Commands/SubmoduleCommand.cpp:30-90`. Template `IDD_SUBMODULE_ADD`, caption
**"Submodule Add"** (RC:1569-1589): group `Submodule of Project: <repo>`,
`Repository:` combo (history `History\SubModuleRepoURLS`/`url`, prefilled from a
`git clone <url>` clipboard link) + `...`, `Path:` combo (history
`History\SubModulePath`, default = selected folder relative to the repo); editing
the URL appends the repo name (without `.git`) to the path
(`SubmoduleAddDlg.cpp:232-243`); `Branch` check + edit; `&Force`; `Load Putty &Key`.
OK: empty path/repository/branch → "This field is required and must not be empty.".

Command: `git submodule add[ -b <branch>| --force] -- <repo> <path>`. Upstream
quirk: `--force` *replaces* `-b <branch>` (`args = L" --force"`, `:50-51`).

## 7. Request pull (`/command:requestpull [/endrev] [/url]`)

`TP/Commands/RequestPullCommand.cpp:24-34`, `CAppUtils::RequestPull`
(`TP/AppUtils.cpp:2926-3012`). Template `IDD_REQUESTPULL`, caption **"Request
pull"** (RC:1241-1256): `&Start` combo + `...` (log picker; default
`History\RequestPull\<WD>\startrevision`), `Repository &URL` combo (history
`History\RequestPull`/`url`, default `…\<WD>\repositoryurl`), `End` edit (default
`…\<WD>\endrevision`, else `HEAD`), `Send Mail after create`
(`TortoiseProc\RequestPull\SendMail`). Empty end → "Branch/Tag name must not be
empty or is invalid.".

Command: `git request-pull -- <start> <url> <end>`, output written to a temp
`pullrequest.txt` ("Creating pull-request..."); failure "Failed to create
pull-request." + stderr. The text then opens in the alternative editor (or Send
Mail).

## 8. Repository Browser (`/command:repobrowser [/rev:<ref>]`)

`TP/Commands/RepositoryBrowserCommand.cpp:22-33` (rev default `HEAD`). Template
`IDD_REPOSITORY_BROWSER`, caption **"Repository Browser"** (RC:325-339): `Path:`
(read-only), `Revision:` button showing the revision (opens a log picker,
`RepositoryBrowser.cpp:963-984`), tree on the left (root `/`), list on the right
with columns `Filename`, `Extension`, `Size` (`:203-205`), info label, `OK`,
`Cancel`, `Help`. Info label (`:622-665`): multi → "%1!d! items selected";
submodule → "Submodule \"%1!s!\"\nRevision %2!s!"; file → "%1!s!\nSize %2!s!";
none → "%1!s!\nShowing %2!d! files, %3!d! submodules and %4!d! folders, %5!d! items
in total". Double-click/Enter: folder → enter it; file → open.

Context menu (`:727-915`), for list selection or a tree node:

| Entry | Condition | Action |
|---|---|---|
| `&Open` (default) | 1 selected | folder → enter; file → extract `git show <rev>:<path>` to temp and open; submodule (with WC) → `/command:repobrowser /path:<sub> /rev:<hash>` |
| `Open with...` | 1, files | OS "open with" |
| `View revision in alternative editor` | 1, files | alternative editor |
| separator | | |
| `Compare with &working tree` | 1, files, has WC | diff `<rev>` vs working tree |
| `Show &log` | 1 | `/command:log /path:<p> /endrev:<rev> /rev:<rev>` (+`/submodule` for submodules) |
| `Show log of submodule` | 1, submodule | log at the submodule hash |
| `Blame` | 1, file, has WC | blame at `<rev>` |
| `Save revision &to...` | 1, file | save-file dialog, `git show` |
| `R&evert to this revision` | files only, has WC | `git checkout --end-of-options <rev> -- <path>` each; "%1!d! files reverted to %2!s!." |
| `Mark for comparison` | 1 file | remember path + rev |
| `Diff with "<marked>"` | 1 file, something marked | diff marked vs selected |
| `Copy paths to clipboard` | any | names, CRLF-separated |
| `Copy SHA-1 to clipboard` | any | object ids |

Labels: LLRC (`Open with...`, `View revision in alternative editor`, `Compare
with &working tree`, `Blame`, `Save revision &to...`, `R&evert to this revision`,
`Copy SHA-1 to clipboard`), RC:3947 (`&Open`), RC:4133 (`Mark for comparison`),
RC:4365, shell `IDS_MENULOG` `Show &log`, `IDS_MENUDIFFNOW` `Diff with "%s"`.
