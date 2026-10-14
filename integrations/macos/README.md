# KoopaGit for Finder (macOS)

A Finder Sync extension providing TortoiseGit's context menu and status badges,
packaged in `KoopaGit.app` together with the Rust binaries.

```
KoopaGit.app/Contents/MacOS/KoopaGit          host app; runs menu actions (request files / `KoopaGit invoke ...`)
KoopaGit.app/Contents/MacOS/koopagit-shell    menus + statuses (JSON / TSV)
KoopaGit.app/Contents/MacOS/koopagit-proc     the dialogs
KoopaGit.app/Contents/MacOS/koopagit-merge    KoopaGitMerge (diff / conflict / patch viewer)
KoopaGit.app/Contents/PlugIns/KoopaGitFinderSync.appex
```

## Build

```sh
integrations/macos/build.sh            # -> target/macos/KoopaGit.app (ad-hoc signed)
open target/macos/KoopaGit.app         # registers the extension with LaunchServices
pluginkit -e use -i dev.koopagit.FinderSync
```

Requires Xcode command-line tools (`xcrun swiftc`). Only `swiftc`, no Xcode
project.

## Install

```sh
integrations/macos/install.sh          # -> /Applications/KoopaGit.app
integrations/macos/install.sh ~/Applications
```

Builds into the given folder, unregisters other copies of the extension,
registers and enables it, and restarts Finder. Re-run to update.

It also installs a `koopagit` command (a wrapper around the bundled
`koopagit-proc`) into `$KOOPAGIT_BIN_DIR`, default `~/.local/bin`:

```sh
koopagit log                 # same as: koopagit /command:log
koopagit commit src/main.rs  # same as: koopagit /command:commit /path:src/main.rs
koopagit help log            # options of a command
```

## Watched folders

Finder Sync only runs inside configured folders. List them, one per line, in
`~/.config/koopagit/finder-roots` (`~` is expanded; `#` starts a comment). If
the file is missing, the home folder is watched.

## How it stays thin

- The extension is sandboxed (Finder requires it). It runs the bundled
  `koopagit-shell` for badges (`status`) and menus (`menu`), with a
  development-only read-only file exception so the helper can read
  repositories.
- Menu actions are not run from the sandbox. Sandboxed callers cannot pass
  launch arguments, so the extension writes the item's invoke array (JSON) to
  a `.koopagit-invoke` file in its container and opens it with the host app,
  which runs `koopagit-shell invoke` and starts `koopagit-proc` unsandboxed.
  The host ignores request files from anywhere else. `KoopaGit invoke <args>`
  does the same from a terminal.

## Status

Built and signature-verified locally (`codesign --verify --deep --strict`).
Not yet verified: loading the extension in Finder (pluginkit/registration) and
the sandbox allowing the helper to execute. A distributable build needs a
Developer ID, notarisation and a design without temporary sandbox exceptions
(e.g. an XPC service in the host app).
