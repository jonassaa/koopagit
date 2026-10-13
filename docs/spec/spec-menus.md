# TortoiseGit behavioural spec: right-click (context) menus

Source: TortoiseGit upstream checkout. All paths are relative to `src/`. Citations are `file:line`, and a bare `:NNN` refers to the file named in the section or table header.

Labels are the exact English strings from these files:
- `Resources/TortoiseProcENG.rc`
- `Resources/TortoiseLoglistCommon.rc2`
- `TortoiseShell/resourceshell.rc`, which holds the `IDS_MENU*` strings
- `Resources/TortoiseMergeENG.rc`
- `Resources/TortoiseGitBlameENG.rc`

In labels, `&` marks the mnemonic letter. A `%s` or `%d` is filled in at runtime, as described in the row.

Conventions used throughout:
- **Hidden, not disabled.** Unless a row says "disabled", an entry whose condition fails is not added to the menu at all. TortoiseGit greys out entries only in a few places, and each is called out.
- **Shift.** Holding Shift while choosing an entry selects the alternative diff/merge tool wherever a diff is launched. In a few menus, holding Shift while the menu opens reveals extra entries; those are noted.
- **WC_REF** is the working-tree pseudo revision (`GIT_REV_ZERO`).
- **Commands.** `/command:x` means the command is launched as `TortoiseGitProc /command:x …` in a separate process (`CAppUtils::RunTortoiseGitProc`).
- **Order.** Rows in each table are listed in insertion order.

## 1. File status list (`CGitStatusListCtrl`)

Short refs used in this section: **SLC** = `Git/GitStatusListCtrl.cpp`, **SLH** = `Git/GitStatusListCtrl.h`.

### 1.1 Menu bits
Every menu ID `IDGITLC_*` (enum, SLH:197-263, `IDGITLC_REVERT = 1`, so the IDs run 1 to 56) is also a bit: `GetContextMenuBit(id) = 1 << id` (SLH:265). A dialog passes a 64-bit `dwContextMenus` mask to `Init()` (SLH:438). `GITSLC_POPALL = 0xFFFF…FF` (SLH:112). The named macros (SLH:113-139) are:

| macro | bit of | macro | bit of |
|---|---|---|---|
| POPCOMPAREWITHBASE | IDGITLC_COMPARE | POPEXPLORE | IDGITLC_EXPLORE |
| POPCOMPARE | IDGITLC_COMPAREWC | POPCOMMIT | IDGITLC_COMMIT |
| POPGNUDIFF | IDGITLC_GNUDIFF1 | POPCHANGELISTS | IDGITLC_CHECKGROUP |
| POPREVERT | IDGITLC_REVERT | POPBLAME | IDGITLC_BLAME |
| POPSHOWLOG | IDGITLC_LOG | POPSAVEAS | IDGITLC_SAVEAS |
| POPSHOWLOGSUBMODULE | IDGITLC_LOGSUBMODULE | POPCOMPARETWOFILES | IDGITLC_COMPARETWOFILES |
| POPSHOWLOGOLDNAME | IDGITLC_LOGOLDNAME | POPRESTORE | IDGITLC_POPRESTORE |
| POPOPEN | IDGITLC_OPEN | POPASSUMEVALID | IDGITLC_ASSUMEVALID |
| POPDELETE | IDGITLC_DELETE | POPSKIPWORKTREE | IDGITLC_SKIPWORKTREE |
| POPADD | IDGITLC_ADD | POPEXPORT | IDGITLC_EXPORT |
| POPIGNORE | IDGITLC_IGNORE | POPUNSETIGNORELOCALCHANGES | IDGITLC_UNSETIGNORELOCALCHANGES |
| POPCONFLICT | IDGITLC_EDITCONFLICT | POPPREPAREDIFF | IDGITLC_PREPAREDIFF |
| POPRESOLVE | IDGITLC_RESOLVECONFLICT | POPLFSLOCK / POPLFSUNLOCK | IDGITLC_LFSLOCK / LFSUNLOCK |

Some entries are tested against their own ID bit directly, with no macro: COMPAREWC, COMPARETWOREVISIONS, GNUDIFF2REVISIONS, REVERTTOREV, REVERTTOPARENT, EXPORT, SAVEAS.

### 1.2 Mask passed by each dialog (args are `Init(cols, key, mask, bHasCheckboxes, bHasWC)`)
| dialog | mask | checkboxes / hasWC | src |
|---|---|---|---|
| CommitDlg | POPALL ^ (COMMIT \| SAVEAS \| PREPAREDIFF) | yes / yes | `TortoiseProc/CommitDlg.cpp:251` |
| AddDlg | POPALL ^ (ADD \| COMMIT \| CHANGELISTS \| PREPAREDIFF) | yes / yes | `TortoiseProc/AddDlg.cpp:63` |
| RevertDlg | POPALL ^ (COMMIT \| RESTORE \| CHANGELISTS) | yes (default) / yes | `TortoiseProc/RevertDlg.cpp:65` |
| ResolveDlg | POPALL ^ (IGNORE \| ADD \| COMMIT \| EXPORT \| RESTORE \| SAVEAS \| PREPAREDIFF \| CHANGELISTS) | yes / yes | `TortoiseProc/ResolveDlg.cpp:60` |
| ChangedDlg (Check for modifications) | POPALL ^ (SAVEAS \| RESTORE \| PREPAREDIFF) | no / yes | `TortoiseProc/ChangedDlg.cpp:101` |
| LogDlg changed files | POPALL ^ (RESTORE \| CHANGELISTS) | no / `m_LogList.m_hasWC` (false in a bare repo) | `TortoiseProc/LogDlg.cpp:308` |
| RebaseDlg file list | POPALL ^ (COMMIT \| RESTORE \| CHANGELISTS) | no / yes | `TortoiseProc/RebaseDlg.cpp:243` |
| SyncDlg in/out changelists | bit(COMPARETWOREVISIONS) \| bit(GNUDIFF2REVISIONS) only | no / no | `TortoiseProc/SyncDlg.cpp:1052-1055, 1101-1104` |
| SyncDlg conflict list | EXPLORE \| OPEN \| SHOWLOG \| CONFLICT \| RESOLVE | no / yes | `TortoiseProc/SyncDlg.cpp:1068-1070` |
| LFSLocksDlg | LFSLOCK \| LFSUNLOCK | yes / yes | `TortoiseProc/LFSLocksDlg.cpp:61` |
| FileDiffDlg | does not use this control; it has its own menu (§1.6) | | |

Per-dialog state that changes the menu:
- `m_CurrentVersion`: empty means working tree. In LogDlg it is the selected commit (`LogDlg.cpp:1025`), and empty when the WC row is selected.
- `m_amend`: set in CommitDlg when amending (`CommitDlg.cpp:1403`). Diffs and reverts then use `HEAD~1` in place of `HEAD`.
- `m_Rev1` / `m_Rev2`: set by SyncDlg.
- `m_bIsRevertTheirMy`: true while a rebase is active (SLC:387). It swaps the mine/theirs labels.
- `m_sDisplayedBranch`: the LogDlg range (`LogDlg.cpp:3191`).

### 1.3 Group header menu (right-click with no item selected, list has checkboxes)
Defined in `OnContextMenuGroup`, SLC:1575-1637.

| id | label | action |
|---|---|---|
| IDGITLC_CHECKGROUP | Select changelist | Check every item in that group (SLC:1590) |
| IDGITLC_UNCHECKGROUP | Deselect changelist | Uncheck every item in that group (SLC:1592) |

### 1.4 Item menu: insertion order (`OnContextMenuList`, SLC:1639-2063)
`n` is the selected count. `st` is the `m_Action` flags of the **selection-mark (focused) item only**, not of all selected items. Notation: `UNV` = LOGACTIONS_UNVER, `IGN` = IGNORE, `DEL` = DELETED, `ADD` = ADDED, `UNM` = UNMERGED, `MISS` = MISSING. `WT` means `m_CurrentVersion` is empty. Entries are never disabled; an entry either appears or does not. Shift held while the menu opens adds the hidden "Add as…" items.

| # | id | label | shown when | src |
|---|---|---|---|---|
| 1 | EDITCONFLICT (default item) | &Edit conflicts | st&UNM, n==1, POPCONFLICT | 1693-1696 |
| 2 | RESOLVECONFLICT | Resolved | st&UNM, POPRESOLVE | 1700 |
| 3 | RESOLVETHEIRS | Resolve conflict using "theirs" | same as #2 (the label uses the theirs title; when `m_bIsRevertTheirMy`, mine and theirs swap order and IDs) | 1701-1715 |
| 4 | RESOLVEMINE | Resolve conflict using "mine" | same as #2 | 1701-1715 |
| — | separator | | st&UNM and (POPCONFLICT or POPRESOLVE) | 1717 |
| 5 | ADD | Add | st&(UNV\|IGN), POPADD | 1727 |
| 6 | ADD_EXE | Add as Executable (+x) | as #5, plus not a directory and Shift held | 1730 |
| 7 | ADD_LINK | Add as Symlink | as #6 | 1731 |
| 8 | COMMIT | Commit... | st&(UNV\|IGN), POPCOMMIT | 1734 |
| 9 | COMPARE | Compare with b&ase; becomes **Three way diff** when the entry is a merge-combined row (`m_ParentNo & MERGE_MASK`) | "versioned diff" condition **V**: !(st&(UNV\|IGN)), and if st&MISS then st must be exactly MISS\|DEL or MISS\|DEL\|MODIFIED; n>0; POPCOMPAREWITHBASE. It is the default item unless (st&UNM and n==1) | 1739-1751 |
| 10 | GNUDIFF1 | Show changes as &unified diff | V, the repo has commits (not `IsInitRepos`), POPGNUDIFF | 1754-1758 |
| 11 | COMPAREWC | Compare with &working tree | V, bit(COMPAREWC), hasWC, !WT | 1760-1764 |
| 12 | COMPAREPARENTWC | Compare parent with working tree: "subject≤20…" (shorthash) | as #11, and parent `m_CurrentVersion^(ParentNo+1)` resolves | 1766-1773 |
| — | separator | | any of #9 to #12 added | 1778 |
| 13 | COMPARETWOREVISIONS (default item) | &Compare revisions | m_Rev1 and m_Rev2 set (SyncDlg), bit set | 1782-1787 |
| 14 | GNUDIFF2REVISIONS | Show changes as &unified diff | as #13 | 1789 |
| 15 | COMPARETWOFILES | Compare two files | n==2, both selected items are files, POPCOMPARETWOFILES | 1796-1809 |
| 16 | COMMIT | Commit... | **W** = (n>0, !(st&(UNV\|IGN)), hasWC); POPCOMMIT, WT, !(st&(SKIPWORKTREE\|ASSUMEVALID)) | 1815-1818 |
| 17 | REVERT | Re&vert... | W, POPREVERT, WT | 1820 |
| 18 | SKIPWORKTREE | Skip worktree | W, WT, POPSKIPWORKTREE, !(st&(ADD\|UNM\|SKIPWORKTREE)) | 1823 |
| 19 | ASSUMEVALID | Assume Unchanged | W, WT, POPASSUMEVALID, !(st&(ADD\|DEL\|UNM\|ASSUMEVALID)) | 1826 |
| 20 | UNSETIGNORELOCALCHANGES | Unflag as skip-worktree and assume-unchanged | W, WT, bit set, st&(SKIPWORKTREE\|ASSUMEVALID) | 1829 |
| 21 | CREATERESTORE / RESTOREPATH | Restore after commit / Restore | W, POPRESTORE, not a directory; "Restore" appears when a backup of this path already exists | 1832-1838 |
| 22 | LFSLOCK / LFSUNLOCK | LFS Lock / LFS Unlock | W, WT, a LFS bit set, !(st&UNM), the repo uses LFS, no directory selected. If the lock column is hidden, both appear. Otherwise Lock appears only if every selected item is unlocked, Unlock only if every one is locked | 1840, 2776-2824 |
| 23 | REVERTTOREV | R&evert to this revision | W, bit set, !WT, !(st&DEL) | 1843-1846 |
| 24 | REVERTTOPARENT | Revert to parent revision: "subject…" (hash) | W, bit set, !WT, parent resolves | 1848-1858 |
| 25 | LOG | Show &log | n==1, !(st&(UNV\|IGN)), POPSHOWLOG | 1861-1864 |
| 26 | LOGSUBMODULE | Show log of submodule | as #25 with POPSHOWLOGSUBMODULE, and the item is a directory (submodule) | 1865 |
| 27 | LOGOLDNAME | Show log &before rename/copy | as #25 with POPSHOWLOGOLDNAME, st&(REPLACED\|COPY), old path known | 1867 |
| 28 | BLAME | &Blame | as #25 with POPBLAME, file, !(st&DEL), !(st&ADD and WT), hasWC | 1869 |
| 29 | EXPORT | Export selection to... | n>0, bit set, !(st&(DEL\|MISS)) | 1875 |
| 30 | SAVEAS | Save revision &to... | n==1, bit set, file, !(st&DEL) | 1881 |
| 31 | VIEWREV | View revision in alternative editor | n==1, POPOPEN, file, !(st&(DEL\|MISS)) | 1884-1886 |
| 32 | OPEN | &Open | same as #31 | 1887 |
| 33 | OPENWITH | Open with... | same as #31 | 1888 |
| 34 | EXPLORE | Explore to | n==1, POPEXPLORE, !(st&DEL), hasWC | 1891 |
| — | separator | | n==1, POPPREPAREDIFF, !(st&DEL) | 1896 |
| 35 | PREPAREDIFF | Mark for comparison | same as the separator | 1897 |
| 36 | PREPAREDIFF_COMPARE | Diff with "X" | a file is already marked (in memory or in reg `DiffLater`). X is the marked revision when the same path is marked; otherwise the compacted path, plus `:shorthash` unless it is the WT version and an absolute path | 1898-1913 |
| 37 | DELETE | &Delete | st&(UNV\|IGN\|MISS), POPDELETE | 1927-1930 |
| 38 | Ignore submenu or flat items | see §1.5 | st&(UNV\|DEL), POPIGNORE | 1932-1985 |
| — | separator | | n>0 (always) | 1992 |
| 39 | submenu **Copy to clipboard** | children: full paths (COPYFULL), relative paths (COPYRELPATHS), file/foldernames (COPYFILENAMES), Copy all information to clipboard (COPYEXT), and column '<column name>' (COPYCOL, only when the right-click hit a sub-item column) | n>0 | 1994-2008 |
| — | separator | | POPCHANGELISTS (plus a legacy SVN-enum status check that always passes for git) | 2010-2013 |
| 40 | REMOVEFROMCS | Remove from changelist | a selected item has a changelist | 2015-2016 |
| 41 | submenu **Move to changelist** | children: `<new changelist>` (CREATECS), separator, `ignore-on-commit` (CREATEIGNORECS), then a separator and every existing changelist except ignore-on-commit (IDs MOVETOCS+i) | POPCHANGELISTS | 2017-2047 |
| 42 | KEEPCHANGELISTS | Keep changelists (checkmark shows `m_bKeepChangeLists`) | POPCHANGELISTS | 2048-2049 |
| — | separator, then submenu **Shell** | the Windows Explorer context menu for the selection | n>0, !(st&(DEL\|MISS)), hasWC, WT | 2055-2061 |

### 1.5 Ignore entries (SLC:1932-1985)
If every selected path has the same extension (the first item's extension), the entries go in a submenu **Add to &ignore list** with these children:
- the single item's name, or `&Ignore %d items by name` for several (IGNORE)
- `*<ext>` (IGNOREMASK), if the extension is not empty
- the containing directory's git path (IGNOREFOLDER), if exactly one item is selected and it is not at the root

If the extensions differ, two flat items appear instead:
- `Add to &ignore list` (one item) or `&Ignore %d items by name` (IGNORE)
- `Ignore %d items by &extension` (IGNOREMASK), if the first item has an extension

### 1.6 Actions (switch at SLC:2114-2760)
Shift held while choosing an item selects the alternative diff tool, where that applies. Diff commands ask for confirmation first (CheckMultipleDiffs, SLC:4360) when n > max(3, reg `NumDiffWarning`=10).

| id | action |
|---|---|
| VIEWREV / OPEN / OPENWITH | WT: use the file itself. Otherwise `git show <rev>:<path>` to a read-only temp file. Then open it in the alternative editor, `ShellOpen`, or the Open-with dialog (SLC:4673-4700) |
| LFSLOCK / LFSUNLOCK | Progress dialog runs `git lfs lock` / `git lfs unlock` on the selection (LFSSetLockedProgressCommand) (2128-2149) |
| EXPLORE | Open the file manager with the file selected (2151) |
| PREPAREDIFF | Remember path and `m_CurrentVersion` (2155-2158) |
| PREPAREDIFF_COMPARE | Diff the marked file/rev against this file/rev, then clear reg `DiffLater` (2160-2167) |
| CREATERESTORE | Copy each selected file to a temp file and show the restore overlay (2169-2192) |
| RESTOREPATH | Confirm "Do you really want to restore the copy? …" (buttons Restore / Abort), copy back, touch (2194-2216) |
| COMPAREWC / COMPAREPARENTWC | For each selected item, diff the working file against `m_CurrentVersion` (or its parent) (2219-2231, 2996-3024) |
| COMPARE | WT: working file vs `HEAD` (`HEAD~1` when amending). ADD or DEL rows diff against null. A new repo diffs against null. Commit: `rev~1` (`~2` when amending) vs rev; an ADD row or a missing parent diffs against null; a merge row is a 3-way diff (2234-2245, 3026+) |
| COMPARETWOREVISIONS | `m_Rev1` vs `m_Rev2`; an added or deleted file diffs against null (2247-2258, 2977-2994) |
| COMPARETWOFILES | `TortoiseGitProc /command:diff /path:<f1> /startrev:<WC_REF or HEAD> /path2:<f2> /endrev:<…> /hwnd:<h>` (a commit uses `rev`, or `rev~1` for a deleted file), plus ` /alternative` when Shift is held (2260-2283) |
| GNUDIFF1 | Concatenate `git diff` for each item: WT uses `HEAD[~1]..WC`; a commit uses `rev~1..rev` (`rev^N..rev` for parent rows, combined for merge rows). Honours `diff.context`. Opens the read-only temp file in TortoiseGitUDiff (2285-2355) |
| GNUDIFF2REVISIONS | Unified diff `m_Rev2..m_Rev1` for each item (2357-2368) |
| ADD / ADD_EXE / ADD_LINK | Progress dialog: `git add` the paths (with `--chmod=+x`, or as a symlink). Offers a Commit button if POPCOMMIT is set. Refresh (2370-2397) |
| DELETE | Remove from the index (libgit2 `remove_bypath`), then delete the files to the Recycle Bin (permanently when Shift is held) (2399, 4702+) |
| BLAME | `TortoiseGitBlame <fullpath> /rev:<m_CurrentVersion>` (LaunchTortoiseBlame) (2403-2407) |
| LOG / LOGSUBMODULE | `/command:log /path:<full>` + [` /submodule` if a directory] + (` /range:<displayed range>`, or ` /endrev:"<rev>" /rev:"<rev>"`). LOGSUBMODULE opens the submodule's own log at the submodule commit recorded in `rev`, when reg `LogSubmoduleShowRevision`=1 (2409-2433) |
| LOGOLDNAME | `/command:log /path:<old full path>` [/submodule] [/range:…] (2435-2446) |
| EDITCONFLICT | `CAppUtils::ConflictEdit` launches TortoiseGitMerge in 3-way mode (or the delete/submodule/symlink conflict dialogs) (2448-2462) |
| RESOLVECONFLICT / RESOLVETHEIRS / RESOLVEMINE | Confirm "Are you sure you want to mark the conflicted file(s) as resolved?", then a progress dialog: theirs/mine first run `git checkout --theirs/--ours -- <p>`, then `git add -- <p>` (ResolveProgressCommand). The row becomes MODIFIED (2464-2508) |
| IGNORE / IGNOREMASK / IGNOREFOLDER | `CAppUtils::IgnoreFile` opens the Ignore dialog (which .gitignore; optionally remove from the index), then appends name, `*.ext` or folder patterns (2510-2546) |
| COMMIT | Write the selection to a temp path file, then `/command:commit /pathfile:"<tmp>" /deletepathfile` (2547-2559) |
| REVERT | If any selected item is a modified file, confirm "Are you sure you want to revert %d item(s)? You will lose ALL changes since the last update!". Then a progress dialog reverts to `HEAD` (`HEAD~1` when amending) (RevertProgressCommand: `git checkout HEAD -- <p>`, or `git rm --cached` for added files) (2560-2660) |
| ASSUMEVALID / SKIPWORKTREE / UNSETIGNORELOCALCHANGES | Confirm (default No), then set or clear the index flags `assume-valid` / `skip-worktree` with libgit2 (2662-2670, 2863-2917) |
| COPYFULL / COPYRELPATHS / COPYFILENAMES | One line per item (`\r\n` between items); the path column only (2671-2675, 3880-3936) |
| COPYEXT | All visible columns: a header line of column names, then the values, tab-separated, `\r\n` between rows (2676-2678) |
| COPYCOL | Only the clicked column, with no header (2679-2681) |
| EXPORT | Folder picker. Copy each file (from the WT, or `git show rev:path`) into `<dir>\<relpath>`. Skips directories and deleted files (2682, 4504-4544) |
| SAVEAS | Save dialog proposing `<dir>\<base>-<shorthash><ext>`, then copy the file or write `rev:path` (2685, 4546-4571) |
| REVERTTOREV / REVERTTOPARENT | For each item, move the WT file to the Recycle Bin (reg `RevertWithRecycleBin`=1), then `git checkout <rev> -- <path or old path>`. For an added file when reverting to the parent: `git rm --cached --ignore-unmatch -- <path>`. Shows "N files reverted to <rev>" (2689-2694, 4600-4671) |
| CREATEIGNORECS / CREATECS / MOVETOCS+i / REMOVEFROMCS | Assign the path to a changelist (moving to `ignore-on-commit` also unchecks the item), or remove it from one. CREATECS first asks for a name. Saves the changelists (2711-2758) |
| KEEPCHANGELISTS | Toggle reg/state `KeepChangeLists` (2729) |

### 1.7 FileDiffDlg file list (its own menu, `TortoiseProc/FileDiffDlg.cpp:545-760`)
Shown only when at least one item is selected. `rev1` and `rev2` are the two compared commits. `first` is the first selected item.

| id | label | shown when | action |
|---|---|---|---|
| ID_COMPARE (default) | &Compare revisions | always | Diff each selected item between rev1 and rev2 (:569, 607-617) |
| ID_GNUDIFFCOMPARE | Show changes as &unified diff | always | Unified diff rev1 to rev2 (old path for renames) (:571, 619-634) |
| — sep | | always | |
| ID_REVERT1 | Revert to revision <title of rev1> | not bare, rev1 set | `git checkout <rev1> -- <old path or path>`, or `git rm --cached` when the file did not exist there (:576-579, 1283+) |
| ID_REVERT2 | Revert to revision <title of rev2> | not bare, rev2 set, and (rev1 empty or `first` not deleted) | same, with rev2 (:581-584) |
| — sep | | not bare | |
| ID_LOG | Show log | always | `/command:log [/submodule] /path:<p> /endrev:<rev2> /rev:<rev2>` (:587, 667-692) |
| ID_BLAME | &Blame revisions | `first` is a file and not bare | Blame at rev2 (deleted file: rev1; renamed with rev2 empty: the old path at rev1) (:590-593, 642-665) |
| — sep | | as ID_BLAME | |
| ID_EXPORT | &Export selection to... | `first` is a file | Export to a folder (:595, 739+) |
| ID_LOGSUBMODULE | Show log of submodule | `first` is a directory | Submodule log at its recorded hash (:597-598) |
| — sep | | always | |
| ID_SAVEAS | &Save list of selected files to... | always | A .txt file: "Changed files between <rev1> and <rev2>" followed by one path per line (:600, 694-726) |
| ID_CLIPBOARD_PATH | Copy paths to clipboard | always | `<col0>\t\r\n` for each item (:601, 1213-1239) |
| ID_CLIPBOARD_ALL | Copy all information to clipboard | always | `c0\tc1\tc2\tc3\tc4\r\n` for each item |

---

## 2. Log commit list (`CGitLogListBase` / `CGitLogList`)

Short refs used in this section: **LLB** = `TortoiseProc/GitLogListBase.cpp`, **LLA** = `TortoiseProc/GitLogListAction.cpp`.

### 2.1 Mask bits and callers
- **Bits.** `m_ContextMenuMask` is a 64-bit mask. Each bit is `1 << ID_*` (enum at `TortoiseProc/GitLogListBase.h:326-397`, `ID_COMPARE = 1` up to `ID_TOGGLE_ROLLUP`). The `ID_COPYCLIPBOARD*` children carry no bits. `hideFromContextMenu(mask, exclusive)` either ANDs with `~mask`, or keeps only `mask` (LLB:205-211).
- **Default.** All bits are set except ID_REBASE_PICK/SQUASH/EDIT/SKIP, ID_LOG, ID_BLAME and ID_BLAMEPREVIOUS (LLB:94-100).

| caller | mask changes | src |
|---|---|---|
| LogDlg `m_LogList` | default | `TortoiseProc/LogDlg.h:213` |
| SyncDlg `m_InLogList` / `m_OutLogList` | default (no changes) | `TortoiseProc/SyncDlg.h:99-100` |
| RefLogDlg `CRefLogList` | adds ID_LOG, removes ID_COMPARETWOCOMMITCHANGES; `m_ColumnRegKey="reflog"` changes the clipboard submenu and default item | `TortoiseProc/refloglist.cpp:30-34` |
| RebaseDlg `m_CommitList` | removes CHERRY_PICK, SWITCHTOREV, RESET, REVERTREV, MERGEREV, REBASE_TO_VERSION, REVERTTOREV, COMBINE_COMMIT and TOGGLE_ROLLUP; while editable (stage "choose branch/commits") adds REBASE_PICK/SQUASH/EDIT/SKIP and LOG | `TortoiseProc/RebaseDlg.cpp:327-335, 1861-1865` (details in §6) |
| TortoiseGitBlame log pane | adds BLAMEPREVIOUS and LOG, then restricts to an exclusive list | `TortoiseGitBlame/LogListBlameAction.cpp:33-45` (see §9) |

Terms used in the tables below:
- **Context.** `n` = selected count. `E` = the first selected entry. `WC` = E is the working-tree pseudo-row (`E.m_CommitHash` empty). Its single parent is HEAD (`GitLogListBase.h:504-505`), so the parent-dependent single-commit items appear for it as well.
- **Flags.** `HEADc` = E is HEAD. `merge` = `.git/MERGE_HEAD` exists. `bisect` = bisect active. `stash` = E is `refs/stash` or that stash's index commit (LLB:1598-1619). `hasWC` = the repo is not bare. `refs(E)` = E has refs. `cur` = current branch name. `P` = parent count of E.
- **Parent label.** `Parent %d: "<subject ≤20 chars…>" (<shorthash>)` (LLB:1706-1724).
- **Ref label under the cursor.** Several items look for a ref label under the cursor (`IsMouseOnRefLabelFromPopupMenu`). If one is hit, the item targets that single ref.
- **Disabled vs hidden.** Nothing is greyed out; entries are hidden.

### 2.2 Insertion order (`OnContextMenu`, LLB:1640-2348)
| # | id | label | shown when | src |
|---|---|---|---|---|
| **A** | **Rebase actions** (E not CURRENT/DONE in rebase) | | | |
| 1 | ID_REBASE_PICK | Pick | bit | 1726 |
| 2 | ID_REBASE_SQUASH | Squash (with commit below) | bit, first and last selected not the bottom row, (cherry-pick mode or P==1) | 1729 |
| 3 | ID_REBASE_EDIT | Edit | bit | 1732 |
| 4 | ID_REBASE_SKIP | Skip | bit | 1735 |
| — | sep | | any rebase bit set | 1738 |
| **B** | **n==1** | | | |
| 5 | ID_COMPARE | Compare with &working tree | !WC, bit, hasWC | 1747 |
| 6 | ID_COMMIT | C&ommit... | WC, bit | 1755 |
| 7 | ID_MERGE_ABORT | Abort &Merge | WC, merge, bit | 1760 |
| 8 | ID_BLAMEPREVIOUS | Blame previous revision (P>1: submenu of Parent labels) | bit (Blame only) | 1767-1784 |
| 9 | ID_GNUDIFF1 | P==1: Show changes as &unified diff. P>1: submenu **&Unified diff with** → All Parents, Only Merged Files, Show extra changes after merge, Parent 1…n | bit, hasWC | 1786-1808 |
| 10 | ID_COMPAREWITHPREVIOUS | &Compare with previous revision (P>1: submenu of Parent labels). It is the default item when reg `DiffByDoubleClickInLog` is set and this is not the reflog | bit | 1810-1834 |
| 11 | ID_BLAME | Blame | bit (off by default everywhere) | 1836 |
| — | sep | | any of #5 to #11 | 1842 |
| 12 | ID_STASH_SAVE | Stash changes | WC, !merge, bit | 1848-1854 |
| 13 | ID_STASH_POP | Stash Pop | (WC or stash), `refs/stash` exists, bit | 1857-1863 |
| 14 | ID_STASH_LIST | Stash List | same as #13 | 1865 |
| — | sep | | any of #12 to #14 | 1872 |
| 15 | ID_BISECTGOOD | Bisect good | bisect, E has no `refs/bisect/*`, bit | 1878-1885 |
| 16 | ID_BISECTBAD | Bisect bad | same as #15 | 1887 |
| 17 | ID_BISECTSKIP | Bisect skip | same as #15 | 1892 |
| 18 | ID_BISECTRESET | Bisect reset | WC, bisect, bit | 1899-1905 |
| — | sep | | any of #15 to #18 | 1908 |
| 19 | ID_PULL | &Pull... | WC, !merge, bit | 1916 |
| 20 | ID_FETCH | F&etch... | WC, bit | 1919 |
| 21 | ID_SUBMODULE_UPDATE | Submodule Update | WC, `.gitmodules` exists, bit | 1922 |
| — | sep | | WC (unconditional) | 1925 |
| 22 | ID_CLEANUP | &Clean up... | WC, bit | 1927 |
| — | sep | | WC (unconditional) | 1930 |
| 23 | ID_LOG | Show log... (default item in the reflog) | !WC, and (bit, or (refs(E) and !stash), or Shift held) | 1954-1959 |
| 24 | ID_REPOBROWSE | &Browse repository | !WC, bit | 1961 |
| 25 | ID_MERGEREV | &Merge to "<cur>"... | !WC, bit, !HEADc, hasWC, !merge, !stash | 1964-1974 |
| 26 | ID_RESET | R&eset "<cur>" to this... | !WC, bit, hasWC, !stash | 1976-1979 |
| 27 | ID_SWITCHBRANCH | `&Switch/Checkout to "<branch>"`, or a submenu **&Switch/Checkout to** of branch short names | !WC, refs(E), bit, hasWC, !stash, and at least one `refs/heads/*` ref other than cur (only the ref under the cursor, if one is hit) | 1982-2027 |
| 28 | ID_SWITCHTOREV | Sw&itch/Checkout to this... | !WC, bit, !HEADc, hasWC, !stash | 2029-2036 |
| 29 | ID_CREATE_BRANCH | Create Br&anch at this version... | !WC, bit, !stash | 2038-2046 |
| 30 | ID_CREATE_TAG | Create &Tag at this version... | !WC, bit, !stash | 2048 |
| 31 | ID_REBASE_TO_VERSION | Rebase "<cur>" onto this...(&G) | !WC, E≠HEAD, hasWC, !merge, !stash, bit | 2051-2055 |
| 32 | ID_EXPORT | E&xport this version... | !WC, bit | 2057 |
| 33 | ID_REVERTREV | &Revert change by this commit (P>1: submenu of Parent labels) | !WC, bit, hasWC, !merge, !stash | 2060-2074 |
| 34 | ID_EDITNOTE | Edit &Notes | !WC, bit, !stash | 2076 |
| — | sep | | !WC (unconditional) | 2079 |
| **C** | **Reflog rows** (`E.m_Ref` set), no mask check | | | |
| 35 | ID_REFLOG_DEL | Delete Ref... | always | 2085 |
| 36 | ID_REFLOG_STASH_APPLY | Stash Apply | n==1, ref starts with `refs/stash` | 2086-2087 |
| — | sep | | n≤2 | 2088 |
| **D** | **n≥2** | | | |
| 37 | ID_COMPARETWO | &Compare revisions | (n==2 or contiguous selection), bit | 2095-2101 |
| 38 | ID_GNUDIFF2 | Show changes as &unified diff | n==2, bit, hasWC | 2106 |
| 39 | ID_LOG_VIEWRANGE | Show log of `<last>..<first>` (short hashes) | n==2, !WC | 2112-2119 |
| 40 | ID_LOG_VIEWRANGE_REVERSE | Show log of `<first>..<last>` | same as #39 | 2120 |
| 41 | ID_LOG_VIEWRANGE_REACHABLEFROMONLYONE | Show log of `<last>...<first>` | same as #39 | 2122 |
| 42 | ID_COMPARETWOCOMMITCHANGES | Comp&are change sets | n==2, bit | 2128-2132 |
| — | sep | | any of #37 to #42 | 2134 |
| 43 | ID_REVERTREV | &Revert changes by these commits | bit, hasWC, !merge (no separator follows: a quirk, the flag is already reset) | 2140-2144 |
| 44 | ID_BISECTSKIP + sep | Bisect skip | n>1, bisect, bit, E has no bisect ref | 2147-2151 |
| **F** | **!WC** | | | |
| 45 | ID_COMBINE_COMMIT | Com&bine to one commit | n≥2, contiguous, bit, hasWC, !merge, HEAD visible, and the first and last selected are exactly `HEAD~a` / `HEAD~b` | 2156-2178 |
| 46 | ID_CHERRY_PICK | n≥2: Cherr&y Pick selected commits... ; n==1: Cherr&y Pick this commit... | bit, !HEADc, hasWC, !merge | 2180-2186 |
| 47 | ID_CREATE_PATCH | &Format Patch... | !stash, (n≤2 or contiguous), bit | 2188-2192 |
| — | sep | | any of #45 to #47 | 2194 |
| 48 | ID_BISECTSTART + sep | Bisect start | hasWC, !merge, !stash, bit, n==2, !WC, !bisect | 2198-2202 |
| **H** | **n==1** | | | |
| 49 | ID_PUSH | Pus&h... (when the cursor is on a local branch or tag label: `Pus&h "<name>"...`) | bit, ((refs(E) and !stash) or Shift held), and (E has a `refs/heads/` or `refs/tags/` ref, or Shift held) | 2207-2226 |
| 50 | ID_SVNDCOMMIT | SVN DCommit... | after #49, bit, `.git/svn` exists | 2228 |
| 51 | ID_PULL | &Pull... | bit, HEADc, !merge, hasWC | 2234 |
| 52 | ID_DELETE | One ref: `&Delete <full refname>`. Several: submenu **&Delete branch/tag** → each full ref name, then All | bit, refs(E) other than `refs/heads/<cur>` (only the hovered ref, if one is hit) | 2241-2285 |
| — | sep | | any of #49 to #52 | 2286 |
| 53 | ID_TOGGLE_ROLLUP + sep | Collapse revisions / Expand collapsed revisions (when E is rolled up) | bit, merge-point filter on, no text filter, !WC | 2289-2293 |
| **I** | **Always** | | | |
| 54 | submenu **Copy to clipboard** | Normal log: Full data, Full commit data without changed paths, SHA-1, Authors (full), Author names, Author email addresses, Subjects, Messages, and Tag/branch names (n==1 and refs(E)). Reflog: Full data, SHA-1, Messages | bit ID_COPYCLIPBOARD | 2297-2332 |
| 55 | ID_FINDENTRY | Search &log messages... | bit | 2334 |
| 56 | ID_SHOWBRANCHES | Show branches this commit is &on | n==1, bit, !WC | 2337 |

### 2.3 Actions (`CGitLogList::ContextMenuAction`, LLA:155-1157)
Sub-IDs are encoded as `cmd = ID | (k << 16)`. Shift held means the alternative diff tool.

| id | action |
|---|---|
| COMMIT | Commit dialog on the log path, then refresh (169-180) |
| MERGE_ABORT | MergeAbort dialog (reset `--merge` / `--mixed` / `--hard`) (`AppUtils.cpp:3396`) (181-186) |
| GNUDIFF1 | k=0 or 1: unified diff `parent1..E`; k=n: `parent_n..E`. 0xFFFF (All Parents): combined `git diff -m`-style (bMerge). 0xFFFE (Only Merged Files): `--cc` (bCombine). 0xFFFD (Show extra changes after merge): `git diff-tree --cc <E>` into TortoiseGitUDiff, or the message "no changes after merge". WC row: `HEAD..WC`. Follows renames when the log is for one file with follow on (187-274) |
| GNUDIFF2 | Unified diff `<last>..<first>` (276-298) |
| COMPARETWO | `CGitDiff::DiffCommit(path, first, last)`: the file-diff dialog, or TortoiseGitMerge for a single file (300-333) |
| COMPARE | DiffCommit(WC, E) (335-369) |
| COMPAREWITHPREVIOUS | DiffCommit(E, E^k), with k=1 by default (371-428) |
| COMPARETWOCOMMITCHANGES | `git format-patch --stdout <X>~1..<X>` for both commits, then an external diff of the two `.patch` files (429-452) |
| LOG_VIEWRANGE* | `/command:log /path:<path> /range:<last>..<first>`; `<first>..<last>` (reverse); `<last>...<first>` (453-474) |
| COPYCLIPBOARD* | see §2.4 (475-486) |
| COPYCLIPBOARDBRANCHTAG | Hovered ref: a tag gives its name without `refs/tags/` or `^{}`; anything else gives `StripRefName` (drops `refs/heads/`, otherwise `refs/`). No ref hovered: every full ref name with `^{}` stripped, each followed by `\r\n` (487-518) |
| EXPORT | Export dialog for E, preferring E's first tag (519-527) |
| CREATE_BRANCH / CREATE_TAG | Create-branch / create-tag dialog at the hovered ref, else the first `refs/remotes/*` ref of E (for tracking), else E's hash (528-545) |
| SWITCHTOREV | Switch/Checkout dialog preset to the hovered ref, the first remote ref, or the hash (546-560) |
| SWITCHBRANCH | Direct `git checkout --end-of-options <branch> --` through a progress dialog (`AppUtils.cpp:1288-1310`) (561-575) |
| RESET | Reset dialog (soft/mixed/hard) to E (576-585) |
| REBASE_PICK / EDIT / SQUASH / SKIP | Set the rebase action on every selected row (586-597) |
| COMBINE_COMMIT | Requires a clean tree. `git reset --hard <first> --`, `git reset --soft <last> --`, then the Commit dialog in amend mode with the concatenated messages and the author date per reg `SquashDate` (0 = last, 1 = first, 2 = now). Then cherry-pick `first..oldHEAD`. On error, `git reset --hard <oldHEAD> --` (598-722) |
| CHERRY_PICK | RebaseDlg in cherry-pick mode, with the selection marked Pick and upstream = current branch (724-751) |
| REBASE_TO_VERSION | RebaseDlg with upstream = E's first local branch, else its first ref, else the hash (752-782) |
| STASH_SAVE / STASH_POP | Stash dialog then `git stash push…` / `git stash pop` (784-792) |
| STASH_LIST | `/command:reflog /ref:refs/stash` (794-796) |
| REFLOG_STASH_APPLY | `git stash apply <ref>` (798-800) |
| REFLOG_DEL | Re-reads the reflog. If the view is stale, shows an error asking the user to refresh (F5). Confirms (button Delete / Abort). For each ref in reverse order: `git stash drop -- <stash@{n}>` or `git reflog delete -- <ref@{n}>` (802-863) |
| LOG | `/command:log /path:"<repo root>" /endrev:<E> /rev:<E>` (864-873) |
| CREATE_PATCH | `/command:formatpatch /path:"<root>" /startrev:<E>`. With n>1: `/startrev:<oldest>~1 /endrev:<newest>` (874-905) |
| BISECTSTART | Bisect-start dialog with good = last (its first ref, if any), bad = first (906-922) |
| BISECTGOOD / BAD / SKIP / RESET | `git bisect good\|bad <hash>` (no hash for the WC row); `git bisect skip <hash…>`; `git bisect reset` (923-957) |
| REPOBROWSE | `/command:repobrowser /path:"<root>" /rev:<E>` (958-964) |
| PUSH | Push dialog with the local branch preset: the hovered ref (unless remote), else the first `refs/heads/*`, else the first `refs/tags/*`, with `^{}` stripped. "Push all" comes from reg `Push\<wd>\AllBranches` (965-990) |
| PULL / FETCH / SVNDCOMMIT | The corresponding dialogs (991-1008) |
| CLEANUP | `/command:cleanup /path:"<root>"` (1009-1015) |
| SUBMODULE_UPDATE | `/command:subupdate /bkpath:"<root>"` (1016-1022) |
| SHOWBRANCHES | Modeless "Commit is on refs" dialog (1023-1030) |
| DELETE | `CAppUtils::DeleteRef` asks for confirmation. A remote branch is deleted on the remote (push delete); a local branch or tag is deleted with libgit2; a stash is dropped. "All" deletes every ref except the current branch (1031-1067) |
| TOGGLE_ROLLUP | Toggle the collapse state of E and refresh (1069-1085) |
| FINDENTRY | Opens the Find dialog (modeless) (1087-1103) |
| MERGEREV | Merge dialog preset to the hovered ref, else E's first ref, else the hash (1104-1118) |
| REVERTREV | Confirm (default No). For each selected commit: `git revert --no-edit --no-commit [-m k] <hash>` (`Git/Git.cpp:3354-3357`); the merge messages are concatenated into MERGE_MSG. Then a box with OK / Commit; Commit opens the Commit dialog (1119-1147, 50-117) |
| EDITNOTE | Notes input dialog: `git notes` add/remove (1148-1154) |

### 2.4 Clipboard formats (`CopySelectionToClipBoard`, LLB:2378-2490). Encoding: plain text, CRLF.
- **Full data**, repeated for each selected entry and concatenated:
  `Revision: <full sha>\r\nAuthor: <name> <<email>>\r\nDate: <date as in the list (date format or relative)>\r\nMessage:\r\n<subject>\r\n<body, CRLF>\r\n<notes/tags><paths>\r\n`
  - notes/tags: `----\r\nNotes:\r\n<notes>` if there are notes, then `----\r\nTag Info:\r\n<tag info>` if E is tagged.
  - paths: `----\r\n` followed by `<ActionName>: <path>[ (from <oldpath>)]` lines joined with `\r\n`, trimmed, then `\r\n`.
- **Full commit data without changed paths**: the same, without the paths block.
- **SHA-1**: full hashes joined with `\r\n`.
- **Authors (full)**: `<name> <<email>>` joined with `\r\n`. **Author names** and **Author email addresses**: likewise, one field each.
- **Subjects**: `* <subject trimmed>\r\n\r\n` for each entry. **Messages**: `* <subject>\r\n<body>\r\n\r\n` for each entry.
- The RefLog list overrides these formats (`TortoiseProc/refloglist.cpp:146-184`); see §4.1.

---

## 3. Log dialog: other context menus (`TortoiseProc/LogDlg.cpp:1361-1590`)

### 3.1 Ref label (`IDC_STATIC_REF`, the branch label at the top left) (:1368-1456)
"Disabled" here means `EnableMenuItem(id, MF_GRAYED)`.

| cmd | label | state | action |
|---|---|---|---|
| 1 (default) | Browse References | | `OnBnClickedBrowseRef` opens the Browse References picker, then sets the range (:1375, 1408-1411) |
| 2 | `HEAD`, or `HEAD -> "<branch>"` when on a branch | | Range = current branch (:1377-1380, 1419-1421) |
| 3 | FETCH_HEAD | disabled if FETCH_HEAD does not resolve | Range = FETCH_HEAD hash (:1381-1384, 1423-1425) |
| 4 | All | disabled while Follow renames is on | All branches (`LOG_INFO_ALL_BRANCH`) (:1385-1386, 1427-1432) |
| 5 | All basic refs | same as #4 | `LOG_INFO_BASIC_REFS` (:1387-1388) |
| 6 | All local branches | same as #4 | `LOG_INFO_LOCAL_BRANCHES` (:1389-1390) |
| — sep, then 7…n | each range-history entry (truncated to 150 chars + `...`) | shown if there is history | Set that range and move it to the front of the history (:1392-1402, 1445-1450) |

Choosing any of 2 to n first clears the all-branch flags. All of them then refresh the log.

### 3.2 From date control (`IDC_DATEFROM`) (:1458-1533)
The To date control has no menu.

| cmd | label | shown when | action |
|---|---|---|---|
| 1 | No limitation | always | Scale = no limit; clear the remembered from-date |
| — sep, 2 | `Last %s commit(s)` / `year(s)` / `month(s)` / `week(s)` with N | reg `LogDialog\NumberOfLogsScale` ≠ no limit and N>0 | Apply that default limit |
| — sep, last | Configure default | always | `/command:settings /page:dialog` |

### 3.3 Message pane (`IDC_MSGVIEW`, a rich edit) (:1535-1590)
Shown only when exactly one log row is selected.

| id | label | state | action |
|---|---|---|---|
| WM_COPY | Cop&y | disabled (greyed) when nothing is selected | Copy the selected text as plain text |
| — sep | | | |
| EM_SETSEL | Copy all information to clipboard | | Copy the whole pane text, keeping the scroll position |
| — sep, ID_EDITNOTE | Edit &Notes | shown if the row is a real commit | Notes dialog, then re-render |

The changed-files list in LogDlg is the status list from §1, with the mask in §1.2. The Filter, Walk-behaviour and View buttons open drop-down menus from a button click, not context menus (:2205+, 3360+, 3428+). They are not covered here.

---

## 4. RefLog dialog and Browse References

Labels are quoted verbatim from the `.rc` files; `&` marks the mnemonic letter.

### 4.1 RefLog dialog (`/command:reflog`), commit list

`CRefLogDlg` has no menu of its own (no `OnContextMenu` in `TortoiseProc/RefLogDlg.cpp`). Its list is a `CRefLogList`, a subclass of `CGitLogList`, so the menu is the §2 log-list menu with these changes:

| Aspect | Value | src |
|---|---|---|
| ContextMenu mask | Base default (`UINT64_MAX` minus REBASE_PICK/SQUASH/EDIT/SKIP, LOG, BLAME, BLAMEPREVIOUS), then `\|= ID_LOG` and `&= ~ID_COMPARETWOCOMMITCHANGES` | TortoiseProc/refloglist.cpp:32-33, TortoiseProc/GitLogListBase.cpp:94-100 |
| `m_ColumnRegKey` | `"reflog"`. Several branches in the shared menu code test this value. | TortoiseProc/refloglist.cpp:31 |
| `m_hasWC` | `!IsBareRepo(cwd)` | TortoiseProc/RefLogDlg.cpp:89 |
| Working-tree row | None. The list is filled only from `GetRefLog(ref)`. | TortoiseProc/RefLogDlg.cpp:143-154 |

Entries that only the reflog list has, or that behave differently there:

| id | label | shown when | disabled? | action | src |
|---|---|---|---|---|---|
| ID_LOG | "Show log..." | 1 selected and the commit hash is non-empty. Always passes in the reflog because the mask bit is set. | no; it is made the **default (bold) item** when `m_ColumnRegKey=="reflog"` | `/command:log /path:"<repo>" /endrev:<hash> /rev:<hash>` | TortoiseProc/GitLogListBase.cpp:1954-1958, TortoiseProc/GitLogListAction.cpp:864-872 |
| ID_REFLOG_DEL | "Delete Ref..." | `pSelLogEntry->m_Ref` is non-empty (every reflog row) and any selection count | no | (1) Reload the reflog. If the count, first/last hash or first date changed, show the error "The current view seems to be out of date. Please refresh, e.g. by pressing F5 and recheck the selection." (2) Confirm with IDS_PROC_DELETEREF `"%s" will be permanently deleted. It can NOT be recovered!\n\nDo you really want to continue?` (1 ref) or IDS_PROC_DELETENREFS `Do you really want to permanently delete the %d selected refs? It can NOT be recovered!`. Buttons are Delete/Abort. (3) For each selected ref, last to first: strip `refs/`, rewrite `x{n}` as `x@{n}`, then run `git.exe stash drop -- <ref>` if the ref starts with `stash`, otherwise `git.exe reflog delete -- <ref>`. Post MSG_REFLOG_CHANGED, which triggers a reload. | TortoiseProc/GitLogListBase.cpp:2083-2085, TortoiseProc/GitLogListAction.cpp:802-863 |
| ID_REFLOG_STASH_APPLY | "Stash Apply" | 1 selected and `m_Ref` starts with `refs/stash` | no | `CAppUtils::StashApply(hwnd, m_Ref)`, which runs `git stash apply <ref>` | TortoiseProc/GitLogListBase.cpp:2086-2087, TortoiseProc/GitLogListAction.cpp:798-800 |
| separator | | after the two entries above, if selectedCount <= 2 | | | TortoiseProc/GitLogListBase.cpp:2088-2089 |
| ID_COMPARETWOCOMMITCHANGES | (hidden) | never, because the mask bit is cleared | | | TortoiseProc/refloglist.cpp:33 |
| "Copy to clipboard" submenu | "Copy to clipboard" | ID_COPYCLIPBOARD bit set and the list is the reflog. It holds only 3 children (below), not the full §2 set. | no | see below | TortoiseProc/GitLogListBase.cpp:2323-2331 |
| ↳ ID_COPYCLIPBOARDFULL | "Full data" | always | | For each selected row: `Revision: <hash>\r\nDate: <date>\r\nMessage: <refaction>: <subject+body>\r\n`. The `<refaction>: ` part is dropped when the action is empty. | TortoiseProc/refloglist.cpp:146-166 |
| ↳ ID_COPYCLIPBOARDHASH | "SHA-1" | always | | Full hashes joined by `\r\n` | TortoiseProc/refloglist.cpp:179-184 |
| ↳ ID_COPYCLIPBOARDMESSAGES | "Messages" | always | | `* <refaction>: <subject+body>\r\n\r\n` for each row | TortoiseProc/refloglist.cpp:167-177 |

All other §2 entries (compare, diff, branch/tag, reset, switch, cherry-pick and so on) appear in the reflog under the normal §2 conditions. The reflog's "Show log" is driven by the ID_LOG bit.

### 4.2 Browse References (`/command:refbrowse`): tree and list menu

Both menus come from one builder, `ShowContextMenu(point, hTreePos, selectedLeafs)` at TortoiseProc/BrowseRefsDlg.cpp:910-1136.

- **Tree** (left pane): right-clicking selects the item under the cursor. `selectedLeafs` is always empty there, so only the tree-node section and "Copy ref names" can appear (`:859-870`).
- **List** (right pane): passes the selected rows together with the tree's current selection (`:882-887`). With no rows selected it shows the same menu as the tree.

Terms used below: *leaf* = one selected ref row. `bIsCommit` = the leaf's object type is a commit (an annotated tag is not). `m_bHasWC` = the repo is not bare. `m_bWantPick` = the dialog was opened as a picker.

**One leaf selected** (`selectedLeafs.size()==1`)

| id | label | shown when | action | src |
|---|---|---|---|---|
| eCmd_Select | "Select" | `m_bWantPick`. A separator follows. | `EndDialog(IDOK)`, which returns the ref | :950-954, :1139 |
| eCmd_ViewLog | "Show &log" | bIsCommit | `/command:log /path:"<repo>" /range:<FixBranchName(ref)>` | :955-956, :1142-1147 |
| eCmd_RepoBrowser | "&Browse repository" | always. `SetDefaultItem(0, TRUE)` makes the **first** item of the menu the default. | `/command:repobrowser /path:"<repo>" /rev:<fullref>` | :957-958, :1163-1165 |
| eCmd_ShowReflog | "Show &RefLog" | ref is under `refs/heads/` or `refs/remotes/` | Modal RefLog dialog, `m_CurrentBranch=<fullref>` | :959-960, :1181-1186 |
| separator + eCmd_DiffWC | "Compare with &working tree" | m_bHasWC && bIsCommit | `/command:showcompare /path:"<repo>" /revision1:<ref> /revision2:<working-copy ref>`. Holding Shift adds `/alternative`. | :962-966, :1278-1286 |
| separator | | always | | :968 |
| eCmd_Fetch | `Fetch from "%s"` (%s = remote) | ref is under `refs/remotes/` and the remote/branch split succeeds. A separator follows. | `CAppUtils::Fetch(hwnd, remote)`: opens the Fetch dialog with that remote preselected, then refreshes | :935-941, :971-978, :1188-1192 |
| eCmd_Merge | `&Merge to "%s"...` (%s = current branch) | m_bHasWC && bIsCommit && ref != `refs/heads/<current>` | `CAppUtils::Merge(hwnd, &ref)`: opens the Merge dialog | :981-988, :1204-1208 |
| eCmd_Switch | "Sw&itch/Checkout to this..." | m_bHasWC && bIsCommit. A separator follows. | `CAppUtils::Switch(hwnd, ref)`: opens the Switch/Checkout dialog | :989-990, :1210-1213 |
| eCmd_CreateBranch | "Create &Branch..." | ref is under `refs/remotes/` | Create Branch dialog with the start point set to the leaf hash, then refresh | :993-997, :1236-1243 |
| eCmd_EditBranchDescription | "Edit description" | ref is under `refs/heads/` and bIsCommit | Input dialog prefilled with the current description, then `UpdateBranchDescription`, which writes `branch.<name>.description` | :999-1003, :1288-1300 |
| eCmd_Rename | "Rename" | ref is under `refs/heads/` | In-place label edit, then `git.exe branch -m <old> -- <new>` | :1004-1008, :1215-1220, :1591 |
| separator (if anything above) + eCmd_UpstreamDrop | "Unset tracked branch" | m_bHasWC && bIsCommit && ref under `refs/heads/` && upstream is set | Unsets `branch.<b>.remote` and `branch.<b>.merge` in config | :1010-1016, :1302-1310 |
| eCmd_UpstreamSet | "Select tracked branch" | m_bHasWC && bIsCommit && ref under `refs/heads/` | Opens a ref picker for remote branches, then `git.exe branch --set-upstream-to=<remote>/<branch> -- <b>` | :1017, :1312-1336 |

**Two leaves selected**

| id | label | action | src |
|---|---|---|---|
| eCmd_Diff | "Compare selected refs" | Changed-files dialog (`CFileDiffDlg`) between ref0 and ref1 | :1021-1023, :1263-1271 |
| eCmd_UnifiedDiff | "Show changes as &unified diff" | Unified diff between the two hashes. Holding Shift selects the alternative viewer. | :1024, :1273-1276 |
| eCmd_ViewLogRange | "Show log of %s", with `A..B` | `/command:log /range:A..B`. B is the last-clicked ref, A is the other one. | :1025-1027, :1149-1154 |
| eCmd_ViewLogRangeReachableFromOnlyOne | "Show log of %s", with `A...B` | `/command:log /range:A...B` | :1028-1029, :1156-1161 |

**Delete entries** (one or more leaves). These are mutually exclusive and each is preceded by a separator if the menu already has entries.

| id | label | shown when | action | src |
|---|---|---|---|---|
| eCmd_DeleteRemoteBranch | "Delete remote branch" / "Delete %d remote branches" | all leaves are under `refs/remotes/` | Confirm, then delete on the server. libgit2 does a push of `:refs/heads/<b>`; the fallback is `git push -- <remote> :refs/heads/<b>...`, batched per remote. | :1032-1045, :726-765, Git/Git.cpp:2124-2126 |
| eCmd_DeleteBranch | "Delete branch" / "Delete %d branches" | all leaves are under `refs/heads/` | Confirm, then `git.exe branch -D -- <b>` (or the libgit2 equivalent) | :1046-1058, Git/Git.cpp:3404 |
| eCmd_DeleteTag | "Delete tag" / "Delete %d tags" | all leaves are under `refs/tags/` | Confirm, then `git.exe tag -d -- <t>` | :1059-1071, Git/Git.cpp:3406 |

The confirmation (`:661-724`) works as follows:

- **One branch:** `Do you really want to delete "%s"?`. If the ref is not fast-forward to HEAD, IDS_PROC_BROWSEREFS_WARNINGUNMERGED is appended and the icon becomes a warning.
- **Many refs:** IDS_PROC_DELETENREFS plus a no-merge-check warning.
- **Remote branches:** IDS_PROC_BROWSEREFS_WARNINGDELETEREMOTEBRANCHES is added.
- Buttons are Yes/No.

**Tree node, no leaves selected** (`hTreePos && selectedLeafs.empty()`)

| id | label | shown when | action | src |
|---|---|---|---|---|
| eCmd_ManageRemotes | "Manage Remotes" | node is under `refs/remotes` | Settings page "Git > Remote" as a single property-sheet dialog | :1077-1084, :1228-1234 |
| eCmd_Fetch | `Fetch from "%s"` | node is a configured remote's folder | `CAppUtils::Fetch(hwnd, remote)` | :1085-1095 |
| eCmd_DeleteRemoteTag \| idx<<16 | "Delete remote tags..." | node is a configured remote's folder | `CDeleteRemoteTagDlg` for that remote | :1097-1098, :1194-1202 |
| eCmd_CreateBranch | "Create &Branch..." | node is under `refs/heads` | Create Branch dialog (no preset start point) | :1102-1110 |
| eCmd_CreateTag | "Create &Tag..." | node is under `refs/tags` | Create Tag dialog | :1111-1116, :1245-1249 |
| eCmd_DeleteAllTags | "Delete all tags" | node is under `refs/tags` | Selects every list row, confirms, then deletes them all | :1117, :1251-1261 |
| separator + eCmd_DeleteRemoteTag \| i<<16 | `Delete remote tags on "%s"...` (one per remote) | node is under `refs/tags` and remotes exist | `CDeleteRemoteTagDlg` for remote i | :1118-1128 |

**Always shown last**

| id | label | action | src |
|---|---|---|---|
| eCmd_Copy | "Copy ref names" | Copies the selected leaves' short ref names (`StripRefName`), joined by `\r\n`. A separator precedes it if needed. On the tree, nothing is selected, so nothing is copied. | :1132-1134, :1338-1349 |

`eCmd_AddRemote` has a handler (`:1222-1226`) but is never added to the menu.

**Other Browse References popups**

- **OK button with 2 refs selected** (log-range mode, `m_bShowRangeOptionWithTwoRefs`, not a picker): a popup anchored below the button (`:223-275`). Item 1 is the selected ref and is the default. Items 2 and 3 are `A..B` and `B..A`. Item 4 is `A...B`. Choosing one returns that range.
- **Filter info-icon menu** (`:1641-1682`): checkable items "Refname", "Subject", "Authors", "SHA-1", then a separator, then "Toggle filters", which inverts all four.

## 5. Sync dialog

The Sync dialog (`TortoiseProc/SyncDlg.cpp`) adds no menu code of its own. Every right-click menu comes from a reused control configured in `OnInitDialog`:

| Tab (label) | Control | Menu configuration | src |
|---|---|---|---|
| "In Commits" | `CGitLogList m_InLogList`, `m_ColumnRegKey="SyncIn"` | **ContextMenu mask not modified.** It uses the base default: all bits except REBASE_PICK/SQUASH/EDIT/SKIP, LOG, BLAME, BLAMEPREVIOUS. `m_hasWC` keeps its default `true`. There is no working-tree row (the list is filled with `FillGitLog(range)`). The full §2 menu applies. | TortoiseProc/SyncDlg.cpp:1030-1041, :541, TortoiseProc/GitLogListBase.h:274-276 |
| "In Changelist" | `CGitStatusListCtrl m_InChangeFileList` | `Init(cols, "InSyncDlg", GetContextMenuBit(IDGITLC_COMPARETWOREVISIONS) \| GetContextMenuBit(IDGITLC_GNUDIFF2REVISIONS), bHasCheckboxes=false, bHasWC=false)`. That leaves only "compare two revisions" and "unified diff two revisions", plus whatever §1 shows without a mask bit (such as the copy submenu). | TortoiseProc/SyncDlg.cpp:1053-1055 |
| "Conflicts" | `CGitStatusListCtrl m_ConflictFileList` | `GITSLC_POPEXPLORE \| GITSLC_POPOPEN \| GITSLC_POPSHOWLOG \| GITSLC_POPCONFLICT \| GITSLC_POPRESOLVE`, no checkboxes, bHasWC defaults to true | TortoiseProc/SyncDlg.cpp:1068-1070 |
| "Out Commits" | `CGitLogList m_OutLogList`, `m_ColumnRegKey="SyncOut"` | Same as In Commits: default mask, the full §2 menu | TortoiseProc/SyncDlg.cpp:1077-1089, :1432 |
| "Out Changelist" | `CGitStatusListCtrl m_OutChangeFileList` | Same mask as In Changelist (`"OutSyncDlg"`, COMPARETWOREVISIONS \| GNUDIFF2REVISIONS, bHasWC=false) | TortoiseProc/SyncDlg.cpp:1102-1104 |
| "Log" | `CGitProgressList m_GitProgressList` | Progress-list menu (table below) | TortoiseProc/SyncDlg.cpp:1106-1112 |

**`CGitProgressList` menu** (TortoiseProc/GitProgressList.cpp:864-923)

The menu is suppressed in dry-run mode, while the operation thread is still running, or when nothing is selected. With exactly one row selected, the row's own entries come first. Those are shown only for file notifications whose action is Add, Revert, Resolved, Checkout, LFS_Lock or LFS_Unlock (`:1142-1199`).

| label | shown when | action | src |
|---|---|---|---|
| "Show &log" | per-row entries apply | `/command:log /path:<file>` | :1150-1158 |
| separator | per-row entries apply | | :1160 |
| "&Open" / "Open with..." | per-row entries apply and the path is not a directory | ShellOpen / the "Open with" dialog | :1161-1167 |
| "Explore to" | per-row entries apply | Opens the file manager with the file selected | :1169-1170 |
| separator + "LFS Lock" / "LFS Unlock" | the row's action is LFS lock or unlock | `/command:lfslock` or `/command:lfsunlock` with `/pathfile:"<tmp>" /deletepathfile` | :1172-1196 |
| separator | any per-row entry was added | | :895-896 |
| "Copy to clipboard" | always, for any number of selected rows | Copies the path column of the selected rows, joined by `\r\n` | :897-915 |

The Pull/Fetch/Push split-button dropdowns in Sync are button menus, not context menus. They are left to the dialogs spec.

## 6. Rebase dialog commit list

The commit list is a `CGitLogList` (`m_CommitList`, `m_ColumnRegKey="Rebase"`, `m_IsIDReplaceAction`, `m_IsRebaseReplaceGraph`, `m_bNoHightlightHead`, `m_bIsCherryPick = m_IsCherryPick`). The §2 menu applies with these mask changes:

| When | Mask change | src |
|---|---|---|
| Constructor default | Clears REBASE_PICK/SQUASH/EDIT/SKIP, LOG, BLAME, BLAMEPREVIOUS | TortoiseProc/GitLogListBase.cpp:94-100 |
| `OnInitDialog` | Clears `ID_CHERRY_PICK`, `ID_SWITCHTOREV`, `ID_RESET`, `ID_REVERTREV`, `ID_MERGEREV`, `ID_REBASE_TO_VERSION`, `ID_REVERTTOREV`, `ID_COMBINE_COMMIT` and `ID_TOGGLE_ROLLUP` | TortoiseProc/RebaseDlg.cpp:327-335 |
| `SetControlEnable` in stage Choose_Branch or Choose_Commit_Pick_Mode | Sets `ID_REBASE_PICK`, `ID_REBASE_SQUASH`, `ID_REBASE_EDIT`, `ID_REBASE_SKIP` and `ID_LOG`. **They are never cleared again.** After the rebase starts the entries stay, but they are hidden per row once the row is CURRENT or DONE. | TortoiseProc/RebaseDlg.cpp:1844-1866 |

**Rebase-specific entries.** These come first in the menu, before the §2 single-selection block.

| id | label | shown when | disabled? | action | src |
|---|---|---|---|---|---|
| ID_REBASE_PICK | "Pick" | mask bit set && the *focused* entry is not REBASE_CURRENT or REBASE_DONE | no | `SetSelectedRebaseAction(PICK)` on every selected row, skipping CURRENT and DONE rows | TortoiseProc/GitLogListBase.cpp:1726-1727, TortoiseProc/GitLogListAction.cpp:586-587, :1300-1318 |
| ID_REBASE_SQUASH | "Squash (with commit below)" | as above, plus neither the first nor the last selected index is the last row, plus (cherry-pick mode or the entry has exactly 1 parent) | no | `SetSelectedRebaseAction(SQUASH)`. It skips the last row, and outside cherry-pick mode it skips merge commits. | GitLogListBase.cpp:1729-1730, GitLogListAction.cpp:592-593 |
| ID_REBASE_EDIT | "Edit" | same condition as Pick | no | Sets the action to EDIT | GitLogListBase.cpp:1732-1733, GitLogListAction.cpp:589-590 |
| ID_REBASE_SKIP | "Skip" | same condition as Pick | no | Sets the action to SKIP | GitLogListBase.cpp:1735-1736, GitLogListAction.cpp:595-596 |
| separator | | any of the four bits set && the entry is not CURRENT or DONE | | | GitLogListBase.cpp:1738-1739 |
| ID_LOG | "Show log..." | 1 selected and the hash is non-empty (enabled by the stage mask) | no | `/command:log /path:"<repo>" /endrev:<h> /rev:<h>` | GitLogListBase.cpp:1954-1956 |

Each action change posts `m_RebaseActionMessage` to the dialog (`GitLogListAction.cpp:1317`).

Other §2 entries still apply in the rebase list, for example "Compare with previous", "Copy to clipboard", "Create branch/tag", "Export" and "Delete ref". Entries whose bits were cleared at `RebaseDlg.cpp:327-335` are absent.

**Related non-context controls** on the same list. These are included because they change the same per-row action:

| control | label | condition | action | src |
|---|---|---|---|---|
| IDC_SPLITALLOPTIONS (split button) | "&Pick ALL", "&Squash ALL", "&Edit ALL", "S&kip unselected", "S&quash unselected", "E&dit unselected" | enabled only in the Choose_* stages | **Set all:** `SetAllRebaseAction(a)`; squash skips the last row and, outside cherry-pick mode, any row that does not have exactly 1 parent. **Set unselected:** `SetUnselectedRebaseAction(a)` with the same squash exclusions plus CURRENT/DONE rows. | TortoiseProc/RebaseDlg.cpp:226-241, :2975-3000, :363-372, TortoiseProc/GitLogListAction.cpp:1320-1341 |
| IDC_BUTTON_UP / IDC_BUTTON_DOWN | "&Up" / "&Down" | enabled in the Choose_* stages | Moves the selected rows up or down by one, keeping them as a block. Shift moves them to the top or bottom. | TortoiseProc/RebaseDlg.cpp:2758-2835 |
| IDC_BUTTON_ADD | "&Add" | enabled in the Choose_* stages, and only when "preserve merges" is off | Log dialog in multi-select picker mode; the chosen commits are inserted | TortoiseProc/RebaseDlg.cpp:3082+ |

Keyboard shortcuts, active only while the list has focus (`PreTranslateMessage`, TortoiseProc/RebaseDlg.cpp:806-880):

| Key | Effect | Requires |
|---|---|---|
| Space | Cycles the action Pick → Skip → Edit → Squash → Pick. On the last row, Edit goes straight back to Pick. | all four rebase bits |
| P / S / Q / E | Pick / Skip / Squash / Edit | the matching mask bit |
| U / D | Up / Down | the button is enabled |
| Ctrl+A | Select all | |
| F5 | Refresh | |

The Rebase "Revision Files" list (`m_FileListCtrl`) uses mask `GITSLC_POPALL ^ (GITSLC_POPCOMMIT | GITSLC_POPRESTORE | GITSLC_POPCHANGELISTS)`, no checkboxes, `bHasWC=true` (TortoiseProc/RebaseDlg.cpp:243).

## 7. Commit message editor

The commit message box is a `CSciEdit` (Scintilla). `CCommitDlg` registers itself as a context-menu handler (TortoiseProc/CommitDlg.cpp:262). The menu is built in `CSciEdit::OnContextMenu` (Utils/MiscUI/SciEdit.cpp:1073-1268).

How the menu is built:

- Right-clicking moves the caret to the click point only temporarily, to find the word under the cursor. The selection is restored afterwards (except after "Select All").
- When the menu is opened from the keyboard, it is anchored at the centre of the control and the current position is used.
- Disabled entries are added with `MF_GRAYED`; they are shown, not hidden.

| # | id | label | shown when | disabled when | action | src |
|---|---|---|---|---|---|---|
| 1 | 1..n | *(one spelling suggestion per item)* | the control is not read-only, a spell checker is active (Windows `ISpellChecker` is preferred, otherwise Hunspell), and the word under the cursor is non-empty. A separator follows if there are suggestions. | never | Replaces the word under the cursor with the suggestion (`SCI_REPLACESEL`) | SciEdit.cpp:1124-1157, :1248-1256 |
| 2 | SCI_ADDWORD | "Add '%s' to dictionary" | not read-only, word length < PDICT_MAX_WORD_LENGTH, the word is not in the autocomplete list, the word is misspelled, it does not start with a digit, and it is not already in the personal dictionary. A separator follows. | never | Adds the word to the personal dictionary and re-checks spelling of the visible lines | SciEdit.cpp:1161-1169, :1230-1233 |
| 3 | SCI_UNDO | "&Undo" | always | nothing to undo | `SCI_UNDO` | :1171-1173 |
| 4 | SCI_REDO | "&Redo" | always | nothing to redo | `SCI_REDO` | :1174-1175 |
| – | separator | | always | | | :1177 |
| 5 | SCI_CUT | "Cu&t" | always | read-only, or no selection | cut | :1179-1180 |
| 6 | SCI_COPY | "Cop&y" | always | no selection | copy | :1181-1182 |
| 7 | SCI_PASTE | "&Paste" | always | `!SCI_CANPASTE` | paste | :1183-1184 |
| – | separator | | always | | | :1186 |
| 8 | SCI_SELECTALL | "Select &All" | always | never | select all (the caret is not restored) | :1188-1189, :1215-1217 |
| – | separator | | not read-only && an edge column is set (`SCI_GETEDGECOLUMN != 0`, i.e. the project property `tgit.logwidthmarker` is set) | | | :1191-1193 |
| 9 | SCI_LINESSPLIT | "Split lines" | same as the separator above | no selection | Joins the selected lines, then re-splits them at the edge column, i.e. re-wraps the paragraph at the width marker | :1194-1197, :1235-1247 |
| – | separator | | at least one context handler is registered (always true in the Commit dialog) | | | :1199-1200 |
| 10 | m_nPopupPickCommitHash | "Pick commit &hash" | Commit dialog | never | Opens the Log dialog in single-select picker mode without the working-tree row. The chosen full SHA-1 is inserted at the caret. | TortoiseProc/CommitDlg.cpp:2142-2144, :2169-2187 |
| 11 | m_nPopupPickCommitMessage | "Pick commit &message" | Commit dialog | never | Same picker; inserts `subject + "\r\n" + body` of the chosen commit | CommitDlg.cpp:2146-2148, :2189-2216 |
| 12 | m_nPopupPasteListCmd | "Paste &filename list" | Commit dialog | never | For every **checked** row of the file list, appends `"%-10s %s\r\n"` with (status name, column-0 path text), then inserts the result. Unversioned rows are labelled "Added". | CommitDlg.cpp:2150-2152, :2218-2242 |
| 13 | m_nPopupPasteLastMessage | "Paste &last commit message" | the commit-message history is non-empty | never | Inserts history entry 0 | CommitDlg.cpp:2154-2159, :2244-2253 |
| 14 | m_nPopupRecentMessage | "Paste r&ecent message..." | the history is non-empty | never | `OnBnClickedHistory()`, described below | CommitDlg.cpp:2161-2163, :2255-2259, :2301-2327 |

Details for the Commit-dialog entries:

- **Status names used by "Paste filename list"** come from `CTGitPath::GetActionName`, checked in this order: "Conflict", "Added", "Missing", "Deleted", "Merged", "Modified", "Rename", "Copy", "Assume valid/unchanged", "Skip worktree", "Ignored", "Unknown" (Git/TGitPath.cpp:1782-1811).
- **`bFileListInEnglish` has no effect.** The project property computes a `langID` that is never used (CommitDlg.cpp:2233-2235).
- **Where the history comes from:** it is reloaded each time the menu opens, from the registry key `Software\TortoiseGit\History\commit<repo-UUID with ':'→'_'>`, value list `logmsgs` (CommitDlg.cpp:1348-1354).
- **What "Paste recent message..." does:** it opens the History dialog. If the selected message is not already a prefix of the text, it extracts the bug ID into the Bug-ID field (when that field is visible). It then inserts the message, or replaces the text when the current text equals the log template.

Snippets are not context-menu entries. They are offered through autocompletion (`HandleSnippet`, AUTOCOMPLETE_SNIPPET), which replaces the word under the cursor with the snippet text (CommitDlg.cpp:2263-2271).

The same SciEdit base menu (entries 1-9) appears in every `CSciEdit`. The Merge dialog's message box registers its own handler that adds only entries 13 and 14 (TortoiseProc/MergeDlg.cpp:226-241).

---

## 8. TortoiseGitMerge views

Paths relative to `src/`. Labels from `Resources/TortoiseMergeENG.rc` (the TortoiseMerge strings live in `src/Resources/`, not `src/TortoiseMerge/resources/`). `&` marks the mnemonic.

**Layouts.** Two-pane diff: Left = original/base (usually read-only), Right = modified ("mine"). Three-pane merge (`IsBottomViewGood()`): Left = Theirs, Right = Mine, Bottom = Merged (the target). Unless noted, "use" commands write into the target view: Right in two-pane, Bottom in three-pane.

**Before the menu opens** (`TortoiseMerge/BaseView.cpp:2607-2666`): if the right-click lands on a line outside the current selection, the selection is cleared and set to the whole contiguous diff block around that line (`LinesInOneChange`, up and down), in all views (`SetupAllViewSelection`), and the caret moves there. `state` is the first state in the selection that is neither Normal nor Unknown. If there is none, `state` is the last line's state. With no selection, `state` is Unknown. Each view computes `bShow = HasSelection() && state != Unknown` (Left :199, Right :218, Bottom :41). A keyboard-invoked menu is anchored at the centre of the view (`CompensateForKeyboard`, :4498).

### 8.1 Header (file name bar) menu: `BaseView.cpp:2410-2454`
Applies only when the click is in the header band or the left border, **and** only in three-pane mode. In two-pane mode, a right-click there shows nothing.

| view | id | label | action | src |
|---|---|---|---|---|
| Left | 10 | Show diff separately | Launch a new TortoiseGitMerge: `"<self>" /base:<base file> /mine:<theirs file> [/basename:<base window name>] [/yourname:<theirs window name>]` (`MainFrm.cpp:1286,1296-1311`) | :2414-2425 |
| Right | 10 | Show diff separately | The same with `/mine:<your file>` (`MainFrm.cpp:1291`) | :2426-2437 |
| Bottom | 10 | Explore to | `CAppUtils::ExploreTo(m_sFullFilePath)`: opens the file manager with the merged file selected | :2438-2449 |

### 8.2 Text-area popup: assembly order (`BaseView.cpp:2457-2508`)
The text-area menu is built in this order. Nothing is shown if the view is not visible (:2455).
1. View-specific items (`AddContextItems`, §8.3 to §8.5). Each one ends by calling `CBaseView::AddContextItems`, which adds the clipboard block (§8.6).
2. If `IsWritable()` (that is, `!m_bReadonly && m_pViewData`, `BaseView.h:94`), the whitespace and encoding block (§8.7).

### 8.3 Left view: `LeftView.cpp:197-236`
| # | id | label | shown when | action (target) | src |
|---|---|---|---|---|---|
| **3-pane** | | | | | |
| 1 | POPUPCOMMAND_USETHEIRBLOCK | Use th&is text block | bShow | Bottom: `UseLeftBlock()`, which copies the Theirs block into the merged view and resolves its state | :203-204 |
| 2 | POPUPCOMMAND_USETHEIRFILE | Use this &whole file | always | Bottom: `UseLeftFile()`, the whole Theirs file | :205 |
| 3 | POPUPCOMMAND_USEYOURANDTHEIRBLOCK | Use text block from r&ight before left | bShow | Bottom: `UseBothRightFirst()`, Mine block then Theirs block | :206-208 |
| 4 | POPUPCOMMAND_USETHEIRANDYOURBLOCK | Use text block from l&eft before right | bShow | Bottom: `UseBothLeftFirst()` | :209 |
| **2-pane** | | | | | |
| 1 | POPUPCOMMAND_USELEFTBLOCK | Use th&is text block | bShow | Right: `UseLeftBlock()`, replaces the right block with the left block | :214-216 |
| 2 | POPUPCOMMAND_USEBOTHLEFTFIRST | Use both text blocks (this one first) | bShow | Right: `UseBothLeftFirst()` | :217 |
| 3 | POPUPCOMMAND_USEBOTHRIGHTFIRST | Use both text blocks (this one last) | bShow | Right: `UseBothRightFirst()` | :218 |
| sep | | — | bShow && left view exists && left not read-only | | :221 |
| 4 | POPUPCOMMAND_PREPENDFROMRIGHT | Prepend right block | same as the separator above | Left (if not read-only): `UseBothRightFirst()` | :222 |
| 5 | POPUPCOMMAND_REPLACEBYRIGHT | Use right block | same | Left: `UseRightBlock()` | :223 |
| 6 | POPUPCOMMAND_APPENDFROMRIGHT | Append right block | same | Left: `UseBothLeftFirst()` | :224 |
| sep | | — | bShow | | :226 |
| 7 | POPUPCOMMAND_USELEFTFILE | Use this &whole file | always | Right: `UseLeftFile()` | :228 |
| 8 | POPUPCOMMAND_USERIGHTFILE | Use whole other &file | left view exists && left not read-only | Left: `UseRightFile()` | :229-232 |

### 8.4 Right view: `RightView.cpp:216-273`
| # | id | label | shown when | action (target) | src |
|---|---|---|---|---|---|
| **3-pane** | | | | | |
| 1 | POPUPCOMMAND_USEYOURBLOCK | Use th&is text block | bShow | Bottom: `UseRightBlock()` | :222-223 |
| 2 | POPUPCOMMAND_USEYOURFILE | Use this &whole file | always | Bottom: `UseRightFile()` | :224 |
| 3 | POPUPCOMMAND_USEYOURANDTHEIRBLOCK | Use text block from r&ight before left | bShow | Bottom: `UseBothRightFirst()` | :227 |
| 4 | POPUPCOMMAND_USETHEIRANDYOURBLOCK | Use text block from l&eft before right | bShow | Bottom: `UseBothLeftFirst()` | :228 |
| **2-pane** | | | | | |
| 1 | POPUPCOMMAND_USELEFTBLOCK | Use &other text block | bShow | Right: `UseLeftBlock()` | :233-235 |
| 2 | POPUPCOMMAND_USEBOTHRIGHTFIRST | Use both text blocks (this one first) | bShow | Right: `UseBothRightFirst()`, right block then left block | :236 |
| 3 | POPUPCOMMAND_USEBOTHLEFTFIRST | Use both text blocks (this one last) | bShow | Right: `UseBothLeftFirst()` | :237 |
| sep | | — | bShow && left not read-only | | :240 |
| 4 | POPUPCOMMAND_PREPENDFROMRIGHT | Prepend this block to left | same as the separator above | Left: `UseBothRightFirst()` | :241 |
| 5 | POPUPCOMMAND_REPLACEBYRIGHT | Use this block on left | same | Left: `UseRightBlock()` | :242 |
| 6 | POPUPCOMMAND_APPENDFROMRIGHT | Append this block to left | same | Left: `UseBothLeftFirst()` | :243 |
| sep | | — | bShow | | :245 |
| 7 | POPUPCOMMAND_MARKBLOCK | Mark this block | a selection exists. For a single-line selection, only if that line is **not** marked. | Right: `MarkBlock(true)`, which sets the marked flag on the selected lines (`BaseView.cpp:6225`) | :248-261 |
| 8 | POPUPCOMMAND_UNMARKBLOCK | Unmark this block | a selection exists. For a single-line selection, only if that line **is** marked. A multi-line selection shows both 7 and 8. | Right: `MarkBlock(false)` | :262-263 |
| 9 | POPUPCOMMAND_LEAVEONLYMARKEDBLOCKS | Leave only marked blocks | always | Right: replaces with left content every line that is neither marked nor Edited (`BaseView.cpp:6243`) | :264 |
| 10 | POPUPCOMMAND_USELEFTFILE | Use whole other &file | always | Right: `UseLeftFile()` | :265 |
| 11 | POPUPCOMMAND_USERIGHTFILE | Use this &whole file | left not read-only | Left: `UseRightFile()` | :266-269 |

### 8.5 Bottom (merged) view, three-pane only: `BottomView.cpp:39-51`
If `!bShow`, the method returns **before** calling the base class. So with no selection the Bottom view has no clipboard block, only §8.7 (Bottom is writable).
| # | id | label | action | src |
|---|---|---|---|---|
| 1 | POPUPCOMMAND_USETHEIRBLOCK | Use text block from &left | `UseLeftBlock()`: the Theirs block | :45 |
| 2 | POPUPCOMMAND_USEYOURBLOCK | Use text block from &right | `UseRightBlock()`: the Mine block | :46 |
| 3 | POPUPCOMMAND_USEYOURANDTHEIRBLOCK | Use text block from r&ight before left | `UseBothRightFirst()` | :47 |
| 4 | POPUPCOMMAND_USETHEIRANDYOURBLOCK | Use text block from l&eft before right | `UseBothLeftFirst()` | :48 |

When a block is used in Bottom, the line state becomes `ResolveState(state)`, the EOL is normalised to the view's EOL, the previous non-empty line is given an EOL, empty lines are cleaned, and all steps are grouped as one undo step (`BottomView.cpp:54-96`).

### 8.6 Clipboard block, all views: `CBaseView::AddCutCopyAndPaste`, `BaseView.cpp:4477-4496`
| id | label | shown | disabled when | action |
|---|---|---|---|---|
| sep | — | always | | |
| ID_EDIT_COPY | &Copy | always | no text selection | `OnEditCopy()` |
| ID_EDIT_CUT | C&ut | `IsWritable()` | no text selection | `OnEditCut()` |
| ID_EDIT_PASTE | &Paste | `IsWritable()` | clipboard has neither CF_UNICODETEXT nor CF_TEXT | `OnEditPaste()` |
| sep | — | `IsWritable()` | | |

### 8.7 Whitespace and encoding block, writable views only: `BaseView.cpp:2467-2506`
| id | label | disabled when | action |
|---|---|---|---|
| POPUPCOMMAND_TABTOSPACES | Convert tabs to spaces | no tabs to convert | `ConvertTabToSpaces()` |
| POPUPCOMMAND_SPACESTOTABS | Convert spaces to tabs | no spaces to convert | `Tabularize()` |
| POPUPCOMMAND_REMOVETRAILWHITES | Trim right | no trailing whitespace | `RemoveTrailWhiteChars()` |
| submenu | End of Line Style | never | Items: CRLF, LF, CR, LFCR, VT, FF, NEL, LS, PS (`eolArray[1..]`, `EncodingDlg.cpp:37`; names from `EOL.cpp:24`). The current EOL type is checked. Action: `ReplaceLineEndings(eol)` plus an undo step. |
| submenu | File Encoding | never | Items: ASCII, UTF-16LE, UTF-16BE, UTF-16LE BOM, UTF-16BE BOM, UTF-32LE, UTF-32BE, UTF-8, UTF-8 BOM (`uctArray`, `EncodingDlg.cpp:24`; `FileTextLines.cpp:614-633`). The current type is checked. Action: `SetTextType()`. |

The dispatcher is at `BaseView.cpp:2510-2604`. `ResetUndoStep()` runs before and `SaveUndoStep()` after every command. The prepend, replace and append commands check `!m_pwndLeft->IsReadonly()` again at execution time (:2546-2556).

Notes:
- "Mark as resolved" is **not** in any view context menu. In TortoiseGitMerge it lives on the ribbon and in the save flow.
- One more TortoiseGitMerge menu exists, the patch-file list (`FilePatchesDlg.cpp:304-365`, shown only when `m_sPath` is set). Its items are "Preview patched file" (enabled only when exactly 1 item is selected; this is the default item; it applies the patch, or shows the rejects for an errored file), "Patch all files" (always enabled), and "Patch selected files" (enabled when at least 1 item is selected).

## 9. TortoiseGitBlame

Paths relative to `src/`. Labels from `Resources/TortoiseGitBlameENG.rc` and `Resources/TortoiseLoglistCommon.rc2`.

### 9.1 Blame text view: right-click on a line (`TortoiseGitBlame/TortoiseGitBlameView.cpp:323-452`)
- **Trigger:** `OnRButtonUp`. The menu acts on the single line under the cursor (`GetLineUnderCursor`), not on a text selection. Nothing is shown if the line is invalid or no revision is found for it.
- **Revision lookup:** the revision is taken from the log list (`m_lineToLogIndex`). Otherwise it is fetched by hash, with parents loaded on demand.
- **Parents with file:** the menu builds `parentHashWithFile` (:358-390). It includes each parent for which this commit's file entry matching the line's filename (a) is not a merge-combined entry and (b) is Modified or Replaced. For a Replaced (renamed) entry, the parent filename is the old path. Added files produce no parent, so "Blame previous" and "Compare" are hidden for them.

| # | id | label | shown when | action | src |
|---|---|---|---|---|---|
| 1 | ID_BLAMEPREVIOUS | Blame previous revision | ≥1 parent with the file. With more than 1 such parent, this becomes a submenu with items `Parent %d` (IDS_PARENT, numbered from 1). | `TortoiseGitProc /command:blame /path:<repo root + parent filename> /endrev:<parent sha> /line:<original line number in that commit>` | :393-413, :459-477 |
| 2 | ID_COMPAREWITHPREVIOUS | Compare with previous revision | ≥1 parent with the file. With more than 1, this becomes a submenu with items `Parent %d` (IDS_BLAME_POPUP_PARENT). The default (bold) item is set when the registry value `DiffByDoubleClickInLog` is non-zero (Parent 1 in the submenu case). | `TortoiseGitProc /command:diff /path:<parent filename> /startrev:<parent sha> /endrev:<line commit sha>`, plus ` /alternative` when Shift is held | :416-440, :479-498 |
| 3 | ID_SHOWLOG | Show log | always | `TortoiseGitProc /command:log /path:<file of line> /rev:<sha> /endrev:<sha>` | :442, :500-510 |
| sep | | — | always | | :444 |
| 4 | ID_COPYHASHTOCLIPBOARD | Copy SHA-1 to clipboard | always | Log list `CopySelectionToClipBoard(ID_COPYCLIPBOARDHASH)`, the same format as the log list's "SHA-1" (see §2). It copies the **log list selection**, which follows the clicked line. | :445, :512-513 |
| 5 | ID_COPYLOGTOCLIPBOARD | Copy log to clipboard | always | `CopySelectionToClipBoard(ID_COPYCLIPBOARDFULL)`, the "Full data" format (§2) | :446, :516-517 |

The path for each command comes from `ResolveCommitFile` (:1655-1668): `g_Git.CombinePath(filename)`, or the document path when the filename is empty. "Parent N" numbering, i.e. which parent is used, is encoded in the high 16 bits of the command id.

### 9.2 Blame log pane (`CGitBlameLogList`, `OutputWnd.cpp:100`, `LogListBlameAction.cpp`)
This is the standard log-list context menu (§2) with `m_ContextMenuMask` restricted (`hideFromContextMenu(mask, exclusivelyShow=true)`, `LogListBlameAction.cpp:33-48`; `GitLogListBase.cpp:205`). Only the following bits remain: ID_COMPAREWITHPREVIOUS, ID_GNUDIFF1, ID_BLAMEPREVIOUS, ID_COPYCLIPBOARD, ID_EXPORT, ID_CREATE_BRANCH, ID_CREATE_TAG, ID_SWITCHTOREV, ID_LOG, ID_SHOWBRANCHES, ID_REPOBROWSE. ID_BLAMEPREVIOUS and ID_LOG are first forced on, because the base class clears them (`GitLogListBase.cpp:98-100`). Order and conditions are those of §2. Actions (`LogListBlameAction.cpp:74-170`) act on the first selected row:

| id | label (log list) | action |
|---|---|---|
| ID_BLAMEPREVIOUS (+ "Parent n" submenu) | Blame previous revision | For each blamed filename of that commit (filenames gathered from the blame lines with that hash, else the document path): `/command:blame /path:<file> /endrev:<parent sha>` |
| ID_GNUDIFF1 | Show changes as &unified diff | `/command:diff /path:<file> /startrev:<parent> /endrev:<sha> /unified [/alternative if Shift]` |
| ID_COMPAREWITHPREVIOUS | &Compare with previous revision | Same as above without `/unified` |
| Copy to clipboard submenu | (§2 items) | `CopySelectionToClipBoard(id)`. ID_COPYCLIPBOARDBRANCHTAG is not handled in this switch, so it does nothing here. |
| ID_EXPORT | E&xport this version... | `/command:export /path:<repo root> /rev:<sha>` |
| ID_CREATE_BRANCH | Create Br&anch at this version... | `/command:branch /path:<repo root> /rev:<sha>` |
| ID_CREATE_TAG | Create &Tag at this version... | `/command:tag /path:<repo root> /rev:<sha>` |
| ID_SWITCHTOREV | Sw&itch/Checkout to this... | `/command:switch /path:<repo root> /rev:<sha>` |
| ID_LOG | Show log... | `/command:log /path:<blamed doc path> /endrev:<sha> /rev:<sha>` |
| ID_REPOBROWSE | &Browse repository | `/command:repobrowser /path:<blamed doc path> /rev:<sha>` |
| ID_SHOWBRANCHES | Show branches this commit is &on | `/command:commitisonrefs /path:<repo root> /rev:<sha>` |

### 9.3 Other blame panes
- **Output pane** (`OutputWnd.cpp:194-212`, resource `IDR_OUTPUT_POPUP`, `TortoiseGitBlameENG.rc:295-304`): items `&Copy` (ID_EDIT_COPY), `&Clear` (ID_EDIT_CLEAR), separator, `&Hide` (ID_VIEW_OUTPUTWND; hides the pane).
- **Properties pane** (`PropertiesWnd.cpp:312-332`): shown only when a non-group property is selected. Its single item is IDS_SCIEDIT_COPY (Copy), which copies the property value to the clipboard.

---

## 10. Open points / not determined

- **Browse References confirmation text.** The delete-branch confirmation appends `IDS_PROC_BROWSEREFS_WARNING*` strings. They are named in §4.2 but not quoted; look them up in `Resources/TortoiseProcENG.rc` when implementing.
- **Exact `git` lines behind some dialogs.** `CAppUtils::StashSave`, `GitReset`, `Merge`, `Push`, `Pull`, `Fetch`, `IgnoreFile` and `CreateBranchTag` open dialogs. The `git` lines those dialogs eventually run belong to `spec-dialogs.md`, not to this file.
- **Status list changelist separator.** The condition at `Git/GitStatusListCtrl.cpp:2011` compares the git action bitmask with the legacy SVN constants `git_wc_status_unversioned` / `git_wc_status_none`. Treat it as always true.
- **"Shell" submenu (§1.4 last row).** It embeds the Windows Explorer context menu for the selected files. There is no direct equivalent on macOS or Linux; record the chosen substitute in `docs/divergences.md`.
- **Button drop-downs are out of scope.** These are menus opened from a button, not context menus:
  - LogDlg: Filter, Walk behaviour and View buttons
  - FileDiffDlg: diff-options button
  - SyncDlg: Pull, Fetch and Push split buttons
  - RebaseDlg: split button (summarised in §6)
