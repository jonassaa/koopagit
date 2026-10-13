# TortoiseGit keyboard spec (KoopaGit parity reference)

Upstream: TortoiseGit 2.19.0 (src/version.h), shallow clone at `origin/master` 7338078f8ddd.
All paths are relative to the upstream root. `rc` = `src/Resources/TortoiseProcENG.rc` unless noted.
"(inferred)" marks behaviour not visible as explicit TortoiseGit code: standard Win32/MFC or Scintilla behaviour, or a conclusion drawn from code ordering.

## 0. Cross-cutting mechanics (read first)

| Mechanism | Detail | Source |
|---|---|---|
| Key routing order | MFC calls `PreTranslateMessage` on the focused control first, then on each parent up to the dialog. After that it calls the dialog's `TranslateAccelerator`/`IsDialogMessage` (Tab, Enter=default button, Esc=IDCANCEL, Alt+mnemonic). A control handler that returns TRUE stops the key. This explains the precedence notes below. (inferred: MFC `WalkPreTranslateTree`) | MFC |
| Ctrl+A in single-line EDIT | In every `CResizableStandAloneDialog`/`CStandAloneDialog`: when the focused window's class is `EDIT`, select all text. Does not apply to RichEdit or Scintilla; those have their own Ctrl+A. | src/Utils/MiscUI/StandAloneDlg.h:85-106 |
| Ctrl+Alt+D | In every standalone dialog: toggle dark theme. | src/Utils/MiscUI/StandAloneDlg.h:103-104 |
| F1 | Help topic for the dialog. `ID_HELP` → `CWinAppEx::OnHelp`, routed through the `HtmlHelp` override. The Help button (`IDHELP`) does the same. | src/TortoiseProc/TortoiseProc.cpp:60; StandAloneDlg.h:262-267; StandAloneDlg.cpp:67,210; doc dug_general.xml:89-95 |
| F5 | "Refresh the current view" is a documented convention. It is implemented per dialog (listed below). There is no global F5 handler. | doc dug_general.xml:96-109; rc:3959 (IDS_FIRSTSTART_HINT3: "F5 for refresh or CTRL+F for search") |
| Shift while launching a diff | Every "diff"/"compare"/"unified diff" action (Enter, double-click, context menu, button) checks Shift. Shift = use the *alternative* diff tool (`/alternative`, `bAlternativeTool`). | GitStatusListCtrl.cpp:2955-3164 (many); GitLogListAction.cpp:164; FileDiffDlg.cpp:418-433,630; BrowseRefsDlg.cpp:1275,1282; SubmoduleDiffDlg.cpp:260; ChangedDlg.cpp:445; PatchListCtrl.cpp:82,123; FormatPatchDlg.cpp:284; RevisionGraphDlgFunc.cpp:442,462; doc dug_diff.xml:155-160 |
| Shift while opening a context menu | Shows the "extended" menu (log list: Push/Show log on every commit etc.; status list: `CMF_EXTENDEDVERBS` for the shell submenu). | GitLogListBase.cpp:1662,1954,2207-2211; GitStatusListCtrl.cpp:4921; doc dug_log.xml:178 |
| Dynamic Alt+letter on menu buttons | For the Commit dialog's OK split button and the post-command buttons of both progress dialogs, TortoiseGit builds an accelerator table at runtime. Each entry's label is scanned with `GetAccellerator` (first `&x`, `&&` skipped, uppercased) and gets an **Alt+X** accelerator. Alt+X selects that entry and runs it immediately. If two entries share a letter, Alt+X instead opens the drop-down (posts F4 to the button). | StringUtils.cpp:122-134; CommitDlg.cpp:491-529,1649,2566-2590; GitProgressDlg.cpp:268-300,320-340; ProgressDlg.cpp:~420-440,820-835 |
| `CMenuButton` (all split/menu buttons) | Enter or Space = click (run current entry) when the menu is not open. F4 opens the drop-down menu, or clicks when the button is configured `m_bAlwaysShowArrow`. | src/Utils/MiscUI/MenuButton.cpp:136-158 |
| `CLinkControl` (hyperlink-style statics such as Commit's "All/None/…" and "View Patch>>") | Space or Enter activates the link. The control reports `DLGC_BUTTON` and handles `BM_CLICK`, so an `&` mnemonic on it triggers a click rather than a focus move. Tab is not captured. | src/Utils/MiscUI/LinkControl.cpp:165-228 |
| `CHistoryCombo` (all history/URL/branch combos) | Enter with the drop-down open: close the drop-down only (does not press the default button). Enter with it closed: passes to the dialog unless `m_bWantReturn` is set (only Sync's Remote Branch and URL combos set it, so there Enter is swallowed). Shift+Delete with the drop-down open: remove the highlighted history item (if deletion is allowed). Ctrl+A: select all text in the edit part. The mouse wheel does nothing unless the drop-down is open. | HistoryCombo.cpp:56-105; HistoryCombo.h:48,176; SyncDlg.cpp:1248-1249 |
| `CACEdit` autocomplete edits (FileDiff rev edits, CommitIsOnRefs rev edit, SendMail To/CC) | With the suggestion list open: Up/Down/PgUp/PgDn/Home/End move in the list, Enter accepts, Esc closes the list. On an empty edit with `_MODE_CURSOR_O_LIST_`, Down opens the full list. | src/Utils/ACEdit.cpp:186-330,492-510; ACListWnd.cpp:335-341,785-810 |
| `CFilterEdit` (search boxes with the clear "x" icon) | No key handling of its own. Esc-clears-filter is implemented per dialog (below). Pasting multi-line text converts it. | src/Utils/MiscUI/FilterEdit.cpp:44-79,438 |
| `CMessageBox` (all TortoiseGit message boxes) | Ctrl+C or Ctrl+Insert copies the message text. For standard `MB_*` types, Esc returns the "negative" button: ABORTRETRYIGNORE→btn1, CANCELTRYCONTINUE→btn1, OKCANCEL→btn2, RETRYCANCEL→btn2, YESNO→btn2, YESNOCANCEL→btn3. Custom-button boxes use their own captions, e.g. "&Remove"/"&Abort", "&Yes"/"&No", "&Retry", "&Ignore", "&Skip", "&Stash", "&Don't show this message again". | MessageBox.cpp:1140-1184; rc:3511-3520,4777,4844,4948,5100 |
| No Ctrl+W | TortoiseGitProc dialogs do **not** bind Ctrl+W. Only TortoiseGitUDiff and TortoiseGitMerge do. | absence in rc accelerator tables |

## 1. Shared list controls

### 1.1 File status list (`CGitStatusListCtrl`)
Used in Commit, Log (changed-files pane `IDC_LOGMSG`), Add, Revert, Resolve, Check for modifications (ChangedDlg), Rebase (files tab), Sync (in/out/conflict lists) and LFS Locks.
The control returns `DLGC_WANTALLKEYS`, so it receives Enter/Tab/Esc itself.

| Key | Action | Condition | Source |
|---|---|---|---|
| Enter | For each selected item: unversioned or ignored → diff against the working copy; unmerged → conflict editor (Shift = alternative tool); missing → skipped; otherwise → diff (two-revision diff when both revisions are set). | list focused | GitStatusListCtrl.cpp:220,3611-3665 |
| Double-click | Same as Enter for one item. A click on the checkbox area is ignored. | `m_bEnableDblClick` | GitStatusListCtrl.cpp:2926-2990 |
| Space | Toggles the focused item's checkbox (native `LVS_EX_CHECKBOXES`, inferred). TortoiseGit then applies the same state to **all selected items** when the toggled item is selected. A three-state checkbox (staging mode) goes checked→unchecked, never to indeterminate. | checkboxes shown | GitStatusListCtrl.cpp:1255-1331 |
| Ctrl+A | Select all items. | | GitStatusListCtrl.cpp:3838-3848 |
| Ctrl+C / Ctrl+Insert | Copy the selected entries' relative paths (filename column), one per line. | | GitStatusListCtrl.cpp:3849-3861 |
| Ctrl+Shift+C / Ctrl+Shift+Insert | Copy relative path + status columns. | | GitStatusListCtrl.cpp:3855-3858 |
| Delete | Delete the selected files (Explorer delete, to the Recycle Bin). Only fires if the list allows `GITSLC_POPDELETE` **and** the selection-mark entry is unversioned or ignored. | | GitStatusListCtrl.cpp:3863-3872 |
| Shift+Delete | As Delete but permanent (no `FOF_ALLOWUNDO`). The same Shift rule applies to the context-menu "Delete". | | GitStatusListCtrl.cpp:4756; doc dug_wcstatus.xml:260-263 |
| Tab / Shift+Tab | Moves to the next/previous dialog control (re-posted as `WM_NEXTDLGCTL`, because the control eats all keys). | | GitStatusListCtrl.cpp:3667-3681 |
| Esc | Sends `WM_CLOSE` to the owning dialog. | | GitStatusListCtrl.cpp:3682-3686 |
| Shift+right-click / Shift+menu key | Extended shell verbs in the context menu. | | GitStatusListCtrl.cpp:1641,4921 |
| Arrows, Home/End, PgUp/PgDn, Shift/Ctrl selection, type-ahead | Native list-view behaviour (inferred). | | — |

### 1.2 Commit log list (`CGitLogListBase` / `CGitLogList`)
Used in Log, Rebase/Cherry-pick, Sync (in/out logs), Blame's log pane and Format Patch, among others.

| Key | Action | Condition | Source |
|---|---|---|---|
| Enter (no Ctrl) | If registry `DiffByDoubleClickInLog` = TRUE: diff the selected revision with its parent. Default is FALSE, and then Enter does nothing in the list (the Log dialog's `OnOK` ignores Enter unless the OK button has focus). | | GitLogListBase.cpp:2719-2737; LogDlg.cpp:1593-1598 |
| Double-click | Same rule (diff only if `DiffByDoubleClickInLog`). | | GitLogListBase.cpp:2760-2767 |
| Ctrl+A | Select all rows. | | GitLogListBase.cpp:2739-2745 |
| Shift + context menu | Extended menu (e.g. Push / Show log for every commit). | | GitLogListBase.cpp:1662; doc dug_log.xml:178 |
| Shift + diff/compare menu command | Alternative diff tool, or alternative unified-diff viewer. | | GitLogListAction.cpp:164,254-411 |
| Shift + choosing a ref / "Find" in the Find dialog | Scroll to and highlight the commit **without** selecting it. | | GitLogListBase.cpp:3395-3430,3452-3530; doc dug_log.xml:397 |

### 1.3 Progress list (`CGitProgressList`, inside `CGitProgressDlg`)
| Key | Action | Source |
|---|---|---|
| Ctrl+A | Select all lines. | GitProgressList.cpp:1027-1036 |
| Ctrl+C / Ctrl+Insert | Copy the selected lines as `"<action>: <path>  <mime>\r\n"`. Needs a selection mark. | GitProgressList.cpp:1037-1060 |

## 2. Commit message editor (`CSciEdit`, Scintilla)
Used in Commit, Rebase (message + output), Merge message, New Branch/Tag message, Stash message, Format/Send mail, the patch view and others.

| Key | Action | Condition | Source |
|---|---|---|---|
| Ctrl+Space | Show autocompletion. Minimum prefix is 1 here, versus the automatic popup at `AutoCompleteMinChars` (default 3). Works only at the end of a word. | no Alt | SciEdit.cpp:1041-1048,787-800,245; doc dug_commit.xml:235-241 |
| Tab (autocomplete open) | Accept the completion. Fill-up char is `ScintillaACFillups`, default `"\t"`. Enter also accepts (Scintilla default, inferred). | | SciEdit.cpp:224 |
| Tab / Shift+Tab | Move focus to the next/previous dialog control. Tab is **not** inserted. | autocomplete not active | SciEdit.cpp:1050-1066 |
| Ctrl+Tab | Spelling suggestions for the word under the caret. | no Alt | SciEdit.cpp:1053-1060 |
| Esc | Sends `WM_CLOSE` to the parent dialog, **unless** an autocomplete list or calltip is open (then Scintilla closes that). | | SciEdit.cpp:1021-1033 |
| Home / Shift+Home | `SCI_HOMEWRAP(EXTEND)`: wrap-aware home. | | SciEdit.cpp:325-326 |
| End / Shift+End | `SCI_LINEENDWRAP(EXTEND)`: wrap-aware end. | | SciEdit.cpp:323-324 |
| All other editing keys (Ctrl+Z/Y, Ctrl+X/C/V, Ctrl+A, Ctrl+wheel zoom, Ctrl+Backspace/Del word delete, Alt+drag rectangular select, etc.) | Scintilla defaults (inferred). | | — |

## 3. TortoiseGitProc dialogs

### 3.1 Log dialog (`CLogDlg`, `IDD_LOGMESSAGE`)
Accelerator table `IDR_ACC_LOGDLG` (rc:3463-3484), loaded in LogDlg.cpp:285 and translated in `PreTranslateMessage` (LogDlg.cpp:1834-1839). Command map at LogDlg.cpp:148-171.

| Key | Action | Condition | Source |
|---|---|---|---|
| Ctrl+C, Ctrl+Insert | `ID_EDIT_COPY`. With the changed-files list focused: copy its selected paths. Otherwise copy the selected commits from the log list in full format (revision, author, date, message, notes/tags, changed paths). | Skipped when focus is in the message view, the search edit or the file filter (normal text copy there). With the changed-files list focused, the list's own Ctrl+C (§1.1) runs first (inferred from routing). | rc:3465-3466; LogDlg.cpp:1779,3085-3091; GitLogListBase.cpp:2380-2440 |
| Ctrl+C / Ctrl+Insert in the message view (`IDC_MSGVIEW`) | Copy the selected text as plain text, done manually so it is detached from the process (issue #3909). | Ctrl+Insert only without Shift | LogDlg.cpp:1827-1833 |
| Ctrl+F, F3 | `ID_LOGDLG_FIND`: open the modeless **Find** dialog (`IDD_FIND`, in TortoiseLoglistCommon.rc2:190), or focus it if already open. | | rc:3467-3468; LogDlg.h:90-93; GitLogListBase.cpp:3359-3371 |
| Alt+D, Ctrl+L | `ID_LOGDLG_FOCUSFILTER`: focus the top search/filter edit (`IDC_SEARCHEDIT`). | | rc:3469-3470; LogDlg.cpp:3080-3083 |
| Ctrl+E | `ID_SELECT_SEARCHFIELD`: focus the search edit and select all its text. | | rc:3482; LogDlg.cpp:1177-1181 |
| F5, Ctrl+F5, Shift+F5 | `ID_LOGDLG_REFRESH` (same as the Refresh button). All three behave identically. | | rc:3471-3473; LogDlg.cpp:163 |
| Alt+Left, Browser Back key | `ID_GO_BACKWARD_SELECT`: go back in the selection history and select that commit. | | rc:3475,3477; LogDlg.cpp:1193-1240 |
| Alt+Right, Browser Forward key | `ID_GO_FORWARD_SELECT`: go forward and select. | | rc:3474,3476 |
| Shift+Alt+Left / Shift+Alt+Right | `ID_GO_BACKWARD` / `ID_GO_FORWARD`: navigate the history but only scroll to and highlight the commit, without selecting it. | | rc:3478-3479; LogDlg.cpp:1183-1191; doc dug_log.xml:872-893 |
| Mouse X1/X2 (Back/Forward) | Same as Alt+Left/Right. With Shift held: highlight without selecting. | | LogDlg.cpp:1817-1826 |
| Alt+Up / Alt+Down | `ID_GO_UP` / `ID_GO_DOWN`: the green jump up/down buttons, i.e. jump to the previous/next commit matching the jump-type combo (author email, committer email, parent 1/2, tag FF, branch FF, or history). | | rc:3480-3481; LogDlg.cpp:148-149,2565-2700; doc dug_log.xml:860-869 |
| Esc | `ID_EXITCLEARFILTER`: if a log filter is active, clear it (same as the filter's "x"); otherwise close the dialog. | any focus, but see the next row | rc:3483; LogDlg.cpp:3590-3598 |
| Esc in the bottom file filter (`IDC_FILTER`) | If it has text: clear it and refill the changed-files list. This runs before the accelerator, so Esc #1 clears the file filter, Esc #2 clears the log filter, Esc #3 closes. | text non-empty | LogDlg.cpp:1810-1816 |
| Enter in the search edit (`IDC_SEARCHEDIT`) | Apply the filter immediately (cancels the typing-delay timer). | | LogDlg.cpp:1789-1793 |
| Enter in the file filter (`IDC_FILTER`) | Apply the file filter immediately. | | LogDlg.cpp:1794-1799 |
| Enter in the log list | Diff with the previous revision **only** if `DiffByDoubleClickInLog`. | | LogDlg.cpp:1780-1788 |
| Enter elsewhere | Does **not** close the dialog: `OnOK` returns unless the OK button itself has focus. | | LogDlg.cpp:1593-1598 |
| Ctrl+V, Shift+Insert | Paste a commit hash from the clipboard and jump to that commit if the text looks like a hash. | Not in the search edit or file filter; Alt not held | LogDlg.cpp:1802-1809,1707-1730; doc dug_log.xml:894-899 |
| Ctrl+A | Log list: select all (§1.2). Changed-files list: select all (§1.1). | | |

Find dialog (`IDD_FIND`): "&Find" (default, Alt+F), "Match &case" (Alt+C), "&Regular Expression" (Alt+R), Cancel. Clicking a ref, or "Find", with Shift held highlights the commit without selecting it.

### 3.2 Commit dialog (`CCommitDlg`, `IDD_COMMITDLG`)
Accelerator table `IDR_ACC_COMMITDLG` (rc:3486-3490), loaded at CommitDlg.cpp:228 and checked first in `PreTranslateMessage` (CommitDlg.cpp:1641-1684).

| Key | Action | Condition | Source |
|---|---|---|---|
| Alt+M | `ID_FOCUS_MESSAGE`: focus the message editor. This coincides with the "&Message:" group-box mnemonic. | | rc:3488; CommitDlg.cpp:138,2826-2829 |
| Alt+F | `ID_FOCUS_FILELIST`: focus the changed-files list. | | rc:3489; CommitDlg.cpp:139,2831-2834 |
| Alt+O / Alt+R / Alt+P | Dynamic OK-button accelerators built from "C&ommit", "&ReCommit" and "Commit && &Push". Each selects that entry of the split OK button and commits. | Only when OK is enabled and post-actions are allowed (`!m_bNoPostActions`) | CommitDlg.cpp:491-529,1649-1650,2566-2590; rc:5130-5132 |
| Ctrl+Enter | Commit (post `IDOK`) if OK is enabled. Always swallowed. | any focus, including the message editor | CommitDlg.cpp:1663-1670; doc dug_commit.xml:305-309 |
| Enter in the Bug-ID edit | Move focus to the message editor. | | CommitDlg.cpp:1671-1677 |
| F5 | Refresh (re-scan the working tree). | ignored while `m_bBlock` | CommitDlg.cpp:1656-1662; doc dug_general.xml:96-109 |
| Esc | Cancel. Asks "really cancel?" (Yes/No, default No, "don't ask again" option) when the message was changed or the list is non-empty. From the message editor or the file list, Esc arrives as `WM_CLOSE` (§1.1, §2). | | CommitDlg.cpp:1567-1600 |
| Alt+A / Alt+N | Check all / check none, via the "&All" and "&None" link labels. | | rc:269-270; LinkControl.cpp:210-217; CommitDlg.cpp:2504-2512 |
| Shift + activating a check link (All/None/Unversioned/Versioned/Added/Deleted/Modified/Files/Submodules) | Inverts the action (uncheck instead of check). | | CommitDlg.cpp:2504-2520 |
| Message editor keys | See §2. | | |
| File list keys | See §1.1. | | |
| "View Patch>>" patch window | When the separate patch window is active: §3.20. | | |

### 3.3 Rebase / Cherry-pick dialog (`CRebaseDlg`, `IDD_REBASE`)
| Key | Action | Condition | Source |
|---|---|---|---|
| Space | Cycle the action of every selected commit: pick → skip → edit → squash → pick. A squash on the last row of a non-merge commit becomes pick. | commit list focused and all four action menu items available (i.e. before rebasing starts) | RebaseDlg.cpp:811-821; GitLogListAction.cpp:1343-1371; doc dug_rebase.xml:28-33 |
| P | Set selected to **pick**. | list focused and action available | RebaseDlg.cpp:822-828 |
| S | Set selected to **skip**. | same | RebaseDlg.cpp:829-835 |
| Q | Set selected to **squash**. | same | RebaseDlg.cpp:836-842 |
| E | Set selected to **edit**. | same | RebaseDlg.cpp:843-849 |
| U | Move the selected commits up (Up button). **Shift+U** moves them to the top. | list focused and Up enabled | RebaseDlg.cpp:850-856,2758-2795 |
| D | Move down. **Shift+D** moves to the bottom. | list focused and Down enabled | RebaseDlg.cpp:857-863,2796-2830 |
| Shift + click Up/Down buttons | Move to top/bottom. | | RebaseDlg.cpp:2763,2801 |
| Ctrl+A | Select all commits. | list focused | RebaseDlg.cpp:864-872 |
| F5 | Refresh. | | RebaseDlg.cpp:873-878 |
| Ctrl+Enter | Press Continue if it is enabled; otherwise focus Abort; otherwise focus Help. Always swallowed. | | RebaseDlg.cpp:879-892 |
| Esc / Cancel key from a RichEdit, Scintilla, list view or tab-page child | Re-posted to the dialog itself, so it acts as dialog Esc and doesn't destroy the tab control. | | RebaseDlg.cpp:893-911 |
| Tab from the log-message editor or the output view | Focus jumps to Continue, else Abort, else Help. | `WM_NEXTDLGCTL` | RebaseDlg.cpp:914-927 |
| List keys | §1.2 for the commit list; §1.1 for the files tab. | | |

Runtime Continue-button captions carry no mnemonics: "Start Rebase", "Start (FastFwd)", "Continue", "Commit", "Amend", "Finish", "Done" (rc:4773,4806-4815; RebaseDlg.cpp:1800-1835).
Split-all menu button entries: "&Pick ALL", "&Squash ALL", "&Edit ALL", "S&kip unselected", "S&quash unselected", "E&dit unselected" (rc:3929-3933,4456-4458; RebaseDlg.cpp:227-241). Only the visible current entry acts as a mnemonic (inferred).

### 3.4 Sync dialog (`CSyncDlg`, `IDD_SYNC`)
| Key | Action | Condition | Source |
|---|---|---|---|
| F5 | Refresh. | not while `m_bBlock` (an operation is running) | SyncDlg.cpp:1330-1336 |
| Esc / Cancel key from a RichEdit or list-view child | Re-posted to the dialog (closes it). | | SyncDlg.cpp:1339-1353 |
| Enter in the Remote Branch or Remote URL combos | Swallowed (`m_bWantReturn`), so it doesn't trigger the default button. | | SyncDlg.cpp:1248-1249; HistoryCombo.cpp:63-72 |
| Shift + Pull button (entry Pull or Fetch) | Open the standalone Pull/Fetch dialog instead of running directly. | | SyncDlg.cpp:139-145,264-270; doc dug_sync.xml:54-58 |
| Shift + Push button (entry Push) | Open the standalone Push dialog. | | SyncDlg.cpp:713-726 |
| Shift + Submodule button | Open the Submodule Update dialog (Update/Init) or Submodule Sync. | | SyncDlg.cpp:1739-1757 |
| Shift + Stash button (entry Stash changes) | Open the Stash Save dialog. | | SyncDlg.cpp:1797-1805 |
| Enter / Space / F4 on the menu buttons | §0 `CMenuButton`. | | |

### 3.5 Progress dialogs
Both dialogs disable Esc while running (Cancel/Abort is the only way out). When finished, Esc is converted to Enter on the focused Close/OK button.

| Dialog | Key | Action | Source |
|---|---|---|---|
| `CProgressDlg` (`IDD_GITPROGRESS`, "Git Command Progress": git CLI output) | Esc after finishing | When Abort is disabled and Close is enabled and visible: focus Close and convert Esc to Enter, which closes the dialog. | ProgressDlg.cpp:751-773 |
| | Esc / Abort while running | `OnCancel`: optionally confirm (`ConfirmKillProcess`), send Ctrl+C to git, then kill the process tree. | ProgressDlg.cpp:665-695 |
| | Alt+letter | Dynamic accelerators for the post-command button entries (§0). Labels come from the shell string table, e.g. "&Pull..." (Alt+P), "Pus&h..." (Alt+H), "&Commit..." (Alt+C), "&Retry" (Alt+R), "&Merge unrelated history" (Alt+M). | ProgressDlg.cpp:420-440,820-835; AppUtils.cpp:140-154,1320-1365,2326-2399; src/TortoiseShell/resourceshell.rc:208,419-420; rc:3513,3950 |
| `CGitProgressDlg` (`IDD_SVNPROGRESS`, "Progress": libgit2 operations) | Esc after finishing | Same Esc→Enter conversion, also gated on `!m_ProgList.IsRunning()`. | GitProgressDlg.cpp:194-216 |
| | Alt+letter | Dynamic accelerators for the "Show &log..." menu-button entries. | GitProgressDlg.cpp:268-300,320-340 |
| | List | §1.3. | |
| `CProgressDlg` log (RichEdit) | Ctrl+A, Ctrl+C | Native RichEdit (inferred). The context menu adds "Copy" and "Copy all". | ProgressDlg.cpp:775-815 |

### 3.6 Simpler dialogs with explicit keys

| Dialog | Key | Action | Condition | Source |
|---|---|---|---|---|
| Add (`CAddDlg`) | Ctrl+Enter | OK if enabled (swallowed). | | AddDlg.cpp:159-184 |
| | F5 | Refresh. | | |
| Revert (`CRevertDlg`) | Ctrl+Enter | OK if enabled. | | RevertDlg.cpp:205-237 |
| | F5 | Refresh (restart the status thread). | | |
| Resolve (`CResolveDlg`) | Ctrl+Enter | OK if enabled. | | ResolveDlg.cpp:151-183 |
| | F5 | Refresh. | | |
| Input dialog (`CInputDlg`, e.g. branch description, notes) | Ctrl+Enter | OK if enabled. **Not** swallowed, so the key continues to the dialog. | | InputDlg.cpp:107-126 |
| Check for Modifications (`CChangedDlg`) | F5 | Refresh. | | ChangedDlg.cpp:330-343 |
| Push (`CPushDlg`) | F5 | Refresh branches and remotes. | | PushDlg.cpp:577-592 |
| Select Remote Ref (`CSelectRemoteRefDlg`) | F5 | Refresh. | | SelectRemoteRefDlg.cpp:111-126 |
| Delete Remote Tags (`CDeleteRemoteTagDlg`) | F5 | Refresh. | | DeleteRemoteTagDlg.cpp:179-194 |
| LFS Locks (`CLFSLocksDlg`) | F5 | Reload locks. | not while loading | LFSLocksDlg.cpp:135-156 |
| Worktree list (`CWorktreeListDlg`) | F5 | Refresh. | | WorktreeListDlg.cpp:427-440 |
| Submodule Diff (`CSubmoduleDiffDlg`) | F5 | Refresh by closing and re-opening the dialog (`m_bRefresh`, `EndDialog(0)`). | | SubmoduleDiffDlg.cpp:207-216 |
| Recent messages (`CHistoryDlg`) | Delete | Remove the selected history entry and save. | | HistoryDlg.cpp:109-125; doc dug_commit.xml:286-291 |
| Apply Patches (`CImportPatchDlg`) | Esc / Cancel key from a RichEdit, Scintilla, list view or tab-page child | Re-posted to the dialog. | | ImportPatchDlg.cpp:663-690 |
| RefLog (`CRefLogDlg`) | F5 | Refresh. | | RefLogDlg.cpp:179-187 |
| | Ctrl+F, F3 | Open or focus the find dialog (`CFindReplaceDialog`, down-only, no whole-word). Not swallowed. | | RefLogDlg.cpp:183-184,227-237 |
| Commit-is-on-refs (`CCommitIsOnRefsDlg`) | Ctrl+A / Ctrl+C / Ctrl+Insert | Select all / copy the selected refs. | ref list focused | CommitIsOnRefsDlg.cpp:523-571 |
| | F5 | Reload refs. | | |
| | Esc in the filter | Clear the filter if non-empty (else the dialog closes). | | |
| Settings (`CSettings`: `CTreePropSheet`) | Ctrl+Tab / Ctrl+Shift+Tab | Next / previous page. | | TreePropSheet.cpp:1003-1013 |
| | Ctrl+PgDn / Ctrl+PgUp | Next / previous page. | | TreePropSheet.cpp:1015-1023 |
| Settings → Advanced page | F2 | Edit the value of the selected entry (label edit). | | Settings/SettingsAdvanced.cpp:166-180 |

Dialogs with **no** custom key handling (standard Win32 only, §5): Clone, Pull/Fetch, Switch/Checkout, Create Branch/Tag, Merge, Stash Save, Reset, Clean, Format Patch, Submodule Update, Submodule Add, Export, Bisect start, Merge Abort, Rename, Create Repo, Request Pull, Send Mail.

### 3.7 Browse References (`CBrowseRefsDlg`)
| Key | Action | Condition | Source |
|---|---|---|---|
| F2 | Rename the selected ref (in-place label edit). | ref list focused | BrowseRefsDlg.cpp:1381-1390 |
| F5 | Refresh. | | BrowseRefsDlg.cpp:1392-1396 |
| Ctrl+E | Focus the filter edit and select all. | | BrowseRefsDlg.cpp:1397-1406 |
| Ctrl+A | Select all refs. | ref list focused | BrowseRefsDlg.cpp:1407-1415 |
| Esc in the filter | Clear the filter if non-empty, otherwise close. | | BrowseRefsDlg.cpp:1416-1422 |
| Shift + Unified diff / Compare with working tree | Alternative tool. | | BrowseRefsDlg.cpp:1275,1282 |

(A Ctrl+Enter→OK handler exists but is commented out: BrowseRefsDlg.cpp:1369-1380.)

### 3.8 Repository Browser (`CRepositoryBrowser`)
| Key | Action | Condition | Source |
|---|---|---|---|
| F5 | Refresh. | | RepositoryBrowser.cpp:917-927; doc dug_repobrowser.xml:92 |
| Ctrl+A | Select all files. | file list focused | RepositoryBrowser.cpp:928-936 |
| Enter | Folder → enter it (and select it in the tree). File → open it. The dialog does not close. | file list focused, exactly one item selected, Alt not held | RepositoryBrowser.cpp:278-299 |
| Enter elsewhere | Close (OK). | | RepositoryBrowser.cpp:301-302 |

### 3.9 Changed Files / file diff list (`CFileDiffDlg`)
| Key | Action | Condition | Source |
|---|---|---|---|
| Ctrl+A | Select all. | file list focused | FileDiffDlg.cpp:879-891 |
| Ctrl+C / Ctrl+Insert | Copy the selection. | file list focused | FileDiffDlg.cpp:892-903 |
| Enter | Diff the selection-mark file (blame diff in blame mode), as on double-click. | file list focused | FileDiffDlg.cpp:904-915 |
| F5 | Refresh. | | FileDiffDlg.cpp:916-920 |
| Esc in the filter | Clear the filter if non-empty. | | FileDiffDlg.cpp:921-928 |
| Enter elsewhere | Does nothing: `OnOK` is empty (issue #4049). | | FileDiffDlg.cpp:941-944 |
| Shift + diff | Alternative tool. | | FileDiffDlg.cpp:418-433 |

### 3.10 Revision Graph (`CRevisionGraphDlg`)
| Key | Action | Source |
|---|---|---|
| Up / Down / Left / Right | Scroll by 20 px. | RevisionGraphDlg.cpp:385-404 |
| PgUp / PgDn | Scroll vertically by half the graph height. | RevisionGraphDlg.cpp:405-414 |
| F5 | Update the full history. | RevisionGraphDlg.cpp:415-417; doc dug_revgraph.xml:352,477 |
| Ctrl+F | Find. The `F` key is swallowed even without Ctrl. | RevisionGraphDlg.cpp:418-421 |
| Ctrl+wheel | Zoom. | RevisionGraphWnd.cpp:963-967 |
| Shift+wheel | Horizontal scroll (Shift+H-wheel scrolls vertically). | RevisionGraphWnd.cpp:970-990 |
| Ctrl+click | Add a node to the selection (two-node compare). | RevisionGraphWnd.cpp:453; doc dug_revgraph.xml:413 |
| Shift + compare | Alternative diff tool. | RevisionGraphDlgFunc.cpp:442,462 |

The menu shows "Zoom &in\tCtrl-+" and "Zoom &out\tCtrl--" (rc:3408-3409), but the accelerator table load is commented out (RevisionGraphDlg.cpp:261). These hints are therefore **dead**, and so is the Esc-cancels-mouse-zoom branch (RevisionGraphDlg.cpp:424-430).

### 3.11 Patch view window (`CPatchViewDlg`: "View Patch" next to Commit and Log)
Accelerator table `IDR_ACC_PATCHVIEW` (rc:3492-3500), loaded at PatchViewDlg.cpp:96.

| Key | Action | Source |
|---|---|---|
| Ctrl+F | Show the find bar, prefilled with the selection. | rc:3498; PatchViewDlg.cpp:300-316 |
| F3, Alt+N | Find next. | rc:3494-3495 |
| Shift+F3, Alt+P | Find previous. | rc:3496-3497 |
| Esc | Hide the find bar if visible, otherwise close the patch window. | rc:3499; PatchViewDlg.cpp:290-298,363-374 |
| Find bar: Enter | Nothing (`OnOK` is empty). Typing searches incrementally (inferred, mirrors UDiff). | FindBar.cpp:65-68 |
| Find bar: Esc | Hide the bar. | FindBar.cpp:70-74 |

Find bar captions (`IDD_FINDBAR`, rc:2186-2196): "< &Previous", "&Next >", "Match &case".

## 4. Separate viewers

### 4.1 TortoiseGitBlame (`src/Resources/TortoiseGitBlameENG.rc`)
Accelerator `IDR_TORTOISE_GIT_BLAME_MAINFRAME` (rc:312-331):

| Key | Action | Source |
|---|---|---|
| Esc, Ctrl+Q | Exit. | rc:314,330 |
| Ctrl+O | Open file. | rc:315,131 |
| Ctrl+C, Ctrl+Insert | Copy. With the log list focused: copy the selected commits. In the properties grid: copy the property value. Otherwise copy the text selection. | rc:317,322; TortoiseGitBlameView.cpp:62,733-750 |
| Ctrl+F | Find dialog (`CFindReplaceDialog`), prefilled with the selection. | rc:319,144; TortoiseGitBlameView.cpp:1925-1950 |
| F3 / Shift+F3 | Find next / previous (no-op if there is no search text). | rc:328-329; TortoiseGitBlameView.cpp:664-676 |
| Ctrl+G | Go to line (dialog "Go to line": Line:, OK, Cancel). | rc:320,145,352-361; TortoiseGitBlameView.cpp:1958-1965 |
| F6 / Shift+F6 | Next / previous pane (MFC). | rc:324-325 |
| F1 / Shift+F1 | Help / context help. | rc:326-327; MainFrm.cpp:47-51 |
| Ctrl+X, Ctrl+V, Shift+Delete, Shift+Insert | Bound to cut/paste IDs, but the read-only view has no handlers, so they do nothing (inferred). | rc:316,318,321,323 |
| Shift + "Compare with parent" etc. | Alternative diff tool. | TortoiseGitBlameView.cpp:493; LogListBlameAction.cpp:124 |

Menu mnemonics: &File (&Open...\tCtrl+O, &Settings..., E&xit), &Edit (&Copy\tCtrl+C, &Find\tCtrl+F, &Go to\tCtrl+G), &View, &Help (rc:127-305). `ID_VIEW_NEXT`/`ID_VIEW_PREV` (jump to the next/previous block of the selected commit) are toolbar/ribbon-only, with no key (TortoiseGitBlameView.cpp:64-65,1996-2009).

### 4.2 TortoiseGitUDiff (`src/TortoiseUDiff/TortoiseUDiff.rc`)
Accelerator `IDC_TORTOISEUDIFF` (rc:60-71), loaded at TortoiseUDiff.cpp:134-139:

| Key | Action | Source |
|---|---|---|
| Ctrl+W | Exit (asks to save if modified). | rc:62; MainWindow.cpp:239-243 |
| Ctrl+O | Open. | rc:63 |
| Ctrl+S / Ctrl+Shift+S | Save / Save as. | rc:64-65 |
| Ctrl+P | Print. | rc:70 |
| Ctrl+F | Show the find bar, prefilled with the selection and focused. | rc:69; MainWindow.cpp:244-268 |
| F3 / Shift+F3 | Find next / previous. Opens the find bar if there is no search text. | rc:67-68; MainWindow.cpp:269-284 |
| Esc | Hide the find bar if visible, else exit (asks to save if modified). | rc:66; MainWindow.cpp:285-299 |
| Find bar | Typing searches incrementally (`EN_CHANGE` → find); "< &Previous", "&Next >", "Match &case". | FindBar.cpp:58-80; rc:106-116 |

Menu: &File: "&Open...\tCtrl+O", "&Save\tCtrl+S", "Save &as...\tCtrl+Shift+S", "P&age setup...", "&Print...\tCtrl+P", "&Dark Mode", "Se&ttings...", "&Find\tCtrl+F", "Apply &Patch...", "&Exit\tCtrl+W" (rc:79-97).

### 4.3 TortoiseGitMerge (`src/Resources/TortoiseMergeENG.rc`)
Accelerator `IDR_MAINFRAME` (rc:600-668), documented in doc/source/en/TortoiseMerge/tme_app_shortcuts.xml.

| Key | Action | Source |
|---|---|---|
| Esc, Ctrl+Q, Ctrl+W | Exit (asks to save). | rc:602-604 |
| Ctrl+O | Open. | rc:642 |
| Ctrl+S / Ctrl+Shift+S | Save / Save as. | rc:645-646 |
| Ctrl+R, F5 | Reload, discarding edits. | rc:643-644 |
| Ctrl+Z, Alt+Backspace | Undo. | rc:633-634 |
| Ctrl+Y | Redo. | rc:631 |
| Ctrl+C / Ctrl+Insert | Copy. | rc:618-619 |
| Ctrl+X / Shift+Delete | Cut. | rc:620-621 |
| Ctrl+V / Shift+Insert | Paste. | rc:628-630 |
| Ctrl+A | Select all. | rc:632 |
| Ctrl+F | Find/replace dialog. | rc:622 |
| F3 / Shift+F3 | Find next / previous. | rc:623,625 |
| Ctrl+F3 / Ctrl+Shift+F3 | Find next / previous occurrence of the **selected text** (case-insensitive). With no selection, clears the search. | rc:624,626; BaseView.cpp:5548-5568 |
| Ctrl+G | Go to line. | rc:627 |
| F7, F11, Ctrl+Down, Alt+Down | Next difference. | rc:648-651 |
| Shift+F7, Shift+F11, Ctrl+F11, Ctrl+Up, Alt+Up | Previous difference. | rc:655-659 |
| F8 / Shift+F8 | Next / previous conflict. | rc:647,654 |
| Ctrl+Alt+Right / Ctrl+Alt+Left | Next / previous inline difference. | rc:652-653 |
| F12, Alt+Right | Use left block (diff mode). | rc:635-636 |
| Shift+F12, Ctrl+F10 | Use "my" (right) block. | rc:638-639 |
| Ctrl+F9 | Use "their" (left) block (conflict mode). | rc:640 |
| Ctrl+Shift+F9 | Use theirs, then mine. | rc:641 |
| Ctrl+Shift+F10 | Use mine, then theirs. | rc:637 |
| Ctrl+M | Toggle the mark on the selected change (right view). | BaseView.cpp:3195-3202 |
| Ctrl+E | Toggle editing (read-only ↔ writable) for the active view. | rc:667; MainFrm.cpp:2482-2490 |
| Ctrl+D | One-pane ↔ two-pane diff. | rc:663 |
| Ctrl+U | Switch left/right files. | rc:664; MainFrm.cpp:2419-2430 |
| Ctrl+L | Collapse unchanged sections. | rc:662 |
| Ctrl+T | Show whitespace. | rc:665 |
| Ctrl+P | Wrap long lines. | rc:666 |
| F6 / Shift+F6 | Next / previous pane. | rc:660-661 |
| Ctrl+Tab | Cycle focus left → right → bottom. | BaseView.cpp:3017-3054 |
| Arrows (±Shift), Ctrl+Left/Right (±Shift) | Caret and word movement with selection, via accelerators. | rc:605-616 |
| Home | Smart home: first non-blank, then column 0. Ctrl+Home goes to the document start. ±Shift extends. | BaseView.cpp:3078-3109 |
| End | End of line (wrap-aware). Ctrl+End goes to the document end. ±Shift extends. | BaseView.cpp:3110-3136 |
| PgUp / PgDn | Page with caret (±Shift). | BaseView.cpp:3055-3077 |
| Backspace / Delete | Delete a char, or a word with Ctrl. Writable views only. | BaseView.cpp:3137-3184 |
| Insert | Toggle insert/overwrite. | BaseView.cpp:3185-3188 |
| Wheel / Ctrl- or Shift-wheel | Vertical scroll / horizontal scroll (all views in sync). | BaseView.cpp:2275-2330 |

## 5. Standard Win32 behaviour relied on (implement generically)
- Tab / Shift+Tab moves focus in dialog tab order (resource order of `WS_TABSTOP` controls). Arrow keys move within radio groups (`WS_GROUP`).
- Enter presses the default push button (`DEFPUSHBUTTON` / `DM_SETDEFID`). Esc sends `IDCANCEL`. Exceptions are listed per dialog above: Log, FileDiff and FindBar have guarded `OnOK`; the status list and SciEdit capture Enter/Esc.
- Alt+letter, or the plain letter when focus is on a non-edit control, activates the `&` mnemonic. On a button or checkbox it clicks or toggles. On a radio button it selects and focuses. On a static label or group box it focuses the next tab-stop control. Duplicate mnemonics cycle focus.
- Space toggles a focused checkbox or presses a focused button. Alt+Down/F4 opens a combo drop-down (but in the Log dialog, Alt+Up/Down are taken by the accelerators).
- List views: arrows, Home/End, PgUp/PgDn, Shift/Ctrl extend selection, type-ahead. Tree views: Left/Right collapse/expand, `*` expands all.
- Edits: Ctrl+Z, Ctrl+X/C/V, Shift+Del/Ins, Ctrl+Backspace (multi-line), Ctrl+Left/Right.
- Property sheet (Settings) bottom buttons: OK, Cancel, "&Apply", Help (system captions, inferred).
- Explorer context menu entries carry `&` accelerators (e.g. "&Commit..." = Alt+C clash with Explorer's Copy). They can be disabled via advanced setting `ShellMenuAccelerators` (doc dug_settings_advanced.xml:466-476). Shift+right-click shows the extended context menu (doc dug_general.xml:42).

## 6. Mnemonics (`&` captions) per dialog
Exact resource captions. OK, Cancel and Help carry **no** mnemonic in any of these dialogs unless shown. "dyn" = set at runtime.

**Commit** `IDD_COMMITDLG` (rc:245-297): "&Message:" (groupbox; Alt+M also an accelerator) · "Amend &Last Commit" · "Set author &date" · "Set au&thor" · "Add &Signed-off-by" · "&All" · "&None" (link labels; Alt=click) · "Show &Unversioned Files" · "Show &Whole Project" · "Message onl&y" · "C&ommit" (IDOK; dyn split entries "C&ommit" / "&ReCommit" / "Commit && &Push", rc:5130-5132). No mnemonic on: "Commit to:", "new branch", "Bug-ID/Issue-Nr:", "Show diff to last commit", "Reset", "Check:", "Unversioned", "Versioned", "Added", "Deleted", "Modified", "Files", "Submodules", "Staging support (EXPERIMENTAL)", "Do not autoselect submodules", "View Patch>>", "Partial Staging>>", "Partial Unstaging>>", Cancel, Help. Alt+F = focus file list (accelerator).

**Log** `IDD_LOGMESSAGE` (rc:210-243): "Show &Whole Project" · "&All Branches" · "S&tatistics" · "Walk Be&havior" · "&View". No mnemonic on: "From:", "To:", Help, Refresh, OK, Cancel. Plus accelerators Alt+D, Alt+arrows (§3.1).

**Push** `IDD_PUSH` (rc:1186-1223): "&Push all branches" · "&Local:" · "&Remote:" · "Re&mote:" (radio) · "Mana&ge" · "Arbitrary &URL:" · "Force &with lease" · "&Force" · "Include &Tags" · "&Autoload Putty Key" · "&Set upstream/track remote branch" · "Push &option:". No mnemonic on: "Ref", "Destination", "Options", "Always push to the selected remote archive for this local branch", "Always push to the selected remote branch for this local branch", "Recurse submodule", OK, Cancel, Help.

**Pull/Fetch** `IDD_PULLFETCH` (rc:1154-1184): "&Remote:" · "Arbitrary &URL:" · "Remote &Branch:" · "&Squash" · "No &Fast Forward" · "No Co&mmit" · "Fast Forward O&nly" · "AutoLoad Putty &Key" · "&Launch Rebase After Fetch". No mnemonic on: "Depth", "Tags", "Prune", "Manage Remotes" (link), OK, Cancel, Help.

**Sync** `IDD_SYNC` (rc:1679-1712): "&Local Branch:" · "&Remote Branch:" · "Remote &URL:" · "Autoload Putty &Key" · "&Force" · "&Apply Patch" · "&Email Patch" · "Show &log" · "&Commit". No mnemonic on: "Manage", "Close" (IDOK), Help. Menu-button captions are dyn: Pull button "&Pull" / "Fetc&h" / "Fetch && Re&base" / "Fetch all refs" / "Remote Update" / "Clean up stale remote branches" / "Compare tags"; Push button "Pus&h" / "Push ta&gs" / "Push notes"; Submodule "Submodule Update" / "Submodule Init" / "Submodule Sync"; Stash "Stash changes" / "Stash Pop" / … (rc:3583-3591,3646,4876,4970,5136,5182; SyncDlg.cpp:1174-1192). Only the currently shown entry acts as a mnemonic (inferred). Note the Alt+H clash between "Fetc&h" and "Pus&h".

**Clone** `IDD_CLONE` (rc:1098-1137): "&URL:" · "&Browse..." (URL) · "&Directory:" · "Bro&wse..." (dir) · "Load Putty &Key" · "From &SVN Repository" · "&Trunk:" · "Ta&gs:" · "Branc&h:" · "&From:" · "User&name:". No mnemonic on: "Depth", "Recursive", "Clone into Bare Repo", "No Checkout", "Branch", "Origin Name", OK, Cancel, Help.

**Switch/Checkout** `IDD_GITSWITCH` (rc:1259-1285): "&Branch" · "&Tag" · "&Commit" · "Create &New Branch" · "Overwrite working tree changes (&force)" · "&Merge" · "T&rack" · "&Override branch if exists". No mnemonic on: "Switch To", "Option", OK, Cancel, Help.

**Create Branch / Create Tag** (same template `IDD_NEW_BRANCH_TAG`, rc:1287-1316; title dyn "Create Branch"/"Create Tag", label dyn "Branch"/"Tag", CreateBranchTagDlg.cpp:86-105): "&Name" (groupbox) · "&HEAD" · "&Branch" · "&Tag" · "&Commit" · "Trac&k" · "&Force" · "&Switch to new branch" · "Si&gn" (tag only) · "&Push" (tag only) · "&Message" (tag; for a branch the group box is renamed "Description" without a mnemonic, rc:4945). No mnemonic on: "Base On", "Options", OK, Cancel, Help.

**Merge** `IDD_MERGE` (rc:1370-1402): "&Branch" · "&Tag" · "&Commit" · "&Squash" · "No &Fast Forward" · "Fast Forward O&nly" · "No Co&mmit" · "Merge &Message". No mnemonic on: "Current branch:", "From", "Option", "Messages", "Strategy", OK, Cancel, Help.

**Rebase** `IDD_REBASE` (rc:1499-1531): "&Branch:" · "&Upstream:" · "&Onto" · "&Up" · "&Down" · "&Add" · "&Preserve merges" · "&Force Rebase" · "&add ""cherry picked from""" (duplicate Alt+A with "&Add") · "&Edit/Split commit". No mnemonic on: "...", Continue/Abort/Help (dyn texts §3.3). While the commit list is focused, plain U/D/E/P/S/Q/Space are list commands (§3.3).

**Stash Save** `IDD_STASH` (rc:1812-1825): "Stash &Message" · "include &untracked" · "--&all". No mnemonic on: "Options", OK, Cancel, Help.

**Reset** `IDD_RESET` (rc:1470-1497): "&Branch" · "&Tag" · "&Commit" · "&Soft: Leave working tree and index untouched" · "&Mixed: Leave working tree untouched, reset index" · "&Hard: Reset working tree and index (discard all local changes)" · "Show modified files in &working tree". No mnemonic on: "Current branch:", "Reset active branch", "Reset Type", OK, Cancel, Help.

**Settings** (`CTreePropSheet`): system OK, Cancel, "&Apply", Help (inferred). The General page `IDD_SETTINGSMAIN` (rc:342-363) has "&Language:" · "Automatically check for &newer versions every week" · "C&heck now" · "Re-run &First Start Wizard" · "Create &Library" · "&Git.exe Path:" · "&Extra PATH:" · "C&heck now" (duplicate Alt+H). Page navigation: §3.6.

**Resolve** `IDD_RESOLVE` (rc:800-811): "Select/deselect &all". No mnemonic on: OK, Cancel, Help, "Reminder: Commit your change after resolve". Ctrl+Enter = OK.

**Clean** `IDD_CLEAN` (rc:1713-1735): "Remove &all untracked files (-fx)" · "Remove &non-ignored untracked files (-f)". No mnemonic on: "Remove ignored files (-fX)", "Remove untracked directories (-d)", "Remove unmanaged directories with .git folder (-f)", "Do not use recycle bin", "Dry run", "Submodules", OK, Cancel, Help.

**Revert** `IDD_REVERT` (rc:568-579): "Select/deselect &all". OK, Cancel, Help. Ctrl+Enter = OK, F5.

**Add** `IDD_ADD` (rc:174-185): "Select/deselect &all" · "Include &ignored files". OK, Cancel, Help. Ctrl+Enter = OK, F5.

**Delete/Remove**: no dialog. A `CMessageBox` with buttons "&Remove" / "&Abort" (default = button 2, Abort) (Commands/RemoveCommand.cpp:118-125; rc:3512,5100).

**Progress, git CLI** `IDD_GITPROGRESS` (rc:1139-1152): "Close" (default), "Abort", post-command menu button (dyn labels with mnemonics, §3.5). **Progress, libgit2** `IDD_SVNPROGRESS` (rc:465-478): "Show &log..." (menu button; dyn entries), OK, Cancel.

**Conflict dialog** `IDD_RESOLVE_CONFLICT` (rc:1533-1552): "Show &changes" (×2). **Merge Abort** `IDD_MERGEABORT` (rc:2020-2033): "M&erge: …", "&Mixed: …", "&Hard: …", "Show modified files in &working tree". **Apply Patches** `IDD_APPLY_PATCH_LIST` (rc:1318-1336): "&Add" · "&Up" · "&Down" · "&Remove" · "&3 way merge" · "&ignore space change" · "Add ""&Signed-off-by""" · "&Keep CR" · "A&pply" · "&Cancel". **Submodule Update** `IDD_SUBMODULE_UPDATE` (rc:1892-1913): "Select/deselect &all" · "&Whole Project" · "&Initialize submodules (--init)" · "&Recursive" · "&Force" · "&No fetch" · "&Merge" · "Re&base" · "Remote &tracking branch". **RefLog** `IDD_REFLOG` (rc:1554-1567): "&Ref:" · "&Search..." · "&Clear stash". **Browse refs** `IDD_BROWSE_REFS` (rc:1916-1933): "Show &nested refs".

## 7. Notable gaps and quirks to mirror (or record as divergences)
1. Log Esc runs in three steps: the file filter clears first, then the log filter, then the dialog closes (§3.1).
2. Enter never closes the Log dialog unless OK is focused. It never closes FileDiff.
3. Ctrl+Enter = OK only in Commit, Add, Revert, Resolve, Input and Rebase (Continue). Elsewhere it does nothing special.
4. F5 refresh exists only where listed. Clone, Pull/Fetch, Switch, Merge, Reset and similar dialogs have none.
5. Revision Graph menu claims Ctrl+/Ctrl- zoom, but no accelerator is loaded. Only Ctrl+wheel zooms.
6. The Commit dialog's dynamic Alt+O/R/P only exist when post-actions are enabled. With duplicate letters, Alt opens the drop-down.
7. In the Rebase commit list, Space/P/S/Q/E/U/D act **without modifiers** and override list type-ahead.

## 8. How KoopaGit implements this

- `ui/keys.slint` → `DialogKeys` wraps every dialog's content and provides §5: Enter that no control consumed calls `ok`, Escape calls `cancel`, and Alt+letter goes through `access-key(letter) -> bool` in the capture phase, before the focused control sees it. Access keys are off on macOS (`Platform.os`), where Option+letter types characters. The Window's `forward-focus` points at the `DialogKeys` (or at the control upstream focuses first), because Slint delivers no keys while nothing has focus.
- Single-line `LineEdit`s consume Enter, so an edit whose Enter should press the default button sets `accepted => { root.ok(); }`.
- Dialog-specific keys (F5, Ctrl+Enter, list keys) are `KeyBinding`s or `key-pressed` handlers inside the dialog's `DialogKeys`, so they bubble after the focused control. Keys that upstream handles in `PreTranslateMessage` ahead of the focused control (Commit's Ctrl+Enter, Log's Escape chain) use `capture-key-pressed`.
- `SpaceActivate` makes KoopaGit's hand-drawn controls (radio, tri-state check, link labels) tab stops that Space activates.
- Slint maps `Control` to ⌘ on macOS, so Ctrl+X shortcuts become ⌘X there automatically.
- Tests: `crates/koopagit-dialogs/tests/keyboard_*.rs` drive dialogs headlessly through `i-slint-backend-testing`.
