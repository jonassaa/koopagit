# KoopaGit

A clone of [TortoiseGit](https://tortoisegit.org/) written in Rust that runs on
Windows, macOS and Linux. It adds file-manager context menus and status icons,
and has the same dialogs as TortoiseGit: Commit, Log, Diff, Merge, Rebase,
Push/Pull/Fetch, Blame, Settings and the rest. KoopaGitMerge is the diff and
conflict viewer.

> **Status: early development.** There are no prebuilt releases yet. Each
> platform is installed by building from source, as described below. The
> Windows Explorer extension is experimental (see [Windows](#windows)).

| Platform | File manager integration | Install |
|---|---|---|
| macOS 12+ | Finder Sync extension (badges + context menu) | [macOS](#macos) |
| Linux | Nautilus (GNOME Files) 43+ extension (emblems + context menu) | [Linux](#linux-nautilus) |
| Windows 10/11 | Explorer shell extension (context menu; experimental) | [Windows](#windows) |

## Requirements (all platforms)

- **Rust 1.85 or newer**, installed with [rustup](https://rustup.rs/).
- **Git** on `PATH`. KoopaGit runs `git` for every operation that changes a
  repository. To use a different binary, set `KOOPAGIT_GIT=/path/to/git`.

Then get the source:

```sh
git clone https://github.com/jonassaa/koopagit.git
cd koopagit
```

## macOS

Prerequisites: the Xcode command-line tools (`xcode-select --install`). You
don't need the full Xcode.

```sh
integrations/macos/install.sh                # installs /Applications/KoopaGit.app
# or, without admin rights:
integrations/macos/install.sh ~/Applications
```

The script does the following:

1. Builds the Rust binaries and the Finder Sync extension.
2. Packages them as `KoopaGit.app` (ad-hoc signed, for local use).
3. Registers and enables the extension, then restarts Finder.
4. Installs a `koopagit` command into `~/.local/bin` (set `KOOPAGIT_BIN_DIR`
   to put it elsewhere).

Run it again to update.

If the script says the extension is not enabled, turn on **KoopaGit** in
**System Settings → General → Login Items & Extensions → Finder**.

By default Finder shows badges and menus everywhere under your home folder.
To limit them to some folders, list those folders one per line in
`~/.config/koopagit/finder-roots`.

For details, see [integrations/macos/README.md](integrations/macos/README.md).

**Uninstall:**

```sh
pluginkit -e ignore -i dev.koopagit.FinderSync
rm -rf /Applications/KoopaGit.app ~/.local/bin/koopagit   # or ~/Applications/KoopaGit.app
killall Finder
```

## Linux (Nautilus)

The extension needs nautilus-python 4.x, which means Nautilus 43 or newer.
That is Ubuntu 23.04+, Debian 12+, Fedora 37+ or a current Arch.

**1. Install build tools and the Nautilus Python bindings:**

```sh
# Debian / Ubuntu
sudo apt install build-essential pkg-config libfontconfig-dev git python3-nautilus
# Fedora
sudo dnf install gcc pkgconf-pkg-config fontconfig-devel git nautilus-python
# Arch
sudo pacman -S --needed base-devel fontconfig git python-nautilus
```

**2. Build and install the binaries** (into `~/.cargo/bin`):

```sh
cargo install --locked --path crates/koopagit-shell   # menus + status helper
cargo install --locked --path crates/koopagit-proc    # the dialogs
cargo install --locked --path crates/koopagit-merge   # KoopaGitMerge
```

**3. Install the Nautilus extension and emblems**, then restart Nautilus:

```sh
integrations/nautilus/install.sh
nautilus -q
```

Nautilus can only use the extension if it finds `koopagit-shell`. If the
KoopaGit menu or emblems don't appear, your desktop session probably doesn't
have `~/.cargo/bin` on `PATH`. Point Nautilus at the helper directly, then log
out and back in:

```sh
mkdir -p ~/.config/environment.d
echo 'KOOPAGIT_SHELL=${HOME}/.cargo/bin/koopagit-shell' > ~/.config/environment.d/koopagit.conf
```

To get a `koopagit` command like the one on macOS, create a symlink:
`ln -s ~/.cargo/bin/koopagit-proc ~/.local/bin/koopagit`.

For details, see [integrations/nautilus/README.md](integrations/nautilus/README.md).

**Update:** `git pull`, then repeat steps 2 and 3.

**Uninstall:**

```sh
cargo uninstall koopagit-shell koopagit-proc koopagit-merge
rm ~/.local/share/nautilus-python/extensions/koopagit_nautilus.py
rm ~/.local/share/icons/hicolor/scalable/emblems/emblem-koopagit-*.svg
rm -f ~/.local/bin/koopagit ~/.config/environment.d/koopagit.conf
nautilus -q
```

## Windows

Prerequisites:

- Rust with the default MSVC toolchain. rustup asks you to install the
  **Visual Studio Build Tools** (workload "Desktop development with C++") if
  they are missing.
- [Git for Windows](https://gitforwindows.org/).

**1. Build** (in a normal PowerShell, from the repository folder):

```powershell
cargo build --release -p koopagit-proc -p koopagit-merge -p koopagit-explorer
```

**2. Install** (in an **administrator** PowerShell, from the repository folder):

```powershell
$dest = "$env:ProgramFiles\KoopaGit"
New-Item -ItemType Directory -Force $dest | Out-Null
Copy-Item target\release\koopagit-proc.exe, target\release\koopagit-merge.exe,
          target\release\koopagit_explorer.dll $dest
```

At this point you can open the dialogs from a terminal, for example
`& "$env:ProgramFiles\KoopaGit\koopagit-proc.exe" log`. Add the folder to your
`PATH` to type just `koopagit-proc log`.

**3. Explorer context menu (experimental).** The extension type-checks, but it
has never been loaded into Explorer. Try it only if you are fine with
restarting Explorer, or unregistering the extension, if something goes wrong.

```powershell
reg import crates\koopagit-explorer\register.reg
Stop-Process -Name explorer        # Windows restarts it automatically
```

`register.reg` assumes `C:\Program Files\KoopaGit`. If you installed somewhere
else, edit the DLL path in the file first. On Windows 11 the menu is under
**Show more options**. The extension uses its own CLSIDs, so it can be
installed next to TortoiseGit.

Status overlay icons don't work yet, because the extension expects
`icons\<Name>Icon.ico` files next to the DLL and the repository doesn't ship
any.

For details, see [crates/koopagit-explorer/README.md](crates/koopagit-explorer/README.md).

**Uninstall** (administrator PowerShell):

```powershell
reg import crates\koopagit-explorer\unregister.reg
Stop-Process -Name explorer
Remove-Item -Recurse "$env:ProgramFiles\KoopaGit"
```

If Explorer still has the DLL loaded, restart Explorer again and retry the
`Remove-Item`.

## Using KoopaGit

The usual way to use KoopaGit is the file manager: right-click a file or
folder inside a repository. You can also open the dialogs from a terminal,
with either of these syntaxes:

```sh
koopagit log                          # conventional subcommands
koopagit commit src/main.rs
koopagit help log                     # a command's options
koopagit /command:log /path:.         # TortoiseGitProc's syntax
```

The `koopagit` command is `koopagit-proc`, under whatever name you installed
it.

Settings are stored in `~/.config/koopagit/settings.ini`. On Windows the file
is `%APPDATA%\koopagit\settings.ini`. Set `KOOPAGIT_SETTINGS` to use another
file. The setting names are the same as TortoiseGit's.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

The crate layout and the shell-extension flow are described in
[docs/architecture.md](docs/architecture.md). Deliberate differences from
TortoiseGit are listed in [docs/divergences.md](docs/divergences.md).
