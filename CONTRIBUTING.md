# Contributing to KoopaGit

Thanks for helping! KoopaGit is a clone of [TortoiseGit](https://tortoisegit.org/),
so most contributions involve making some part of it behave more like TortoiseGit.

## Ground rules

- **TortoiseGit is the spec.** Dialogs, menu entries and their order, option
  names, defaults and workflows should match TortoiseGit. When you're not sure
  how something should behave, check the
  [TortoiseGit source](https://gitlab.com/tortoisegit/tortoisegit) (the manual
  is under `doc/`) and copy what it does.
- **Diverge only when the platform forces it.** Every deliberate difference is
  recorded, with the reason, in [docs/divergences.md](docs/divergences.md).
- **No new features.** Features that TortoiseGit doesn't have are out of scope.
  Open an issue to discuss before you start on one.
- KoopaGit uses its own name ("KoopaGit", "KoopaGitMerge") wherever TortoiseGit
  shows its name. Setting keys keep their upstream names.

## Reporting issues

- **Bugs:** use the *Bug report* template. Include your platform, `git --version`
  and the commit you built from.
- **Differences from TortoiseGit:** use the *Differs from TortoiseGit* template.
- **Security vulnerabilities:** report them privately, as described in
  [SECURITY.md](SECURITY.md). Don't open a public issue.

## Building

See the [README](README.md) for each platform's prerequisites. The minimum
supported Rust version is 1.92. To build and run the dialogs from the checkout:

```sh
cargo run -p koopagit-proc -- log
```

[docs/architecture.md](docs/architecture.md) describes the crate layout and how
the shell extensions reach the dialogs. In short:

- `koopagit-core` is the git and domain layer. It has no GUI or OS dependencies.
- Mutating git operations shell out to the `git` CLI with the same arguments
  TortoiseGit builds. Fast read-only queries use `gix`.
- Platform-specific code lives behind `cfg(target_os = ...)` in its own crate
  or module.
- Shell extensions stay thin: they query status and start the launcher.

## Before you open a pull request

All of these must pass on the workspace. CI runs them on Linux, macOS and
Windows.

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Keep `main`'s history linear: rebase your branch onto the current `main` before
you open the pull request (or when asked to), and don't merge `main` into it.

## License

KoopaGit is licensed under the [GNU General Public License v3.0 or later](LICENSE).
By contributing, you agree that your contributions are licensed under the same
terms.

Everyone taking part in the project is expected to follow the
[Code of Conduct](CODE_OF_CONDUCT.md).
