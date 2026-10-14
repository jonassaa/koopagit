# Divergences from TortoiseGit

KoopaGit clones TortoiseGit's behaviour (see `docs/spec/`). This file lists
every place where it deliberately differs, and why. Add to it whenever a
platform forces a change.

## Shell integration

| Area | TortoiseGit (Windows) | KoopaGit | Reason |
|---|---|---|---|
| Overlay mechanism | Nine `IShellIconOverlayIdentifier` handlers registered through the shared *TortoiseOverlays* component; Explorer loads only 15 handlers system-wide and uses the first handler that answers `S_OK`, ordered by `GetPriority`. | Windows: nine handlers registered directly with KoopaGit's own CLSIDs (same priorities). macOS: one Finder Sync badge per path. Linux: one Nautilus emblem per path. Both pick the overlay with the same status → overlay mapping (spec §2.2). | Finder and Nautilus have no slot limit and no priority race, so the slot fallback chain (e.g. Added → Modified when the Added slot is not loaded) only applies on Windows. TortoiseOverlays is a separate installer we do not ship. |
| Overlay icon set / handler page | "Icon Set" and "Overlay Handlers" settings pages (`TortoiseOverlays` registry). | Fixed KoopaGit badge/emblem set; the Windows extension loads `icons\<Name>Icon.ico` next to the DLL. | No TortoiseOverlays on macOS/Linux. |
| Status cache | `TGitCache.exe` (recursive, default), or in-process "Shell" / "Shell extended" caches. | Status computed in-process with `gix` for each query; folder status is always recursive. `CacheType` 0 ("None") is honoured; 2/3 behave like 1. The "Shell" mode's direct-children-only folder status is not replicated. | No cache process yet. Recursive status is what the default upstream mode shows. |
| Drive-type masks | `DriveMask*` settings decide which drive types get overlays. | Ignored. | Windows drive types have no portable equivalent; the include/exclude path lists still apply. |
| Finder folders | Explorer shows overlays everywhere. | Finder Sync only runs in configured folders: `~/.config/koopagit/finder-roots` (default: home). | Finder Sync API requirement (`directoryURLs`). |
| macOS sandbox | n/a | The Finder extension is sandboxed with a development-only read-only file exception; menu actions are handed to the unsandboxed host app as a request file it opens (sandboxed callers cannot pass launch arguments). | Finder Sync extensions must be sandboxed; dialogs must not inherit the sandbox. |
| Context menu placement | Top-level entries prefixed `Git `, a "TortoiseGit" popup, separators around both. | Same entries, order, masks and `Git ` prefix on every platform. Nautilus has no separators, so they are dropped there. | Nautilus menu API has no separator item. |
| Accelerators | `&` marks access keys in menu labels. | Kept on Windows; stripped (`&&` → `&`) in the macOS and Nautilus menus. | Finder and Nautilus menus have no access keys. |
| Menu icons | Per-entry icons (`ShowContextMenuIcons`). | No menu icons yet on any platform. | Not implemented; Finder/Nautilus menu icons need per-platform image assets. |
| Windows 11 menu | `IExplorerCommand` "TortoiseGit" root with `ContextMenu11Entries`. | Not implemented; only the classic menu. | Requires a sparse package/identity; deferred. |
| Right-drag menu | Move/copy/add/patch/worktree entries on right-drag. | Not implemented. | Finder and Nautilus have no drag-menu extension point; Windows deferred. |
| `/pathfile:` | Explorer always writes selected items to a UTF-16 temp file for commands like commit/add. | All extensions go through `koopagit-shell invoke`, which uses `/pathfile:` (UTF-16LE + BOM, `/deletepathfile`) when more than one path is passed and `/path:` for a single path. | Same file format; single paths stay readable on the command line. The launcher accepts both. |
| Launcher name | `TortoiseGitProc.exe /command:...` | `koopagit-proc /command:...` (same grammar and command names). | Different product name; arguments map 1:1. |
| Subcommand syntax | Only `/command:<name> /key:value`; no arguments shows the About dialog. | Also accepts `koopagit <command> [--key value] [path...]` with `-h`/`--help`, `help <command>`, `-V`/`--version` and `--` (built with clap). Options are the upstream keys in kebab-case (`--close-on-end` = `/closeonend`, upstream spelling accepted); paths are positional. No arguments prints usage. The upstream grammar is chosen whenever the first argument starts with `/` or a single `-`, and is what every shell extension uses. Internal drag-and-drop commands are hidden from help; `koopagit help` prints CLI help, so the manual is `koopagit rtfm` (or `/command:help`). | `/command:` is awkward to type in a terminal; added at the user's request. Both forms build the same invocation. |
| Product name | "TortoiseGit" in window titles (`<path> - <dialog> - TortoiseGit`), message-box captions, the context-menu submenu, settings texts; viewers named TortoiseGitMerge / TortoiseGitBlame / TortoiseGitUDiff. | "KoopaGit" everywhere the name is shown (KoopaGitMerge / KoopaGitBlame / KoopaGitUDiff; `KoopaGit.app` on macOS). Settings keys keep their upstream names (`TortoiseProc\...`, `TortoiseGitMerge\...`); the Help command still opens the TortoiseGit manual. | Different product; the manual describes the same behaviour. |

## Settings and tools

| Area | TortoiseGit | KoopaGit | Reason |
|---|---|---|---|
| Settings store | Registry `HKCU\Software\TortoiseGit\...` (and some HKLM policy values). | Plain text file `settings.ini` (`%APPDATA%\koopagit`, `$XDG_CONFIG_HOME/koopagit` or `~/.config/koopagit`; override with `KOOPAGIT_SETTINGS`). Keys keep their upstream names with `\` separating sub-keys. HKLM policy masks are not read. | No registry on macOS/Linux; one format everywhere. |
| PuTTY / Pageant | "Autoload Putty Key" options and TortoiseGitPlink. | Not offered; git uses the platform's SSH agent/config. | PuTTY/Pageant are Windows-specific. |
| Recycle bin | Revert and Clean up move files to the recycle bin by default (`RevertWithRecycleBin`). | The platform trash via the `trash` crate (Windows Recycle Bin, macOS Trash, freedesktop Trash). Files that cannot be trashed are listed; Revert then restores them anyway. | The recycle bin is a Windows shell feature; each platform has its own trash API. |
## Remote operations (Push, Pull, Fetch, Sync, Clone)

- **PuTTY key / Pageant** ("Autoload Putty Key", "Load Putty Key", `remote.<name>.puttykeyfile`): omitted. PuTTY and Pageant are Windows-only; on every platform KoopaGit relies on git's own SSH configuration (ssh-agent, `core.sshCommand`).
- **Clone from SVN** (`git svn clone` options): omitted for now; git-svn support is out of the initial scope.
- **Clone repository-browser button**: omitted. The directory "Browse..." button uses the native folder picker (`rfd`).
- **Clipboard prefill** of URLs (`git clone <url>` / `git pull` links on the clipboard): omitted; no clipboard API in the core dialogs yet.
- **Radio buttons** are rendered as mutually exclusive check boxes (Slint's standard widgets have no radio button).
- **Sync dialog**: Push notes, Compare tags, Ref List, the Submodule and Stash split buttons, and Apply/Email Patch are not implemented yet. Shift+click shortcuts to the full Pull/Fetch/Push dialogs are not available.
- **Sync In/Out ChangeList tabs**: filled when the dialog opens or the refs change, from the merge base of the local and remote branch (In: merge base → remote; Out: merge base → local). Upstream fills In ChangeList only after a pull/fetch, from the old and new remote hash. The Conflicts tab is not implemented.
- **Pull "Merge unrelated histories" and "Reset" post-buttons**: not offered. The post-command buttons launch `/command:` invocations, and TortoiseGit has no command-line form for these two actions.
- **Fetch/Pull "Launch Rebase After Fetch"** opens `/command:rebase /upstream:<ref>` after the fetch, instead of opening the Rebase dialog in-process.

## Settings dialog

| Area | TortoiseGit | KoopaGit | Reason |
|---|---|---|---|
| Windows 11 Context Menu, Overlay Handlers pages | Present (Windows). | Omitted. The ignored/unversioned overlay toggles live on Icon Overlays and are saved under the keys the shell reads (upstream saves them under `TortoiseOverlays`, where the shell never reads them). | Windows-only features; fixes an upstream no-op. |
| Icon Overlays | "Show overlays and context menu only in explorer", drive-type checkboxes. | Omitted. | Windows-only concepts. |
| General | "Git for Windows" group, Check now / First Start Wizard / Create Library buttons, browse button. | Group titled "Git" outside Windows; those buttons are absent. | Windows-only or not implemented yet. |
| Remote page | Putty Key field. | Shown only on Windows. | PuTTY is Windows-only. |
| Alternative editor fallback | With no alternative editor configured, config files, "View revision in alternative editor" and the Request pull result open in Notepad. | They open in the platform's default text editor (`open -t` on macOS, `xdg-open` on Linux; Notepad on Windows). | Notepad is Windows-only. |
| Diff Viewer | Per-extension "Advanced..." dialog. | Not offered in the UI (the `DiffTools\.<ext>` / `MergeTools\.<ext>` settings are honoured). | Not implemented yet. |
| Stub pages | Credential, Issue Tracker Integration (COM providers), Icon Set. | Show "Not available in KoopaGit yet." | Credential helpers are set by the First Start Wizard; providers and icon sets are Windows-specific. |
| Small UI differences | Tri-state "Select/deselect all"; hierarchical-config hint box. | Plain button; no hint box. Switching config level discards unsaved edits on that page. | Slint widget set. |

## Diff, merge and blame viewers

| Area | TortoiseGit | KoopaGit | Reason |
|---|---|---|---|
| Conflict editor | TortoiseGitMerge three-way view of BASE / MINE / THEIRS. | Works from the conflict markers in the working-tree file (theirs left, mine right, merged preview); BASE is not shown unless diff3 markers are present. | Simpler first implementation. |
| Viewer processes | TortoiseGitUDiff and TortoiseGitBlame are separate executables. | Shown in-process by `koopagit-proc`; only `koopagit-merge` is a separate binary. | Fewer binaries; same command names. |
| `/diff:<patch>` | Review/apply a patch in TortoiseGitMerge. | Displays the patch in the unified-diff viewer only. | Patch apply not implemented yet. |
| Image diff | TortoiseGitIDiff. | Not available. | Out of scope for now. |

## Log, RefLog, Browse References, Reset

| Area | TortoiseGit | KoopaGit | Reason |
|---|---|---|---|
| Copy to clipboard | Win32 clipboard. | `pbcopy` / `clip` / `wl-copy`, `xclip`, `xsel`; if none works the text is shown in a copyable field. | Slint has no portable clipboard API. |
| Reset dialog | Modal from Log/RefLog/Browse References (no `/command:`). | Non-blocking window from the same places, with the same Branch/Tag/Commit picker. | Avoids nested event loops in Slint. |

## Reflog, refs, rebase and viewer menus

| Area | TortoiseGit | koopagit | Reason |
|---|---|---|---|
| Context menus | Win32 popup menus built per click. | Native menus (`DynMenuArea`) whose content comes from pure Rust builders (`menus::reflog`, `refbrowse`, `rebase`, `commitmsg`, `viewers`). Same entries, order and conditions; Windows `&` accelerators are kept in the labels. | Slint's `for` can only repeat one element type, so menus are rendered as fixed blocks of items/submenus/separators. |
| RefLog / Stash List | Full log-list menu with every §2 entry. | All entries that apply to reflog rows except "Switch/Checkout to <branch>", "Push", "Delete <ref>", "Search log messages" and "Collapse"; merge commits use their first parent (no "Parent N" submenus). | Reflog rows carry no ref labels; the rest is not implemented yet. |
| Browse References | Ref tree with in-place label editing; "Select tracked branch" opens a ref picker; Filter info-icon menu. | Collapsible tree plus list. Rename, "Edit description" and "Select tracked branch" use an input bar under the list (tracked branch is typed as `remote/branch`). The tree lists all refs under a node recursively. No filter-field menu. "Browse repository" launches `/command:repobrowser` (implemented separately). | Slint has no editable tree items and cannot nest modal dialogs. |
| Delete remote tags | `CDeleteRemoteTagDlg`. | Same idea: lists `git ls-remote --tags`, deletes checked tags with one `git push -- <remote> :refs/tags/<t>...`. | — |
| Rebase dialog | Rebase/cherry-pick engine; "Add" button picks more commits from a log; Up/Down with Shift moves to the top/bottom; "Revision Files" tab; SquashDate setting. | Engine (`koopagit_core::rebase`) with Pick/Squash/Edit/Skip, Onto, Preserve merges, Force, autostash (`rebase.autostash`) or "stash?" prompt, Edit stops, Continue after conflicts, Skip, Abort, cherry-pick mode (`/cherrypick /revs:<sha,...>`, `-x` via `CherrypickAddCherryPickedFrom`). No "Add" button and no "Revision Files" tab; Shift+U/D (keyboard) moves to the top/bottom; squash keeps the earlier commit's author date. The replay runs on the UI thread. | Not implemented yet / simpler. |
| Commit message editor | Scintilla menu with spelling suggestions, Undo/Redo, Split lines, "Paste recent message..." opening a history dialog. | Cut/Copy/Paste/Select All, Pick commit hash/message, Paste filename list, Paste last commit message, and a "Paste recent message" submenu with the history entries. No spelling, Undo/Redo or Split lines entries (Ctrl/Cmd+Z still works). Text is inserted at the caret through the clipboard, which is restored afterwards. "Pick commit hash/message" needs the Log dialog's `/outfile` picker mode. | Slint's `TextEdit` exposes no undo or insert API. |
| TortoiseGitMerge menus | Menus act on the selected lines; header-bar menu ("Show diff separately"); whitespace/EOL/encoding block; three-pane "Use this whole file" applies to Bottom. | Menus act on the block of differences under the cursor; two-pane right file is editable unless it is a temporary revision file, with a Save button; marks are per block. No header-bar menu and no whitespace/EOL/encoding entries. "Copy" copies the block's lines. | No text selection in the viewer rows. |
| TortoiseGitBlame | Line menu also in a log pane (§9.2). | Line menu only (Blame previous / Compare with previous with "Parent N" submenus, Show log, Copy SHA-1, Copy log); there is no log pane. | No log pane in the blame viewer yet. |

## Branch, stash, rebase and file operations

| Area | TortoiseGit | KoopaGit | Reason |
|---|---|---|---|
| Questions inside dialogs | Separate message boxes. | Shown in a notice bar inside the dialog. | Slint does not support nested modal event loops. |
| Revert pre-check | Only directly selected and added files are pre-checked. | When the whole repository is selected every row starts checked. | Avoids an all-unchecked list. |
| Git Init special folders | Drive roots, Desktop, Profile, Documents, Windows, System, Program Files. | `/`, the home folder, `~/Desktop`, `~/Documents` (Windows equivalents on Windows). | Platform equivalents. |

## File list context menu

| Area | TortoiseGit | KoopaGit | Reason |
|---|---|---|---|
| "Shell" submenu | Embeds the Windows Explorer context menu of the selection. | Not offered. | No equivalent API in Finder/Nautilus menus inside an app; Windows deferred. |
| Changelists | "Remove from changelist", "Move to changelist" (incl. `ignore-on-commit`), "Keep changelists", group header menu. | Not implemented. | TortoiseGit-only bookkeeping; not part of git. |
| Column-specific copy | "Copy to clipboard ▸ column '<name>'" for the right-clicked column. | Not offered. | Slint list rows don't report which column was clicked. |
| Shift while choosing | Holding Shift when picking a diff entry uses the alternative diff tool; Shift+Delete deletes permanently. | Shift is only read when the menu opens (it still reveals "Add as Executable/Symlink"). | Slint menu activation carries no modifier state. |
| Confirmations | Modal message boxes. | Non-blocking message boxes (`msgbox::ask_async`). | Slint has no nested modal event loop. |
| "Open with..." | Windows "Open with" dialog. | Windows: same dialog; macOS/Linux: pick an application, then open the file with it. | No cross-platform "Open with" chooser. |
| Recycle bin on revert | Moves the working-tree file to the Recycle Bin before reverting. | Moves a copy of the file to the OS trash, then reverts. | Same recoverability with the `trash` crate. |
| "Restore after commit" | After a successful commit the marked files are restored silently; after a failed commit, or on Cancel, the user picks "Restore old state" / "Keep current state" (plus Cancel). | Same. The failed-commit question is a message box; the Cancel-time question is shown in the Commit dialog's notice bar. | Slint does not support nested modal event loops. |
## Log menus

| Area | TortoiseGit | koopagit | Reason |
|---|---|---|---|
| Ref label under the cursor | Several log-menu entries (Switch to, Push, Delete, Create branch, Copy tag/branch names) target the ref label the mouse is on. | Entries use all of the commit's refs (submenus where upstream has them); the hovered label is not detected. | Slint reports no hit position for labels inside a row when the menu opens. |
| Combine to one commit | Any `HEAD~a`..`HEAD~b` range; opens the Commit dialog in amend mode, then cherry-picks the newer commits back. | Only ranges that end at HEAD; commits directly with the concatenated messages (oldest first) after a confirmation; amend afterwards to edit the message. `SquashDate` is ignored. | Reduced first implementation. |
| Collapse / Expand revisions | Toggles roll-up of merge-point-filtered rows. | Not offered (the merge-point filter does not exist yet). | Not implemented. |
| Show branches this commit is on | Modeless "Commit is on refs" dialog inside the Log process. | The same dialog via `/command:commitisonrefs /rev:<sha>` in its own process. | KoopaGit opens dialogs from the Log as separate launcher commands (the RefLog does the same). |
| Bisect start | Opens the Bisect start dialog with good/bad prefilled. | Runs `git bisect start <newer> <older>` directly. | No bisect dialog yet. |
| Edit Notes | Notes dialog. | Multi-line notes window; empty text removes the note (`git notes remove`). | Same behaviour, different window. |
| Ref label "Browse References" | Opens the picker and shows the chosen ref's log. | Launches `/command:refbrowse`; pick from there. | Browse References cannot return a choice to another process. |
| Date pickers | From/To date-time pickers. | `YYYY-MM-DD` text fields (Enter applies), with the From menu. | Slint has no date picker. |
| Statistics | Statistics, graph and pie pages (`CStatGraphDlg`). | Summary values and per-author / per-week tables only. | Charts not implemented. |
| Jump bar "(FF)" types | Follow fast-forward history. | Follow first parents. | Equivalent for linear history; approximate otherwise. |
| Unified diff "All Parents" / "Only Merged Files" | Combined diff in TortoiseGitUDiff. | `git diff-tree -p -m` / `--cc` shown with `koopagit-merge /diff:` in its own window. | The in-process viewer blocks. |
| Working-tree row | Shows author/date of the working tree. | No author or date. | No meaningful time zone-correct value without a tz database. |
| Shift-held menu variants | Shift shows hidden "Show log" / "Push" entries and selects the alternative diff tool. | Not detected when a menu opens. | Slint does not pass modifiers to `ContextMenuArea`. |
## Additional dialogs

| Area | TortoiseGit | koopagit | Reason |
|---|---|---|---|
| File Diff revision buttons | `HEAD` split button with `Reference Browser` / `Log` / `RefLog` pickers. | `HEAD` button plus an editable combo of branches, tags and `Working Tree`. | No reference/log picker windows in select mode yet. |
| File Diff list | Columns `File` / `Action`, own context menu (spec-menus §1.7). | Same columns and menu (`menus/filediff.rs`). "Show changes as unified diff" with the working tree as version 1 shows `version 2..Working Tree`, i.e. the reverse direction. | `/command:showcompare /unified` only diffs from a revision to the working tree. |
| Export post button | `Explore to` after a successful export. | Not offered. | Post buttons can only launch commands or run git. |
| Format Patch pickers | `...` buttons open the log / reference browser. | Editable combos with history and branches; folder picker for the output directory. | No picker windows yet. |
| Apply Patches list | Drag & drop of patch files; patch list context menu. | Add button (native file picker) only. | Not implemented yet. |
| Import patch outside a repository | `ChooseRepository` folder dialog. | Current directory first, then a native folder picker. | Same flow, platform picker. |
| Worktree lock reason | libgit2 lock without a reason. | `git worktree lock` (no reason prompt either). | Same behaviour via the CLI. |
| Submodule Add | Clipboard `git clone <url>` prefill, PuTTY key. | Not offered. | Clipboard read not wired; PuTTY is Windows-only. |
| Submodule Add `--force` | Quirk: `--force` replaces `-b <branch>`. | Kept. | Parity. |
| Repository Browser tree | Native tree view; revision chosen through the log dialog; drag & drop out of the browser. | Indented folder list with expand arrows; revision typed or picked from branches/tags; no drag & drop. | Slint has no tree view or drag-out; no log picker yet. |
| Repository Browser "Open with..." | Windows "Open with" dialog. | Same as Open (default application). | No portable "open with" chooser. |
| Recycle bin | Revert and Clean move files to the recycle bin (`RevertWithRecycleBin`). | Same, using the platform trash (`trash` crate). Clean's recycle mode runs `git clean -n` and trashes the listed paths (also inside submodules when "Submodules" is checked). | — |
| Merge "Remove branch" | Asks for confirmation, then `git branch -D`. | The post button runs `git branch -D -- <branch>` in a progress window without an extra confirmation. | Post buttons can't show a nested question. |
| Add "+x" / "Symlink" | Changes the index mode of the added files. | Same via `git update-index --chmod=+x` / `--cacheinfo 120000,...`; the symlink button is offered for at most 50 files (it pre-hashes their contents). | Bounded pre-work. |

## Remaining commands

Spec: `docs/spec/spec-dialogs-3.md`.

| Area | TortoiseGit | koopagit | Reason |
|---|---|---|---|
| Revision graph | GDI-drawn graph with overview pane, filter dialog (From/To, local branches, current branch), node shapes per type, "Save graph as" to an image file of the whole graph. | Slint boxes and straight edges laid out with the log's lane algorithm; File/View/Git menu bar, zoom, node context menu. No overview pane or filter dialog (always all branches); "Arrows point towards merges" is stored but edges have no arrowheads; "Save graph as" writes a PNG of the visible window. | Reduced first implementation on Slint primitives. |
| Send Mail delivery | SMTP direct (MX lookup), MAPI, or a configured SMTP server, with TortoiseGit's own mail code. | SMTP via `git send-email` (server/port/encryption from the Email page); MAPI, or no `git send-email`, opens the default mail client with a `mailto:` link per message (attachments must be added by hand). "SMTP direct" uses git's configured `sendemail.smtpServer` unless the Email page names one. | No portable SMTP/MAPI client; reuse git's mailer. |
| Email settings credentials | "Store credentials..." / "Clear" (Windows credential store). | Not offered; `git send-email` asks or uses `sendemail.smtpUser`/`smtpPass`. | Windows credential manager. |
| git-svn | `git svn` commands in the progress dialog; svnrebase opens the Rebase dialog in-process. | Same commands; svnrebase starts `/command:rebase /upstream:<svn ref>` as a separate process and waits; "Pulled Diff"/"Pulled Log" after svnfetch compare against the upstream ref name rather than the post-fetch hash. `svn show-ignore` output is written to `.git/info/exclude` and confirmed in a message box instead of the progress log. | Post buttons are fixed before the job runs. |
| daemon | Lists every local IPv4/IPv6 address as `git://<ip>/` in the progress log. | Shows the primary address in the progress window title. | No pre-text in the progress dialog; std has no interface enumeration. |
| pgpfp | Shows the TortoiseGit release signing key fingerprint. | Explains that koopagit releases are not signed yet. | koopagit has no signing key. |
| updatecheck | Downloads TortoiseGit's version file and offers updates. | Shows the version and that update checks are unavailable. | No koopagit update server. |
| commitisonrefs | Ref list with multi-selection (compare two refs, log of A..B / A...B), commit picker button (Browse refs / Log / RefLog). | Single selection: Show log, Browse repository, Compare with working tree, Copy. Commit is typed. | Reduced first implementation. |
| First Start Wizard | Language download links, Git-for-Windows link, PuTTY key generation, Cygwin/MSYS2 hacks, credential "Advanced..." page. | English only; SSH client choice is written to `core.sshCommand`; credential helper choices per platform (osxkeychain/libsecret/manager, cache, store). | Platform differences. |
| Drag and drop | `dropmove`, `dropcopyadd`, `dropnewworktree` from Explorer's right-drag menu. | `dropmove` and `dropcopyadd` exist as commands; no file manager sends them yet. Nested repositories inside copied folders are skipped (their `.git` is not copied) instead of asking. `dropcopy` is compiled out upstream too. | No drag menu extension point in Finder/Nautilus. |
| Network page | Proxy enabled when `http.proxy` is set; SSH client stored in the registry and passed as `GIT_SSH`. | "Enable Proxy Server" is stored as `Git\Servers\global\http-proxy-enabled`; on Apply `http.proxy` is written to/removed from the global git config. The SSH client is written to the global `core.sshCommand`. The password is shown as plain text. | koopagit runs git with the user's own config; Slint row model has no password field. |
| Colors / Blame / UDiff pages | Change the colours of status lists, log graph, TortoiseGitBlame and TortoiseGitUDiff; "Dark theme" switches TortoiseGit to dark mode. | Values are stored with upstream keys and defaults (`COLORREF`). Only the Log graph applies them (Colors 3: branch line colours, line width, node size; in dark mode with lightness inverted like `CTheme::GetThemeColor`); other views keep their built-in colours, and the theme follows the OS. | Applying them needs changes to shared view code. |
| Log graph lines | `paintGraphLane` draws a lane joining or leaving a node as a horizontal bar plus a quarter arc, filled with a gradient from the node lane's colour to the branch lane's colour. | Same shapes (quarter-ellipse arcs into the lane) in the branch lane's solid colour; a lane shifting sideways away from a node is an S-curve. The Graph column widens to fit the lanes (up to 320 px). | Slint `Path` strokes take one brush per path; the lane algorithm is KoopaGit's simpler one, not qgit's `Lanes`. |
| Hook Scripts | Local hooks from `.tgitconfig` (project hooks, with an approval prompt). | User hooks from the settings store only; `.tgitconfig` project hooks are not run. "Hide the script while running" has no effect. A failed pre-rebase hook asks Abort / Proceed anyway in the Rebase dialog's notice bar instead of a task dialog. | Reduced first implementation; Slint can't nest modal loops. |
| Issue tracker | Bug-ID field, `bugtraq.logregex` links in the log, COM `IBugTraqProvider`. | Bug-ID field in the Commit dialog (label, append/prepend, number check, `/bugid:`); no log links, no providers. | Providers are Windows COM. |

## Keyboard

Spec: `docs/spec/spec-keyboard.md`. Win32 dialog behaviour (Enter = default button, Escape = Cancel, Alt+letter access keys, Space on tab stops) is rebuilt on Slint by `ui/keys.slint`.

| Area | TortoiseGit | KoopaGit | Reason |
|---|---|---|---|
| Access keys on macOS | Alt+letter on every `&` caption. | Off on macOS. | Option+letter types characters on macOS, and macOS apps have no mnemonics. |
| Access-key underlines | Shown while Alt is held (or always, per Windows setting). | Never drawn; the keys work without the underline. | Slint text cannot underline one letter of a widget caption. |
| Plain-letter mnemonics | A letter alone activates a mnemonic while a non-edit control has focus. | Only Alt+letter. | Needs the focused control's type, which Slint does not expose. |
| Commit Alt+M / Alt+F | Accelerators (work everywhere). | Routed as access keys, so off on macOS. | Same table as the other access keys. |
| Enter on a focused check box | Presses the default button. | Toggles the check box. | Slint's standard CheckBox handles Return itself. |
| Radio groups | Tab reaches the checked radio of a group; arrow keys move within the group. | Every radio is a tab stop; Space selects. No arrow keys. | KoopaGit's radios are drawn by hand; Slint has no group focus navigation. |
| Access keys on conditional controls | Focus moves to the control. | Controls that only exist in some modes (Create Branch/Tag Trac&k, &Switch, Si&gn, &Push; &HEAD; First Start Wizard labels; Add's "Include &ignored files") toggle without taking focus, or have no access key. | Slint cannot reference elements created by `if`. |
| Shared access-key letters | Win32 cycles focus between the controls sharing a letter. | Same where implemented (Merge Alt+M, Rebase Alt+U/Alt+P, Sync Alt+L/Alt+H); elsewhere the first match wins (New Worktree &Commit/&Checkout). | — |
| Shift + diff/compare | Uses the alternative diff tool. | Ignored. | KoopaGit has no alternative diff tool. |
| File list | Native list view: type-ahead, PgUp/PgDn, Ctrl+arrow focus moves; Enter opens the conflict editor for unmerged rows and skips missing ones. | Arrows, Home/End (Shift extends), Ctrl+A, Ctrl+C / Ctrl+Shift+C (no trailing CRLF), Space, Delete / Shift+Delete (with a KoopaGit confirmation), Enter = the dialog's diff for every selected row. | Slint list built by hand; existing diff actions reused. |
| Commit message editor | Scintilla: Ctrl+Space autocomplete, Ctrl+Tab spelling, wrap-aware Home/End. | Slint TextEdit keys. Tab moves focus as upstream. | No autocomplete or spell checker in KoopaGit. |
| Commit split button | Alt+O/R/P only when post actions are allowed; duplicate letters open the drop-down; F4 opens it. | Alt+O/R/P always; no F4. | No "no post actions" mode; the split button is a combo. |
| Log Ctrl+F / F3, RevisionGraph Ctrl+F | Modeless Find dialog. | Ctrl+F / F3 focus the log filter; no Find in the revision graph. | No Find dialog yet; the filter serves the same purpose. |
| Log Alt+Left/Right, Shift+Alt+Left/Right, Back/Forward keys, mouse X1/X2 | Selection history navigation. | Not available. | No back/forward navigation yet. |
| Log Escape | File filter → log filter → close. | Log filter → close. | KoopaGit has no file filter under the changed-files list. |
| Log Ctrl+C / Ctrl+V in the jump or date edits | Copy commits / jump to a pasted hash. | The edits copy and paste text. | Slint LineEdits handle the standard shortcuts first. |
| RefLog Ctrl+F / F3, "&Search..." | Find dialog. | Not available. | No find in KoopaGit's RefLog. |
| Browse References F2 | In-place label edit. | Opens KoopaGit's inline rename prompt; also fires from the ref tree. | Tree and list share one focus scope; no in-place edit. |
| Repository Browser tree | Keyboard navigable. | Only the file list takes focus. | Not implemented yet. |
| Sync | Shift+Pull/Push/Submodule/Stash open the standalone dialogs; Escape aborts a running command. | No Shift variants; Escape does nothing while git runs. | Sync has no Shift-click handling and no abort. |
| Rebase | Shift+click Up/Down moves to top/bottom; Tab from the message or output view jumps to Continue; the split-all button runs its current entry. | Shift+U / Shift+D only from the keyboard; Tab follows the normal order; the split-all button opens its menu. | Slint Button clicks carry no modifiers; the button is a menu. |
| Progress post-command keys | Accelerators on the menu-button entries; duplicates open the drop-down. | Alt+letter on the separate post buttons; the first match wins. | KoopaGit shows separate buttons. |
| Message box default | Some boxes default to the second button (e.g. "&Remove"/"&Abort" → Abort). | Same for message boxes (`msgbox::ask_default`). Questions shown in a dialog's notice bar have no default button; Enter goes to the dialog. | The notice bar is not a separate window with its own Enter handling. |
| History combos | Shift+Delete removes the highlighted history entry. | Not available. | No history removal callback. |
| CommitIsOnRefs | Ctrl+A selects all refs; Ctrl+C copies the selection with CRLF. | Single selection; Ctrl+C copies one ref. | List is single-selection. |
| Settings | Advanced page F2 edits a value; page access keys (&Language, C&heck now); Ctrl+Tab walks the page tree. | Values are always editable; rows have no access keys; Ctrl+Tab / Ctrl+PgDn walk the flat page list. | Pages are generated from data without `&` captions. |
| KoopaGitMerge / KoopaGitUDiff / KoopaGitBlame | Find (Ctrl+F, F3, Ctrl+F3), Go to line (Ctrl+G), Open (Ctrl+O), Save as, Reload (Ctrl+R/F5), Undo/Redo, text selection and clipboard keys, inline-difference keys (Ctrl+Alt+Left/Right), Ctrl+M/E/D/U/L/T/P view toggles, F6 pane switching, Ctrl+Tab, Print. | Not available. Implemented: next/previous difference and conflict, use-block keys, Save (Ctrl+S), exit, scrolling; Blame Ctrl+C copies the current line. | The viewers lack those features. |
| Viewer arrow/Home/End keys | Move a caret. | Scroll the view (Blame: move the current line); Home/End act as Ctrl+Home/End. | The viewers have no caret. |
| Viewer exit | Esc / Ctrl+W / Ctrl+Q ask to save unsaved changes. | With unsaved changes (diff edits, conflict choices) the exit keys do nothing; Save or the Close button decide. Without changes they exit. | No save prompt in the viewers yet; an exit key must not drop work silently. |
| Conflict editor difference keys | Stop at every difference. | Stop at conflict blocks only. | KoopaGit shows only conflict blocks. |
