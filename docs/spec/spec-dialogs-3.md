# TortoiseGit spec, part 3: remaining commands and settings pages

Paths are relative to upstream `src/`. `RC` = `Resources/TortoiseProcENG.rc`,
`LLRC` = `Resources/TortoiseLoglistCommon.rc2`, `TP` = `TortoiseProc/`.
Registry keys are under `HKCU\Software\TortoiseGit\` (koopagit: `settings.ini`).

## 1. Revision graph (`/command:revisiongraph`)

- Dialog `IDD_REVISIONGRAPH`, caption **"Revision Graph"** with menu bar `IDR_REVISIONGRAPH` (RC:1963-1969, 3398-3433):
  - File: `&Save graph as...`, `&Exit`.
  - View: `Zoom &in` (Ctrl-+), `Zoom &out` (Ctrl--), `Zoom to &100%`, `Fit height`, `Fit width`, `Fit graph`, `&Filter`, `Show Ove&rview`, `Show branchings and &merges`, `Show all tags`, `Arrows point towards merges`.
  - Git: `&Compare revisions`, `Compare &HEAD revisions`, `&Unified diff`, `Unified &diff of HEAD revisions` (greyed until two nodes are selected).
- Data (`TP/RevisionGraph/RevisionGraphDlgFunc.cpp:185-310`): log with `--simplify-by-decoration` (+ `--sparse` when "Show branchings and merges"), over all branches by default (`LOG_INFO_ALL_BRANCH`), optionally only local branches / current branch / a From..To range from the filter dialog.
  Then history is rewritten: a commit is **removed** when it has no label (or only tag labels while "Show all tags" is off), exactly one parent, exactly one child, and that child has exactly one parent; the child is re-linked to the removed commit's parent. Labeled commits, merges and fork points remain.
- Node context menu (`TP/RevisionGraph/RevisionGraphWnd.cpp:1120-1315`), in order:
  1. `Show &log` (RC:3946) → one node: `/command:log /path:<p> /endrev:<h> /rev:<h>`; two nodes: `/startrev:<h1> /endrev:<h2>`.
  2. One node only: `&Browse repository` (LLRC:81) → `/command:repobrowser /rev:<ref>`.
  3. One node: local branch names on the node (current branch excluded) → one: `&Switch/Checkout to "<b>"` (LLRC:106) = `git checkout <b>`; several: submenu `&Switch/Checkout to` ▸ names; none: `Sw&itch/Checkout to this...` (LLRC:92) for the first remote branch/tag → Switch dialog.
  4. One node: `Copy ref names` (RC:3893): ref names (or hash) joined by CRLF.
  5. One node with refs: one ref → `&Delete <ref>` (LLRC:104); several → `&Delete branch/tag` (LLRC:103) ▸ each ref + `All` (RC:3621).
  6. One node: `Compare &HEAD revisions` (RC:3683), `Unified &diff of HEAD revisions` (RC:3684), `Compare with &working tree` (LLRC:30).
  7. Two nodes: `&Compare revisions` (RC:3677), `&Unified diff` (RC:3678).

## 2. Send Mail (`/command:sendmail`)

- `CAppUtils::SendPatchMail` (`TP/AppUtils.cpp:2110-2138`); dialog `IDD_SENDMAIL`, caption **"Send Patch"** (RC:1658-1677): group `Mail` with `To:`, `CC:`, `Subject:`; checkboxes `Patch As Attachment`, `Combine One Mail`; link `eMail settings`; checkable list of patch files (all checked); `Send` / `Cancel` / `Help`.
- `Subject:` is enabled only with Combine (`TP/SendMailDlg.cpp:134-146`). Addresses `;`-separated; history `TortoiseProc\EmailAddress\email*`.
- OK with empty To and CC (unless delivery = MAPI) → "Email Address To and CC can't empty at the same time." (RC:4705).
- Per patch (`TP/SendMailPatch.cpp:35-91`): single mail → subject from the patch's `Subject:` header, body = patch (or attachment); combined → user subject, bodies concatenated (or attachment subjects listed).
- Delivery (`TP/Settings/SettingSMTP.cpp:36-40`): `TortoiseProc\SendMail\DeliveryType` 0 = SMTP direct, 1 = MAPI, 2 = SMTP configured; `Address`, `Port` (25), `Encryption` (0 none / 1 STARTTLS / 2 SSL), `AuthenticationRequired`.

## 3. git-svn

| Command | Behaviour | Ref |
|---|---|---|
| `svndcommit` | Unless git config `svn.rmdir` is set, dialog **"SVN Commit Type"**: group `DCommit Type` radios `Normal &SVN Commit` / `&Git Style Commit (--rmdir)`, `Remember selection ("svn.rmdir" option)` (writes `svn.rmdir` true/false). Dirty tree → "The current working tree is not clean.\nDo you want to stash the changes?" Stash/Abort → `git stash`. Then `git svn dcommit [--rmdir]`; after success with stash: "Do you want to stash pop now?" → `git stash pop`. | `TP/AppUtils.cpp:3142-3229`, RC:1225-1239, 4721, 4738 |
| `svnrebase` | Same stash prompt. Upstream = `svn-remote.svn.fetch` value after `:refs/` (error "Could not get \"svn-remote.svn.fetch\" config value."). `git svn fetch` (auto-close if no errors). If upstream == HEAD: "Everything updated."; if fast-forward: `git reset --hard --end-of-options <up> --` + "Fast Forward"; else Rebase dialog onto upstream (post button Show log). Stash pop question afterwards. | `TP/Commands/SVNRebaseCommand.cpp:30-174` |
| `svnfetch` | Needs `svn-remote.svn.fetch` (else "Found no SVN remote."). `git svn fetch`; if the upstream moved, post buttons `Pulled Diff` (`showcompare /revision1 /revision2`) and `Pulled Log` (`log /range:old..new`). | `TP/Commands/SVNFetchCommand.cpp:26-87` |
| `svnignore` | Dialog **"Import SVN Ignore"** group `Type`: `Import svn ignore file to .git/info/exclude file (file will be overwritten)` → `git svn show-ignore` written to `.git/info/exclude`; `Create .gitignore file` → `git svn create-ignore`. | `TP/Commands/SVNIgnoreCommand.cpp:26-58`, RC:1746-1757 |

## 4. Other commands

| Command | Behaviour | Ref |
|---|---|---|
| `daemon` | Warning "While running daemon, all data of this repository is exposed without authentication and/or encryption." Proceed/Abort (+ do-not-show `DaemonNoSecurityWarning`, only Proceed is remembered). Progress dialog pre-text lists `git://<ip>/` per local address (`git://localhost/` if none), runs `git daemon --verbose --export-all --base-path=<repo>`. | `TP/Commands/DaemonCommand.cpp:25-102`, RC:3868 |
| `pgpfp` | Message box with the TortoiseGit Release Signing Key fingerprint (`74A2 1AE3 01B3 CA5B D807  2F5E F7F1 7B3F 9DD9 539E`). | `TP/Commands/Command.cpp:441-445` |
| `commitisonrefs` | Dialog **"References commit is on"** (RC:2198-2210): commit edit + ref picker button (Browse refs / Log / RefLog), subject line, `Show log` button, ref list, `Filter:`. List = branches and tags containing the commit (`GetRefsCommitIsOn(..., BRANCH_ALL)`). List menu: Compare two, Unified diff, Show log of A..B / A...B, Show log, Browse repository, Compare with working tree, Copy. | `TP/CommitIsOnRefsDlg.cpp` |
| `firststart` | Wizard "First Start Wizard - TortoiseGit": Start (hint), Language (`&Language:` combo + download link), Git (`&Git.exe Path:` + `...`, `&Extra PATH:`, version + `C&heck now`), User (`&Name:`, `&Email:`, `&Don't store these settings now.`, stored to global gitconfig), Authentication (SSH client combo, credential helper combo, `&Don't store these settings now.`). | RC:1404-1410, 2212-2277 |
| `updatecheck` | `IDD_CHECKFORUPDATES`: downloads the version file and offers updates. | RC:548 |
| `dropmove` | `/droptarget`; source and target must be in the same working tree ("Target and source must be in the same working tree!"). `/rename` with one item asks "Move and rename". Existing target asks "Move: New name for %s". Per item `git mv -- <src> <dst>`. | `TP/Commands/DropMoveCommand.cpp`, RC:3949, 4447, 4697 |
| `dropcopyadd` | Copies each item into `/droptarget` (existing file: "The file\n%s\nalready exists! Do you want to overwrite it?" Overwrite/Keep/Cancel), directories recursively (nested repos asked about), then the Add progress command on the copied files. | `TP/Commands/DropCopyAddCommand.cpp` |
| `dropcopy` | Compiled out upstream ("Command not implemented"). | `TP/Commands/Command.cpp` |

## 5. Settings pages (previously stubs)

| Page | Controls (label → key, default) | Ref |
|---|---|---|
| Colors 1 | group `Status and action colors`: `possible or real conflict/obstructed` → `Colors\Conflict` RGB(255,0,0); `added files` → `Colors\Added` (100,0,100); `missing/deleted/replaced` → `Colors\Deleted` (100,0,0); `merged` → `Colors\Merged` (0,100,0); `modified/copied` → `Colors\Modified` (0,50,160); `renamed` → `Colors\Renamed` (0,0,255). group `Revision graph`: `Note node` → `Colors\NoteNode` (160,160,0); `Unknown ref-types` → `Colors\OtherRef` (224,224,224); `Use local branch color for current branch` → `TortoiseProc\Graph\RevGraphUseLocalForCur`. `Dark theme`. `Restore Default`. | RC:1412-1439, `TP/Colors.cpp:25-53` |
| Colors 2 | group `Log Graphic`: `Current Branch` (200,0,0), `Local Branch` (0,195,0), `Remote Branch` (255,221,170), `Tag` (255,255,0); `Filter match` (200,0,0). | RC:835-852 |
| Colors 3 | group `Log Branch Line`: `LINE1`..`LINE8` → `Colors\BranchLine1..8` = black, red, green, blue, (128,128,128), (128,128,0), (0,128,128), (128,0,128); `Line width` → `TortoiseProc\Graph\LogLineWidth` 2; `Node size` → `TortoiseProc\Graph\LogNodeSize` 10. | RC:1441-1470, `TP/Settings/SettingsColors3.cpp:89-92` |
| Alternative editor | "Configure alternative editor. Preferably one which can handle *nix line endings." radios `&Notepad` / `&Custom` + path → `AlternativeEditor` (`#` prefix = built-in). | RC:1784-1795 |
| Hook Scripts | list (Hook Type, Working Tree Path, Command Line, Wait, Show, Enabled) + `Remove`, `Edit...`, `&Copy`, `Add...`. Hook dialog **"Configure Hook Scripts"**: `&Enable`, `Hook Type:` combo, `Run for this repository`, `Run when working tree path is under:` + `...`, `Command Line To Execute:` + `...`, `Wait for the script to finish`, `Hide the script while running`. Stored in `hooks` as 5 lines per hook: type, path (`!` prefix = disabled), command, `true`/`false`, `show`/`hide`. Types: `start_commit_hook`, `pre_commit_hook`, `post_commit_hook`, `pre_push_hook`, `post_push_hook`, `pre_rebase_hook`. Lookup walks from the working tree root up through parent folders, then path `*`. Arguments: start/pre-commit `<pathfile> <msgfile> <cwd>` (msg file read back on exit 0); post-commit `<cwd> <true|false amend>`; pre/post-push `<errorfile> <cwd>`; `%root%` replaced by the project root. | RC:890-922, `Utils/Hooks.cpp:75-483` |
| Issue Tracker Config | config-source selector (Effective/Local/Project/Global/System) + `bugtraq.url`, `bugtraq.warningifnoissue` (combo), `bugtraq.message`, `bugtraq.append` (combo), `bugtraq.label`, `bugtraq.number` (combo), `bugtraq.logregex` (+ `Test`), each with `inherit`; provider uuid win32/x64/parameters; `Save to:`. | RC:1005-1052 |
| Network | `&Enable Proxy Server`, group `Proxy Settings`: `Server &address:`, `&Port:`, `&Username:`, `&Password:` (stored under `Git\Servers\global\http-proxy-*`, written to global `http.proxy` as `http://[user[:pass]@]host[:port]`, unset when disabled); group `SSH`: `&SSH client:` + `Browse...` → `SSH`. | RC:420-439, `TP/Settings/SetProxyPage.cpp:33-37, 224-270` |
| Email | `Delivery:` (SMTP direct / MAPI / SMTP configured), `SMTP Server:`, `Port:`, `From`, `Encryption` (None/STARTTLS/SSL), `SMTP Server requires authentication`, `Credentials` `Login:` + `&Store credentials...` / `&Clear`. | RC:441-463, `TP/Settings/SettingSMTP.cpp` |
| TortoiseGitBlame | group `Colors`: `Recently modified lines` → `BlameNewColor` RGB(255,255,80), `Older lines` → `BlameOldColor` (255,255,255), `Restore Default`; group `Font`: `&Font:` → `BlameFontName` "Consolas", size `BlameFontSize` 10, `Tab size:` → `BlameTabSize` 4; group `Blame`: `&Detect moved or copied lines:` → `TortoiseGitBlame\Workspace\DetectMovedOrCopiedLines` (disabled), `Within a file:` / `Between files:` character counts, `Ignore whitespace`, `Only consider first parents on blame`; group `Log`: `Show complete log` (1), `Follow renames` (0). | RC:949-980, `TP/Settings/SettingsTBlame.cpp:43-54` |
| TortoiseGitUDiff | group `Colors` (Foreground/Background) for `Diff command`, `Diff position`, `Diff header`, `Diff comment`, `Diff added lines`, `Diff removed lines` → `UDiffFore*Color` / `UDiffBack*Color`; `Restore Default`; group `Font`: `UDiffFontName` "Consolas", `UDiffFontSize` 10, `Tab size:` `UDiffTabSize` 4; note "These settings also apply to the Patch Viewer dialog." | RC:2117-2151, `TP/Settings/SettingsTUDiff.cpp:33-49` |
