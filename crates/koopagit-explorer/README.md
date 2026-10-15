# KoopaGit Explorer extension (Windows)

In-process COM server providing TortoiseGit's classic Explorer context menu
(`IShellExtInit` + `IContextMenu`) and status overlays (one
`IShellIconOverlayIdentifier` class per overlay slot). On macOS/Linux this
crate compiles to an empty library so the workspace stays buildable.

**Status: unverified.** It type-checks and passes clippy for
`x86_64-pc-windows-msvc` (`cargo clippy --target x86_64-pc-windows-msvc -p
koopagit-explorer`), but it has not been built into a DLL, registered or loaded
into Explorer yet.

## Layout next to the DLL

```
koopagit_explorer.dll
koopagit-proc.exe        launched for every menu action
koopagit-merge.exe       "Review/apply single patch" (when available)
icons\<Name>Icon.ico     Conflict, Modified, Deleted, ReadOnly, Locked, Added, Normal, Ignored, Unversioned
```

## Register

Edit the DLL path in `register.reg`, import it as administrator, then restart
Explorer. The CLSIDs are KoopaGit's own (`{5e1a8f0X-...}`), so it can coexist
with TortoiseGit. `unregister.reg` removes the same keys.

## Behaviour

- Menus come from `koopagit_shell::actions::query_menu` (same table, masks and
  early exits as the other platforms); items are inserted as `sep, top
  entries, TortoiseGit popup, sep`, with `&` accelerators kept.
- Multiple selected paths are passed to `koopagit-proc` through a UTF-16LE
  `/pathfile:` like upstream.
- Overlays share a one-entry status cache, like TortoiseGit's
  `g_filepath`/`g_filestatus`; status is computed in-process (no TGitCache
  equivalent yet; see `docs/divergences.md`).
- Not implemented yet: Windows 11 `IExplorerCommand` menu, right-drag menu,
  menu icons, `GetCommandString` help text.
