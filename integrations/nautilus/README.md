# KoopaGit for Nautilus (GNOME Files)

A nautilus-python 4.x extension providing TortoiseGit's context menu and status
emblems. It is deliberately thin: it shells out to `koopagit-shell` for menus
and statuses, and `koopagit-shell invoke` starts `koopagit-proc` detached.

## Install

```sh
cargo install --path crates/koopagit-shell   # koopagit-shell
cargo install --path crates/koopagit-proc    # koopagit-proc (dialogs)
cargo install --path crates/koopagit-merge   # koopagit-merge (KoopaGitMerge)
sudo apt install python3-nautilus            # or: dnf install nautilus-python
./integrations/nautilus/install.sh
nautilus -q
```

`koopagit-proc` and `koopagit-merge` must sit next to `koopagit-shell` or be
found via `KOOPAGIT_PROC` / `KOOPAGIT_MERGE`. Set `KOOPAGIT_SHELL` if the helper is not on Nautilus's PATH.

## Differences from Explorer

See `docs/divergences.md`: Nautilus shows emblems (several may stack, we add
one) instead of overlay icons, has no menu separators, and the top-level vs
"KoopaGit" submenu split follows the same `ContextMenuEntries` setting.
