# Architecture

```
crates/koopagit-core       git/domain library (no GUI/OS deps): gix status/log, git CLI,
                           TortoiseGitProc argument grammar, git command lines, settings store
crates/koopagit-shell      platform-independent shell logic: ITEMIS flags, menu table,
                           overlays, path filters; plus the `koopagit-shell` helper binary
crates/koopagit-dialogs    Slint dialogs (TortoiseProc's dialogs)
crates/koopagit-proc       the launcher: `koopagit-proc /command:<name> /path:<p>`,
                           or `koopagit <command> [options] [path...]`
crates/koopagit-explorer   Windows Explorer COM extension (empty elsewhere)
integrations/nautilus      nautilus-python extension + emblems (Linux)
integrations/macos         Finder Sync extension + host app (macOS)
docs/spec                  behaviour specs extracted from upstream TortoiseGit
```

## Flow

1. A file manager asks for a menu or a status:
   - Windows: the in-process DLL calls `koopagit-shell` logic directly.
   - macOS/Linux: the extension runs `koopagit-shell menu ...` (JSON) or
     `koopagit-shell status ...` (`overlay<TAB>path` lines).
2. Clicking a menu item runs its `invoke` array (`koopagit-shell invoke ...`),
   which starts `koopagit-proc` detached with TortoiseGitProc-compatible
   arguments (multiple paths via `/pathfile:`).
3. `koopagit-proc` finds the repository, loads settings and opens the dialog.
   Mutating git operations shell out to `git` with TortoiseGit's arguments.

Extensions never run git operations or dialogs themselves, so a crash or hang
in a dialog cannot take the file manager down.
