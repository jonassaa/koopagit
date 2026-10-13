# TortoiseGit shell integration: behavioural spec

Extracted from the TortoiseGit source (snapshot in `scratchpad/upstream/tortoisegit`). All `file:line` references are relative to that tree.
Abbreviations: `CM` = `src/TortoiseShell/ContextMenu.cpp`, `MI` = `src/TortoiseShell/MenuInfo.cpp`, `SC` = `src/TortoiseShell/ShellCache.cpp`, `SCh` = `src/TortoiseShell/ShellCache.h`, `G` = `src/TortoiseShell/Globals.h`, `IO` = `src/TortoiseShell/IconOverlay.cpp`, `RC` = `src/TortoiseShell/resourceshell.rc`.

---

## 0. Data model

### 0.1 ITEMIS_* flags (G:137-162)

| Flag | Value | Flag | Value |
|---|---|---|---|
| ONLYONE | 0x00000001 | PATCHFILE | 0x00001000 |
| EXTENDED | 0x00000002 | PATCHINCLIPBOARD | 0x00008000 |
| INGIT | 0x00000004 | PATHINCLIPBOARD | 0x00010000 |
| CONFLICTED | 0x00000008 | TWO | 0x00020000 |
| FOLDER | 0x00000010 | SUBMODULECONTAINER | 0x00040000 |
| FOLDERINGIT | 0x00000020 | GITSVN | 0x00080000 |
| NORMAL | 0x00000040 | STASH | 0x00100000 |
| IGNORED | 0x00000080 | WCROOT | 0x00200000 |
| INVERSIONEDFOLDER | 0x00000100 | BISECT | 0x00400000 |
| ADDED | 0x00000200 | BAREREPO | 0x00800000 |
| DELETED | 0x00000400 | SUBMODULE | 0x01000000 |
| (SHORTCUT 0x2000 unused) | | MERGEACTIVE | 0x02000000 |
| | | HASDIFFLATER | 0x04000000 |
| | | INACCESSIBLE | 0x08000000 |

### 0.2 Menu-entry bits `TGitContextMenuEntries` (64-bit, G:22-83)

Each entry's bit is used for three masks: top-level placement, extended-only, and hide. Several menu rows share a bit (all Bisect rows share `Bisect`; Merge and MergeAbort share `Merge`; IgnoreSub and DeleteIgnoreSub share `Ignore`).

```
Sync 0x2  Commit 0x4  Add 0x8  Revert 0x10  Cleanup 0x20  Resolve 0x40  Switch 0x80
Sendmail 0x100  Export 0x200  CreateRepo 0x400  Branch 0x800  Merge 0x1000  Remove 0x2000
Rename 0x4000  SubmoduleUpdate 0x8000  Diff 0x10000  Log 0x20000  Conflicteditor 0x40000
RefBrowser 0x80000  ShowChanged 0x100000  Ignore 0x200000  RefLog 0x400000  Blame 0x800000
RepoBrowser 0x1000000  ApplyPatch 0x2000000  RemoveKeep 0x4000000  SVNRebase 0x8000000
SVNDCommit 0x10000000  SVNIgnore 0x40000000  LogSubmodule 0x1_00000000  PrevDiff 0x2_00000000
ClipPaste 0x4_00000000  Pull 0x8_00000000  Push 0x10_00000000  Clone 0x20_00000000
Tag 0x40_00000000  FormatPatch 0x80_00000000  ImportPatch 0x100_00000000  DiffLater 0x200_00000000
Fetch 0x400_00000000  Rebase 0x800_00000000  StashSave 0x1000_00000000  StashApply 0x2000_00000000
StashList 0x4000_00000000  SubmoduleAdd 0x8000_00000000  SubmoduleSync 0x1_0000_00000000
StashPop 0x2_0000_00000000  DiffTwo 0x4_0000_00000000  Bisect 0x8_0000_00000000
Inaccessible 0x10_0000_00000000  SVNFetch 0x80_0000_00000000  RevisionGraph 0x100_0000_00000000
Daemon 0x200_0000_00000000  Worktree 0x400_0000_00000000  LFS 0x1000_0000_00000000
Settings 0x2000_0000_00000000  Help 0x4000_0000_00000000  About 0x8000_0000_00000000
```

### 0.3 Visibility rule (CM:2322-2340, MenuInfo.h:131-138)

Each table row has four `{yes, no}` pairs. A row is a candidate if **any** pair matches (`ShouldInsertItem`). A pair matches if:
- `yes && no`: all `yes` bits set **and** all `no` bits clear;
- only `yes`: all `yes` bits set;
- only `no`: all `no` bits clear;
- `{0,0}`: never matches.

---

## 1. Context menu (classic Explorer menu: `QueryContextMenu`, CM:830-1116)

### 1.1 Early exits (in order)
1. Drop handler instance: build the drag-and-drop menu instead (§1.6) (CM:838-841).
2. `CMF_DEFAULTONLY` set: nothing (CM:843).
3. No files and no folder: nothing (CM:846).
4. Menu type not NORMAL/EXPLORE/VERBSONLY: nothing (CM:849).
5. Background folder is a "illegal" known folder (Recycle Bin, CD burning, Favorites, Start Menu, Network, Control Panel, Fonts, History, Recent, SendTo, Printers, ...): nothing (CM:852, 1889-1924).
6. Any selected path or the folder starts with `::{` (namespace extension): nothing (CM:855-869).
7. **HideMenusForUnversionedItems**: if set and Shift is **not** held (`CMF_EXTENDEDVERBS` absent), and none of `INGIT|INVERSIONEDFOLDER|FOLDERINGIT|BAREREPO|INACCESSIBLE` is set: nothing (CM:871-875).
8. The folder is inside a `.git` admin dir: nothing (CM:878).
9. Then `ITEMIS_EXTENDED` is set if Shift is held (`CMF_EXTENDEDVERBS`) (CM:881). `ITEMIS_HASDIFFLATER` is set if the `DiffLater` registry value is non-empty (CM:884-886).
10. Shortcut (`CMF_VERBSONLY`) with exactly one file whose target is not inside a git working tree: nothing (CM:888-900).
11. The menu already holds our folder submenu (checked through the `dwItemData` marker): nothing, so the menu is never added twice (CM:902-921).

### 1.2 Build algorithm (CM:924-1115)
- Always insert a **separator** at the insertion point in the parent menu first (CM:941).
- Walk `menuInfo[]` in table order (MI:27-258). For each row:
  - **Separator row**: set `bAddSeparator=true` if the submenu is non-empty or an entry was added since the last separator (CM:947-958). Separators are lazy, so there is never a leading, trailing or doubled separator.
  - `bInsert = ShouldInsertItem(row)`.
  - **Extended-only**: if the row's bit is in `menuex` (ContextMenuExtEntries) and `ITEMIS_EXTENDED` is not set, skip it. `Inaccessible` is never skipped this way (CM:962-966).
  - **Hidden mask**: if every bit of the row's id is in the ContextMenuEntriesMask (HKLM|HKCU), skip it. Rows with id `None` (value 0) are skipped here as well (CM:968-969).
  - If `!bInsert`, skip it.
  - `bIsTop = topmenu & row.id` (ContextMenuEntries). If `bAddSeparator && bMenuEntryAdded && !bIsTop`, insert a separator into the submenu (CM:974-985).
  - Ignore rows (`IgnoreSub` / `DeleteIgnoreSub`) build a nested **Ignore submenu** (§1.4). The `LFSMenu` row builds a nested **LFS submenu** (§1.5).
  - Every other row is inserted into the main menu if top (prefixed `"Git "`), otherwise into the TortoiseGit submenu. `Inaccessible` is **always top-level** (CM:1007-1011).
- **Top-level entries** go into the parent menu, in table order, directly after the leading separator and **before** the TortoiseGit submenu item.
- When the submenu ends up empty (`bMenuEmpty`): add a trailing separator (CM:1036-1050). The result is `sep, [top entries], sep`.
- Otherwise insert the popup **"&TortoiseGit"** (`IDS_MENUSUBMENU`, RC:238), then a trailing separator (CM:1055-1110). The popup icon depends on the selection (CM:1068-1098):
  - background or single folder: `IDI_MENUFOLDER`;
  - single file: `IDI_MENUFILE`;
  - shortcut: `IDI_MENULINK`;
  - multiple items: `IDI_MENUMULTIPLE`;
  - fallback: `IDI_APP`.
- Icons are shown only if `ShowContextMenuIcons` (default 1) (CM:943).
- Labels of top-level items get the `"Git "` prefix. If `ShellMenuAccelerators=0`, `&` is stripped from top-level labels and from the submenu name (CM:511-524, 1056-1062).

Special label rules (`InsertGitMenu`, CM:505-643):
- **Commit**: if the path is a submodule (`GetAdminDirMask & SUBMODULE`), the label becomes `"&Commit submodule..."`. When the current branch can be read from HEAD, the trailing dots are replaced so the label reads `Commit -> "<branch>"...`. A detached 40-hex HEAD is shortened to `<short-hash>...`, names over 64 characters become `Left(64)+"..."`, and `&` in the branch name is escaped (CM:529-588).
- **DiffLater**: if a path is already stored, the label becomes `Diff with "<path compacted to 39 chars>"` (`IDS_MENUDIFFNOW`, RC:383) (CM:590-603).

### 1.3 Entry table (exact table order; `---` = separator row)

Columns:
- **Cond** = the yes/no pairs, `+X` = yes, `-X` = no; pairs joined by `OR`.
- **Proc** = the TortoiseGitProc `/command:` and its arguments. `path(f)` = `/path:"<first selected item, else folder>"`; `path(d)` = `/path:"<folder_>"` (selected folder or background folder, ignoring selected files); `pathfile` = `/pathfile:"<tmp>" /deletepathfile` (tmp lists all selected items, or the folder if nothing is selected); `pathfile(dirs)` = the same list with directories only.
- Every launch appends ` /hwnd:<hex>` (CM:1647-1653).
- **Default** = placement with the default masks: **TOP** = main menu, **sub** = TortoiseGit submenu, **ext** = only with Shift. **W11** = included in the Windows 11 modern menu by default.

| # | Command (MI line) | Bit | English label (RC line) | Proc | Cond | Default |
|---|---|---|---|---|---|---|
| 1 | Inaccessible (MI:29) | Inaccessible | `Inaccessible...` (RC:448) | `inaccessible` path(d) | +INACCESSIBLE -(INGIT\|FOLDERINGIT\|BAREREPO) | always TOP (forced), never ext-filtered; W11 forced |
| 2 | Clone (MI:32) | Clone | `Clone...` (RC:421) | `clone` path(d) | (+FOLDER -(INGIT\|FOLDERINGIT\|BAREREPO\|INACCESSIBLE)) OR (+FOLDER\|IGNORED) OR (+FOLDER\|EXTENDED) | **TOP**, W11 |
| 3 | Pull (MI:35) | Pull | `&Pull...` (RC:419) | `pull` pathfile(dirs) | (+FOLDERINGIT\|ONLYONE -(BISECT\|MERGEACTIVE)) OR (+WCROOT -(BISECT\|MERGEACTIVE)) | sub, W11 |
| 4 | Fetch (MI:38) | Fetch | `F&etch...` (RC:281) | `fetch` pathfile(dirs) | (+FOLDERINGIT\|ONLYONE) OR (+BAREREPO) OR (+WCROOT) | sub |
| 5 | Push (MI:41) | Push | `Pus&h...` (RC:420) | `push` pathfile(dirs) | (+FOLDERINGIT\|ONLYONE) OR (+BAREREPO) OR (+WCROOT) | sub, W11 |
| 6 | Sync (MI:44) | Sync | `Sync...` (RC:297) | `sync` path(d) ` /seq:<n>` (n = per-process counter, CM:1220-1228) | +FOLDERINGIT\|ONLYONE -(BISECT\|MERGEACTIVE) | **TOP**, W11 |
| --- | separator (MI:48) | | | | | |
| 7 | Commit (MI:50) | Commit | `&Commit...` (RC:208) (see §1.2) | `commit` pathfile | (+INGIT) OR (+FOLDERINGIT) | **TOP**, W11 |
| 8 | SVNDCommit (MI:53) | SVNDCommit | `SVN DCommit...` (RC:295) | `svndcommit` path(f) | (+INGIT\|GITSVN -(BISECT\|MERGEACTIVE)) OR (+FOLDERINGIT\|GITSVN -(BISECT\|MERGEACTIVE)) | sub |
| 9 | SVNRebase (MI:56) | SVNRebase | `SVN Rebase` (RC:319) | `svnrebase` path(d) | +FOLDERINGIT\|GITSVN\|ONLYONE -(BISECT\|MERGEACTIVE) | sub |
| 10 | SVNDFetch (MI:59) | SVNFetch | `SVN Fetch` (RC:368) | `svnfetch` path(d) | +FOLDERINGIT\|GITSVN\|ONLYONE | sub |
| 11 | SVNIgnore (MI:62) | SVNIgnore | `Import SVN Ignore ...` (RC:300) | `svnignore` path(d) | +FOLDERINGIT\|GITSVN\|ONLYONE | sub, **ext** |
| --- | separator (MI:65) | | | | | |
| 12 | Diff (MI:67) | Diff | `&Diff` (RC:390) | `diff /path:"a"` [`/path2:"b"` if exactly 2 selected; folder if none]; `/alternative` if Shift held at invoke (CM:1332-1349) | (+INGIT\|ONLYONE) OR (+TWO -FOLDER) | sub, W11 |
| 13 | DiffLater (MI:70) | DiffLater | `Diff later` (RC:381) / `Diff with "%s"` | see §1.7 | +ONLYONE -FOLDER (no git requirement) | sub |
| 14 | PrevDiff (MI:73) | PrevDiff | `&Diff with previous version` (RC:412) | `prevdiff` path(f) [+`/alternative` w/ Shift] | +INGIT\|ONLYONE -ADDED | sub |
| --- | separator (MI:76) | | | | | |
| 15 | Log (MI:78) | Log | `Show &log` (RC:394) | `log` path(f) | (+INGIT\|ONLYONE -ADDED) OR (+FOLDER\|FOLDERINGIT\|ONLYONE -ADDED) OR (+FOLDERINGIT\|ONLYONE -ADDED) OR (+BAREREPO) | sub, W11 |
| 16 | LogSubmoduleFolder (MI:81) | LogSubmodule | `Show log of this folder` (RC:409) | `log` path(f) `/submodule` | +FOLDERINGIT\|ONLYONE\|WCROOT\|SUBMODULE | sub |
| 17 | RefLog (MI:84) | RefLog | `Show &RefLog` (RC:287) | `reflog` path(d) | (+FOLDERINGIT\|ONLYONE) OR (+BAREREPO) | sub |
| 18 | RefBrowser (MI:87) | RefBrowser | `Browse References` (RC:317) | `refbrowse` path(d) | (+FOLDERINGIT\|ONLYONE) OR (+BAREREPO) | sub |
| 19 | Daemon (MI:90) | Daemon | `Daemon` (RC:414) | `daemon` path(f) | same as Log | sub |
| 20 | RevisionGraph (MI:93) | RevisionGraph | `Revision &graph` (RC:226) | `revisiongraph` path(f) | same as Log | sub |
| 21 | RepoBrowse (MI:96) | RepoBrowser | `&Repo-browser` (RC:377) | `repobrowser` path(d) | (+FOLDERINGIT\|ONLYONE) OR (+BAREREPO\|ONLYONE) | sub, W11 |
| 22 | ShowChanged (MI:99) | ShowChanged | `Check for modi&fications` (RC:354) | `repostatus`: pathfile if >1 selected, else path(f) (CM:1440-1445) | (+INGIT) OR (+FOLDER\|FOLDERINGIT) | sub |
| 23 | Rebase (MI:102) | Rebase | `Rebase...` (RC:282) | `rebase` path(d) | +FOLDERINGIT\|ONLYONE -(BISECT\|MERGEACTIVE) | sub |
| 24 | StashSave (MI:105) | StashSave | `Stash changes` (RC:283) | `stashsave` path(f) | +INGIT\|ONLYONE -MERGEACTIVE | sub, W11 |
| 25 | StashApply (MI:107) | StashApply | `Stash Apply` (RC:284) | `stashapply` path(d) | +FOLDERINGIT\|ONLYONE\|STASH | sub, **ext** |
| 26 | StashPop (MI:109) | StashPop | `Stash Pop` (RC:299) | `stashpop` path(d) | +FOLDERINGIT\|ONLYONE\|STASH | sub |
| 27 | StashList (MI:111) | StashList | `Stash List` (RC:289) | `reflog` path(d) `/ref:refs/stash` | +FOLDERINGIT\|ONLYONE\|STASH | sub |
| --- | separator (MI:114) | | | | | |
| 28 | BisectStart (MI:116) | Bisect | `Bisect start` (RC:239) | `bisect` path(d) `/start` | +FOLDERINGIT\|ONLYONE -(BISECT\|MERGEACTIVE) | sub |
| 29 | BisectGood (MI:119) | Bisect | `Bisect good` (RC:240) | `bisect` path(d) `/good` | +FOLDERINGIT\|ONLYONE\|BISECT | sub |
| 30 | BisectBad (MI:122) | Bisect | `Bisect bad` (RC:327) | `bisect` path(d) `/bad` | +FOLDERINGIT\|ONLYONE\|BISECT | sub |
| 31 | BisectSkip (MI:125) | Bisect | `Bisect skip` (RC:336) | `bisect` path(d) `/skip` | +FOLDERINGIT\|ONLYONE\|BISECT | sub |
| 32 | BisectReset (MI:128) | Bisect | `Bisect reset` (RC:335) | `bisect` path(d) `/reset` | +FOLDERINGIT\|ONLYONE\|BISECT | sub |
| --- | separator (MI:131) | | | | | |
| 33 | ConflictEditor (MI:133) | Conflicteditor | `&Edit conflicts` (RC:396) | `conflicteditor` path(f) [+`/alternative` w/ Shift] | +INGIT\|CONFLICTED -FOLDER | sub, W11 |
| 34 | Resolve (MI:136) | Resolve | `Res&olve...` (RC:213) | `resolve` pathfile | (+INGIT\|CONFLICTED) OR (+INGIT\|FOLDER) OR (+FOLDERINGIT) | sub |
| 35 | MergeAbort (MI:139) | Merge | `Abort &Merge` (RC:374) | `merge` path(f) `/abort` | (+INGIT\|MERGEACTIVE) OR (+FOLDERINGIT\|MERGEACTIVE) | sub, W11 (shares Merge bit) |
| 36 | Rename (MI:142) | Rename | `Re&name...` (RC:330) | `rename` path(f) [+`/submodule` if SUBMODULE] | (+INGIT\|ONLYONE\|INVERSIONEDFOLDER -WCROOT) OR (+WCROOT\|SUBMODULE) | sub |
| 37 | Remove (MI:145) | Remove | `&Delete` (RC:332) | `remove` pathfile [+`/submodule` if SUBMODULE] | (+INGIT\|INVERSIONEDFOLDER -(ADDED\|WCROOT)) OR (+FOLDERINGIT\|WCROOT\|SUBMODULE) | sub |
| 38 | RemoveKeep (MI:148) | RemoveKeep | `&Delete (keep local)` (RC:427) | `remove` pathfile `/keep` | +INGIT\|INVERSIONEDFOLDER -(ADDED\|WCROOT) | sub |
| 39 | Revert (MI:151) | Revert | `Re&vert...` (RC:211) | `revert` pathfile | (+INGIT -NORMAL) OR (+FOLDERINGIT) | sub, W11 |
| 40 | Cleanup (MI:154) | Cleanup | `&Clean up...` (RC:212) | `cleanup` pathfile | +FOLDERINGIT\|FOLDER | sub |
| --- | separator (MI:157) | | | | | |
| 41 | LFSMenu (MI:159) | LFS | `LFS` (RC:342) popup | see §1.5 | +INVERSIONEDFOLDER\|INGIT, and the repo has `.git/lfs` | sub |
| --- | separator (MI:162) | | | | | |
| 42 | Switch (MI:167) | Switch | `S&witch/Checkout...` (RC:231) | `switch` path(d) | +FOLDERINGIT\|ONLYONE | sub, W11 |
| 43 | Merge (MI:170) | Merge | `&Merge...` (RC:232) | `merge` path(d) | +FOLDERINGIT\|ONLYONE -(BISECT\|MERGEACTIVE) | sub, W11 |
| 44 | Branch (MI:173) | Branch | `Create &Branch...` (RC:233) | `branch` path(d) | +FOLDERINGIT\|ONLYONE | sub |
| 45 | Tag (MI:175) | Tag | `Create &Tag...` (RC:278) | `tag` path(d) | +FOLDERINGIT\|ONLYONE | sub |
| 46 | Export (MI:178) | Export | `E&xport...` (RC:235) | `export` path(d) | (+FOLDERINGIT\|ONLYONE) OR (+BAREREPO) | sub |
| --- | separator (MI:181) | | | | | |
| 47 | CreateRepo (MI:183) | CreateRepo | `Create repositor&y here...` (RC:209) | `repocreate` path(d) | (+FOLDER -(INGIT\|FOLDERINGIT\|BAREREPO\|INACCESSIBLE)) OR (+FOLDER\|IGNORED) OR (+FOLDER\|EXTENDED -INGIT) | **TOP** |
| 48 | Add (MI:186) | Add | `&Add...` (RC:210) | `add` pathfile | (+INVERSIONEDFOLDER -INGIT) OR (+INGIT\|FOLDER) OR (+IGNORED) OR (+DELETED -(FOLDER\|ONLYONE)) | sub |
| 49 | Blame (MI:189) | Blame | `&Blame` (RC:365) | `blame` path(f) | +INGIT\|ONLYONE -(FOLDER\|ADDED) | sub, W11 |
| 50 | IgnoreSub (MI:192) | Ignore | `Add to &ignore list` (RC:356) popup | §1.4 | +INVERSIONEDFOLDER -(IGNORED\|INGIT\|WCROOT) | sub |
| 51 | DeleteIgnoreSub (MI:195) | Ignore | `Delete and add to &ignore list` (RC:220) popup | §1.4 | +INVERSIONEDFOLDER\|INGIT -(IGNORED\|WCROOT) | sub |
| --- | separator (MI:202) | | | | | |
| 52 | Worktree (MI:204) | Worktree | `Worktrees` (RC:224) | `worktreelist` path(d) | (+FOLDERINGIT\|ONLYONE) OR (+BAREREPO) | sub |
| 53 | SubmoduleAdd (MI:207) | SubmoduleAdd | `Submodule Add...` (RC:285) | `subadd` path(d) | +FOLDERINGIT\|ONLYONE | sub |
| 54 | SubmoduleUpdate (MI:210) | SubmoduleUpdate | `Submodule &Update...` (RC:334) | `subupdate` pathfile [+`/bkpath:"<folder>"` if folder flags have SUBMODULECONTAINER, or item flags have SUBMODULECONTAINER&WCROOT&ONLYONE] (CM:1238-1246) | +FOLDERINGIT\|SUBMODULECONTAINER | sub |
| 55 | SubmoduleSync (MI:213) | SubmoduleSync | `Submodule Sync` (RC:286) | `subsync` pathfile [+`/bkpath` same rule] | +FOLDERINGIT\|SUBMODULECONTAINER | sub, **ext** |
| --- | separator (MI:216) | | | | | |
| 56 | FormatPatch (MI:218) | FormatPatch | `Create Patch Serial...` (RC:279) | `formatpatch` path(d) | +FOLDERINGIT\|ONLYONE | sub |
| 57 | ImportPatch (MI:222) | ImportPatch | `Apply Patch Serial...` (RC:280) | `importpatch` pathfile | (+PATCHFILE) OR (+FOLDERINGIT\|ONLYONE) | sub |
| 58 | ApplyPatch (MI:226) | ApplyPatch | `Review/apply single &patch...` (RC:369) | launches **TortoiseGitMerge.exe**, see §1.7 | (+PATCHFILE\|ONLYONE) OR (+EXTENDED\|ONLYONE -FOLDER) | sub |
| 59 | Sendmail (MI:229) | Sendmail | `Send Mail...` (RC:315) | `sendmail` pathfile | (+PATCHFILE) OR (+EXTENDED -FOLDER) | sub |
| --- | separator (MI:237) | | | | | |
| 60 | Settings (MI:239) | Settings | `&Settings` (RC:236) | `settings` path(f) | (+FOLDER) OR (-FOLDER), i.e. always | sub, W11 |
| 61 | Help (MI:241) | Help | `&Help` (RC:400) | `help` (no path) | always | sub |
| 62 | About (MI:243) | About | `A&bout` (RC:237) | `about` (no path) | always | sub |
| - | SubMenu, SubMenuFile, SubMenuFolder, SubMenuLink, SubMenuMultiple (MI:248-257) | None | `&TortoiseGit` | n/a: these rows only supply the icon and text for the popup and are never inserted | | |

Commented out and therefore not shipped: Copy (MI:164), UnIgnoreSub (MI:199; the `ignoredprops` data it needs is never filled), ClipPaste/Paste (MI:232-235).

Status-bar help text comes from the row's `menuDescID` (e.g. `IDS_MENUDESCCOMMIT` "Commits your changes to the repository"). The fallback is `IDS_MENUDESCDEFAULT` "no description for this command is available" (CM:1658-1680, RC:241-258).

### 1.4 Ignore submenu (`InsertIgnoreSubmenus`, CM:1979-2262)
- Requires a file selection, not a folder background (CM:1987). The popup text is chosen in this order (CM:2225-2230):
  1. `ITEMIS_IGNORED`: "Remove from &ignore list";
  2. `ITEMIS_INGIT`: "Delete and add to &ignore list";
  3. otherwise: "Add to &ignore list".
- With IGNORED set, the unignore branch needs the `ignoredprops` data, which is always empty, so in practice **no popup appears**.
- Not ignored, **single item** (`ONLYONE`):
  - Item 1 is the item's file name. It runs `ignore` pathfile, plus `/delete` if INGIT.
  - Item 2 appears only if the item is not a folder and has an extension. Its label is `*.<ext>` and it runs `ignore` pathfile `/onlymask`, plus `/delete` if INGIT (CM:2055-2108, 1253-1267).
- Not ignored, **multiple items**:
  - Not in git: `"&Ignore %d items by name"` and `"Ignore %d items by &extension"`.
  - INGIT: `"Delete and &ignore %d items by name"` and `"Delete and ignore %d items by &extension"`.
  - With 16 or more items, the text says "multiple items" in place of a count, because Explorer passes at most 16 items to the extension (CM:2110-2208, RC:309,380,418,443-447).
- The popup goes to the main menu if the `Ignore` bit is in ContextMenuEntries, else into the submenu (CM:2234-2235).
- Windows 11: the sub-items are flattened and prefixed `"<popup text>: "`, with a separator before them (CM:2236-2249).

### 1.5 LFS submenu (`InsertLFSSubmenu`, CM:1926-1977)
- Shown only if `.git/lfs` exists for the folder or first file (CM:1937-1940; `TGitPath.cpp:897-906`) and at least one child matches. Children, each with its own conditions:
  - `Show locked files` (`lfslocks` pathfile): +FOLDERINGIT|ONLYONE.
  - `Lock` (`lfslock` pathfile): +INGIT|INVERSIONEDFOLDER -FOLDER.
  - `Unlock` (`lfsunlock` pathfile): same as `Lock`.
- The popup goes top or sub depending on the `LFS` bit (CM:1971-1972). On Windows 11 the children are added unconditionally (not gated by the Win11 mask) (CM:999-1000, 1950).

### 1.6 Drag-and-drop (right-drag) menu (`QueryDropContext`, CM:741-828)
Disabled when `EnableDragContextMenu=0` (default 1). Needs both dropped files and a target folder. "Same repo" means source and target work-tree roots are equal, or either one is unknown (CM:758).

Entries in this order, all prefixed "Git ":
1. **`Git Move versioned item(s) here`** → `dropmove` pathfile + `/droptarget:"<folder>"`. Condition: (same repo, or target folder ADDED) and target is FOLDERINGIT and source has NORMAL, INGIT or FOLDERINGIT.
2. **`Git Move and rename versioned item here`** → `dropmove … /rename`. Same condition as 1, plus ONLYONE.
3. **`Git Copy and add files to this WC`** → `dropcopyadd`. Condition: target FOLDERINGIT, source not INGIT or from another repo, and neither move entry was shown.
4. If the source is a PATCHFILE (`.diff` / `.patch`):
   - `Review/apply single &patch...` (ApplyPatch), only when ONLYONE;
   - then `Apply Patch Serial...` → `importpatch … /droptarget`.
5. **`Git New worktree...`** → `dropnewworktree`. Condition: ONLYONE, source is WCROOT or BAREREPO, and the target folder is not inside a work tree.
6. A separator follows if anything was added.

### 1.7 Invocation details (`InvokeCommand`, CM:1207-1655)
- Launcher: `<dll dir>\TortoiseGitProc.exe` plus `" /command:<name> …"`. It is started through Explorer's `IShellDispatch2::ShellExecute` when a site is available (Windows 11 path), otherwise detached with `CreateProcess`. If the launch fails, an error message box is shown (CM:2264-2320).
- The temp pathfile is a UTF-16 file in the TortoiseGit temp dir with a `git` prefix and one path per line ending in `\n`. If no files are selected, it holds the folder (CM:698-739).
- **DiffLater** (CM:1350-1375):
  - Ctrl held: clear the stored path and launch nothing.
  - Nothing stored yet: store the selected path in `HKCU\Software\TortoiseGit\DiffLater` (REG_SZ, default ""; `ShellExt.cpp:37`) and launch nothing.
  - A path is stored: launch `diff /path:"<sel>" /path2:<stored, escaped>` [`/alternative` if Shift held], then clear the stored path.
- **ApplyPatch** (CM:1494-1562) starts `TortoiseGitMerge.exe`, not TortoiseGitProc:
  - A patch file selected, or a `TGIT_UNIFIEDDIFF` patch on the clipboard (written to a temp file): ` /diff:"<patch>"` [` /patchpath:"<folder>"` if the target is FOLDERINGIT] [` /wc` if INVERSIONEDFOLDER].
  - Otherwise: ` /patchpath:"<path>"`.


### 1.8 Default masks (SCh:27-50, SC:46-54)
- **Top-level (ContextMenuEntries / ContextMenuEntrieshigh)** = Sync|CreateRepo|Clone|Commit = `0x20_00000406` (low `0x00000406`, high `0x00000020`).
- **Extended-only (ContextMenuExtEntriesLow / High)** = SVNIgnore|StashApply|SubmoduleSync = `0x1200040000000` (low `0x40000000`, high `0x00012000`).
- **Hidden (ContextMenuEntriesMaskLow / High)** = 0. HKLM and HKCU values are OR-ed (SC:56-59, 203-214). An entry is hidden when **all** its bits are in the mask.
- **Windows 11 (ContextMenu11Entries, QWORD)** = Sync|Commit|Switch|Revert|Conflicteditor|Diff|Pull|Push|RepoBrowser|Log|Blame|Clone|Merge|StashSave|Settings = `0x2000103801871096`.

### 1.9 Windows 11 modern menu (`IExplorerCommand`, CM:2343-2525)
- A single root item titled **"TortoiseGit"** with the `IDI_APP` icon from TortoiseGitProc.exe and `ECF_HASSUBCOMMANDS`.
- It is hidden in the classic menu: the site window class is not `NamespaceTreeControl` (CM:2385-2401).
- `GetState`:
  - re-runs `Initialize` from the shell item array (folder background is taken from the Explorer view path);
  - hides the root when HideMenusForUnversionedItems is set and Shift is not held and none of INGIT|INVERSIONEDFOLDER|FOLDERINGIT is set;
  - then runs `QueryContextMenu(nullptr, …, CMF_EXTENDEDVERBS|CMF_NORMAL)` and hides the root if no commands were produced (CM:2448-2503).
- Because `CMF_EXTENDEDVERBS` is always passed, **ITEMIS_EXTENDED is always set** on Windows 11.
- With `hMenu == nullptr` the menu is flat: only rows whose bit is in ContextMenu11Entries (plus Inaccessible and LFS children) become subcommands. Separators come from the same lazy logic (CM:983, 987, 1013).

---

## 2. Overlay icons

### 2.1 Handlers (Globals FileState G:121-135, Guids.h, `TortoiseGIT.cpp:125-149`, `register.registry:167-192`)
Overlays are registered through the shared **TortoiseOverlays** component (`HKLM\SOFTWARE\TortoiseOverlays\<Slot>` "Git"=CLSID). TortoiseGit does not supply icon files itself (IO:67-69).

| Slot | FileState | CLSID | GetPriority (IO:72-105) |
|---|---|---|---|
| Conflict | FileStateConflict | 475A024D-… | 0 (highest) |
| Modified | FileStateModified | 8DA7CDCB-… | 1 |
| Deleted | FileStateDeleted | D69716CD-… | 2 |
| ReadOnly (= assume-valid / "needs lock") | FileStateReadOnly | 5F380D0B-… | 3 |
| Locked (= skip-worktree) | FileStateLockedOverlay | 4E453CBA-… | 4 |
| Added | FileStateAddedOverlay | A38915E4-… | 5 |
| Normal | FileStateVersioned | 451C7E59-… | 6 |
| Ignored | FileStateIgnoredOverlay | 1B94B098-… | 100 (S_FALSE) |
| Unversioned | FileStateUnversionedOverlay | 18BF1135-… | 100 (S_FALSE) |

The shell only uses the first S_OK handler, so `IsMemberOf` makes sure **exactly one** handler answers S_OK per path (IO:249-253). The status is computed once and reused by the other handlers through a one-entry cache (`g_filepath`/`g_filestatus`, IO:129-142, 243-247).

### 2.2 Status → overlay (IO:254-398)
"loaded" means Explorer actually loaded that handler (`GetOverlayInfo` was called; the slot limit is 15).

| git status | Overlay shown |
|---|---|
| none | none |
| unversioned | Unversioned, only if `ShowUnversionedOverlay` (default 1) and the slot is loaded |
| ignored | Ignored, only if `ShowIgnoredOverlay` (default 1) and the slot is loaded |
| normal | Locked if skip-worktree; else ReadOnly if assume-valid; else Normal. Each flag only applies if its slot is loaded. Skip-worktree beats assume-valid. |
| deleted | Deleted; Modified if the Deleted slot is not loaded |
| modified | Modified; Normal if not loaded |
| added | Added; Modified if not loaded |
| conflicted | Conflict; Modified if not loaded |

Status enum `git_wc_status_kind` (`src/Git/GitStatus.h:25-36`): none, unversioned, ignored, normal, deleted, modified, added, conflicted, unknown.
Importance ranking (`GitStatus.cpp:78-99`): none 0 < unversioned 1 < ignored 2 < normal = added 6 < deleted 8 < modified 10 < conflicted 12.

TortoiseGitProc's "Overlay handlers" page warns that when too many other handlers are installed, Windows drops TortoiseGit slots in this order: Locked, then Ignored, then ReadOnly, then Unversioned (`SetOverlayHandlers.cpp:189-205`).

### 2.3 Gate before status lookup (IO:144-160; SC:352-419)
1. If the path is **not allowed** by `IsPathAllowed`, and the handler is Normal and `ShowExcludedAsNormal` (default 1) and the drive is not A:/B: and the path is a directory inside a git work tree: show the **Normal** overlay. Otherwise show nothing.
2. `IsPathAllowed` checks, in order:
   - `:\RECYCLER` and `:\$Recycle.Bin` are always excluded;
   - the include/exclude path filter (`OverlayIncludeList` / `OverlayExcludeList`) decides if it matches;
   - otherwise the drive type decides: DriveMaskFixed default 1; Remote, Removable, CDROM, RAM and Floppy default 0. A: and B: count as removable when Floppy is 0. Unknown drives are excluded when DriveMaskUnknown is **set** (inverted sense, SC:415).
3. Path filter rules (SC:529-686): entries are newline-separated and environment variables are expanded. A trailing `?` makes the entry apply to that folder only; otherwise it is recursive (a trailing `*` or `\` is stripped). The most specific path wins, a single-folder entry beats a recursive one, and include beats exclude for the same path. Include paths override drive-type exclusions (dug_settings_overlays.xml).
4. If `ShowOverlaysOnlyNonElevated` (default 0) is set and the process is elevated, the cache type is forced to `none` (IO:156-160).

### 2.4 Cache types (SCh:72-78; SC:28; `SetOverlayPage.cpp:54,141-158`)

| Value | ShellCache enum | Settings UI label | Folder status |
|---|---|---|---|
| 0 | none | None | Folders that are in a work tree (`HasGITAdminDir(dir)`) get **normal**; files and other folders get nothing (IO:224-238). |
| 1 | exe | **Default** (TGitCache.exe), the default unless in an RDP session | **Recursive.** The status comes from TGitCache over a pipe and includes assume-valid / skip-worktree flags (IO:163-181). Paths not in a work tree return none before asking the cache. |
| 2 | dll | Shell, the default in an RDP session (`SM_REMOTESESSION`) | In-process. Folder = conflicted if any index entry under it has a stage; else modified if any **direct** child (or a direct submodule) is modified, added or deleted compared with the index; else normal. A folder not in the index is unversioned; the WC root is normal (`GitStatus.cpp:534-713`, called non-recursively from `GitFolderStatus.cpp:87`). |
| 3 | dllFull | Shell Extended | Same as dll but **recursive** over all index entries below the folder (`GetAllStatus(path, cachetype != dll)`, `GitFolderStatus.cpp:87`). |

- In every mode, a folder status of added or deleted is shown as **modified** (`AdjustFolderStatus`, `GitStatus.cpp:741-745`), so folders only show none, unversioned, ignored, normal, modified or conflicted.
- Recursive aggregation in the TGitCache (exe) mode (`TGitCache/CachedDirectory.cpp:588-608, 726-748`):
  - folder = most important of (own status, most important direct file, every child folder's status), then AdjustFolderStatus;
  - a folder that is not ignored itself but has only ignored content shows **unversioned**;
  - `UnversionedAsModified` (default 0; only meaningful for exe) counts unversioned or none files as modified, unless the running status is added;
  - `TGitCacheRecurseSubmodules` (default 0) passes a submodule's status up to its parent repo's folders (CachedDirectory.cpp:642);
  - a recursive status is only reported when the folder's own status is greater than none (CachedDirectory.cpp:696).
- Upstream quirk: `isfull` in `GitStatus.cpp:57,112` is computed as `CRegStdDWORD(CacheType) ? dll : exe == dllFull`, which is always false. So the dll and dllFull modes never compare with HEAD and never compute ignored status: ignored items show as unversioned. This matches the doc's note that "Shell only shows differences of the filesystem to the git index" (dug_settings_overlays.xml).
- In the dll and dllFull modes, if `BlockStatus` (default 0) is set and a TortoiseGitProc instance is running (mutex `TortoiseGitProc.exe`), the status is "invalid", which means no overlay (`GitFolderStatus.cpp:44-52`).

---

## 3. ITEMIS_* computation (`CShellExt::Initialize`, CM:52-503)

### 3.1 Helpers
- **`GetAdminDirMask(path)`** (`src/Git/TGitPath.cpp:685-731`). Returns 0 unless the path is inside a work tree (`HasAdminDir`: walks up from the dir, or from the parent dir for a file, looking for `.git`. The admin dir must contain HEAD, config, objects/ and refs/heads/. A bare repo met on the way stops the walk; a path inside `.git` gives false). Otherwise it sets:
  - always `INGIT|INVERSIONEDFOLDER`;
  - for a directory, `FOLDERINGIT`; if it is the WC root, `WCROOT`, plus `SUBMODULE` if the parent repo's `.gitmodules` lists its relative path in `submodule.*.path`;
  - `STASH` if `<gitdir>/refs/stash` exists or `packed-refs` has `refs/stash`;
  - `GITSVN` if `<gitdir>/svn/.metadata` exists;
  - `BISECT` if `<worktree gitdir>/BISECT_START` exists;
  - `MERGEACTIVE` if `<worktree gitdir>/MERGE_HEAD` exists;
  - `SUBMODULECONTAINER` if `<wc root>/.gitmodules` exists.
- **Status → flags**, applied after the mask (CM:318-331):
  - unversioned, ignored or none: clear `INGIT`;
  - ignored: `IGNORED`;
  - normal: `NORMAL`;
  - conflicted: `CONFLICTED`;
  - added: `ADDED`;
  - deleted: `DELETED`.
  - In both the exe and dll paths, if the item is a directory: `FOLDER`, plus `FOLDERINGIT` unless the status is unversioned, ignored or none (CM:266-271, 286-291).

### 3.2 Explorer selection (CM:211-350)
- Items excluded by `NoContextPaths` and items inside `.git` are skipped.
- `PATCHFILE` is set if **any** selected item ends in `.diff` or `.patch` (case-insensitive).
- Status is fetched only for the **first** item that gets one (`statfetched`). Flags therefore describe the first item, apart from PATCHFILE. The mask is still OR-ed for each item processed before `statfetched`.
- `SimpleContext=1` and the item is a dir: no status lookup; the status is normal if the dir is inside a work tree (CM:241-245).
- Cache type exe and the path is allowed: the status comes from TGitCache; the item is skipped if it is not in a work tree or is the admin dir. Otherwise `GitStatus::GetStatus` runs, and on error the item falls back to normal if inside a work tree (CM:250-309).
- `INVERSIONEDFOLDER` is also set if the first item's **parent** is in a work tree (CM:333-335).
- If the first item is a **bare repo**, itemStates is **replaced** by `BAREREPO` alone (CM:337-338).
- `PATCHINCLIPBOARD` is set if clipboard format `TGIT_UNIFIEDDIFF` is present; `PATHINCLIPBOARD` if `CF_HDROP` is present (CM:342-349).

### 3.3 Folder background or drop target (`pIDFolder`, CM:355-450)
- `itemStatesFolder` gets the folder's admin mask (CM:405) and NORMAL / CONFLICTED / ADDED / DELETED / IGNORED from the folder's status, plus `FOLDER`. A bare repo folder **replaces** it with `BAREREPO` (CM:420-421).
- If no files are selected, `ONLYONE` is set (CM:441-442).
- Except for the drop handler, `itemStates |= itemStatesFolder` (CM:443-444). Because the admin mask has `INGIT` set unconditionally, a background click inside a work tree always ends with INGIT set. This is an upstream quirk: the INGIT clear at CM:407-408 acts on `itemStates` before the OR.
- `INACCESSIBLE` is set when the item is not BAREREPO or INVERSIONEDFOLDER, but `<folder>\.git` exists or bare-repo detection errored (CM:423-424). Example: an invalid or broken repo.

### 3.4 Post-processing (CM:452-500)
- Exactly 2 items selected: `TWO`.
- Exactly 1 item selected (and allowed): `ONLYONE`. If that item is a **directory** (not the drop handler), `folder_` is set to it and the code re-applies the admin mask, clears INGIT for unversioned/ignored/none, and sets IGNORED, `FOLDER`, ADDED and DELETED. It also sets `INACCESSIBLE` if `.git` exists but the repo is not recognised, or bare detection reports a format error.

### 3.5 Set in QueryContextMenu
- `EXTENDED`: Shift held (classic menu) or always (Windows 11).
- `HASDIFFLATER`: a DiffLater path is stored. Nothing in the menu table uses this flag; the DiffLater label reads the registry directly.

---

## 4. Settings keys (all `HKCU\Software\TortoiseGit\…` unless noted)

| Key | Type | Default | Effect (source) |
|---|---|---|---|
| CacheType | DWORD | 1 (exe), or 2 (dll) in an RDP session | Overlay and status source: 0 none, 1 TGitCache, 2 Shell, 3 Shell Extended (SC:28) |
| ShowOverlaysOnlyNonElevated | DWORD | 0 | Forces cache `none` for elevated processes (SC:29, IO:156) |
| DriveMaskRemote / Fixed / CDROM / Removable / Floppy / RAM / Unknown | DWORD | 0 / **1** / 0 / 0 / 0 / 0 / 0 | Which drive types get overlays; Unknown has the inverted sense (SC:30-36, 405-416) |
| OverlayExcludeList / OverlayIncludeList | string (`\n`-sep) | "" | Path filter (SC:691-692) |
| ShowExcludedAsNormal | DWORD | 1 | Excluded versioned folders still get the Normal overlay (SC:44, IO:146) |
| ShowUnversionedOverlay | DWORD | 1 | (SC:42). The settings UI writes `HKCU\Software\TortoiseOverlays\ShowUnversionedOverlay`, which the shell does not read (SetOverlayHandlers.cpp:36) |
| ShowIgnoredOverlay | DWORD | 1 | Same remark (SC:43) |
| UnversionedAsModified | DWORD | 0 | TGitCache only (SC:39) |
| TGitCacheRecurseSubmodules | DWORD | 0 | TGitCache only (SC:40) |
| BlockStatus | DWORD | 0 | dll modes: no status while TortoiseGitProc runs (SC:61) |
| ContextMenuEntries / ContextMenuEntrieshigh | DWORD×2 | 0x406 / 0x20 | Top-level entries (SC:49-50) |
| ContextMenuExtEntriesLow / ContextMenuExtEntriesHigh | DWORD×2 | 0x40000000 / 0x12000 | Shift-only entries (SC:53-54) |
| ContextMenuEntriesMaskLow / High | DWORD×2, HKCU **and** HKLM | 0 | Hidden entries (OR of both hives) (SC:56-59) |
| ContextMenu11Entries | QWORD | 0x2000103801871096 | Windows 11 menu entries (SC:46) |
| HideMenusForUnversionedItems | DWORD | 0 | No menu outside repos unless Shift is held (SC:41, CM:871) |
| SimpleContext | DWORD | 0 | Skip the status lookup for selected dirs (SC:38, CM:241) |
| ShellMenuAccelerators | DWORD | 1 | Keep `&` accelerators on top-level items (SC:37) |
| ShowContextMenuIcons | DWORD | 1 | Menu icons (CM:943) |
| EnableDragContextMenu | DWORD | 1 | Right-drag menu (CM:88, 743) |
| NoContextPaths | string (`\n`-sep) | "" | No context menu for these paths. A trailing `*` means prefix match; a trailing `\` means that folder and everything below; otherwise exact match; all case-insensitive (SC:69, 311-350) |
| LoadDllOnlyInExplorer | DWORD | 0 | The stub only loads the extension in explorer.exe (TortoiseStub.cpp:97) |
| DiffLater | string | "" | Path stored by "Diff later" (`ShellExt.cpp:37`) |
| LanguageID | DWORD | `CLangDll::s_defaultLang` | UI language of the menu strings (SC:60) |

Settings are re-read when `RegNotifyChangeKeyValue` signals a change under `HKCU\Software\TortoiseGit` (SC:70-82, 106-171). The HKLM hide-mask is re-read at most every 10 s (SC:203-214). The admin-dir lookup cache has a 10 s TTL and is purged every 10 min (SCh:24, SC:427-499).

---

## 5. Notes for a cross-platform clone
- Finder Sync, Nautilus and Dolphin have no 15-slot overlay limit and no "first S_OK wins" rule. Use one badge per path picked with the same mapping as §2.2, keeping the fallback chain only where the platform limits badges. Record this in `docs/divergences.md`.
- Keep the `/command:` names and arguments in §1.3 exactly, so the launcher maps 1:1 with TortoiseGitProc.
- The menu builder is pure logic: (flags, masks, the table above) → ordered list of entries, top/sub placement and separators. It can be unit-tested without any GUI. The quirks noted above (background INGIT, the BAREREPO overwrite, the first-item-only status) are part of the observable behaviour; decide explicitly whether to keep them.
