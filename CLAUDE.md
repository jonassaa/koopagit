# KoopaGit

A faithful clone of [TortoiseGit](https://tortoisegit.org/), written in Rust, running on Windows, macOS and Linux.

## North star: parity with TortoiseGit

TortoiseGit is the spec. When deciding how a feature should behave, look, or be named, match TortoiseGit first: same dialogs, same menu entries and ordering, same option names, same defaults, same workflow. Upstream source: https://gitlab.com/tortoisegit/tortoisegit (docs under `doc/`).

- Diverge only where the platform forces it (e.g. Finder has no overlay-icon API identical to Explorer's). Record every divergence in `docs/divergences.md` with the reason.
- Additions beyond TortoiseGit's feature set are out of scope unless the user asks.

## What TortoiseGit is (the surface to clone)

1. **Shell integration**: file-manager context menus and status overlay icons on files/folders.
   - Windows: Explorer shell extension (COM `IContextMenu`, `IShellIconOverlayIdentifier`).
   - macOS: Finder Sync Extension (badges + context menu; requires an app bundle).
   - Linux: Nautilus / Dolphin / Thunar extensions (one per file manager).
2. **Standalone dialogs** launched by the shell integration: Commit, Log, Diff, Merge, Rebase, Sync / Push / Pull / Fetch, Switch/Checkout, Branch, Tag, Stash, Blame, Repo Browser, Settings, Resolve Conflicts, Clone, Submodules.
3. **TortoiseGitMerge / TortoiseGitBlame / TortoiseGitUDiff**: separate diff/merge, blame and unified-diff viewers.
4. **Command-line launcher** (`TortoiseGitProc.exe /command:<name> /path:<path>`): the shell extension invokes dialogs through it. Keep the same command names so behaviour maps 1:1.

## Architecture rules

- Cargo workspace. The git/domain core is a library crate with no GUI or OS dependencies; GUI, CLI launcher and each shell extension are separate crates depending on it.
- Platform-specific code lives behind `cfg(target_os = ...)` in dedicated crates/modules, never sprinkled through shared logic.
- Shell extensions stay thin: they query status and spawn the launcher. Long-running work happens in the launched process, so a crash or hang never takes down the file manager.

## Settled decisions

- **Git backend**: a mix. Mutating operations (commit, merge, rebase, push/pull/fetch, stash, branch, tag, …) shell out to the `git` CLI with the same arguments TortoiseGit builds. Fast read-only queries (repo discovery, HEAD/branch, status for overlays, log walking) use `gix`.
- **Product name**: shown as "KoopaGit" wherever TortoiseGit shows its own name (window titles, message boxes, menus, KoopaGitMerge/Blame/UDiff). Crate, binary, env-var and config-path identifiers stay lowercase `koopagit`. Settings keys keep their upstream names.
- **GUI toolkit**: Slint (native-ish styles per OS).
- **Linux file manager first**: Nautilus (nautilus-python extension that calls the launcher).

## Git history

Keep `main` linear. Before merging a branch into `main`, rebase it onto the current `main` (`git rebase main`) so its commits sit on top. A merge commit (`git merge --no-ff`) is fine after that, but never merge an un-rebased branch, and never merge `main` into a feature branch.

## Verification

Every change ends with `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` passing on the workspace.
