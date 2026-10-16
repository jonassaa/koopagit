# KoopaGit

[![CI](https://github.com/jonassaa/koopagit/actions/workflows/ci.yml/badge.svg)](https://github.com/jonassaa/koopagit/actions/workflows/ci.yml)
[![License: GPL v3+](https://img.shields.io/badge/license-GPL--3.0--or--later-blue.svg)](LICENSE)

A clone of [TortoiseGit](https://tortoisegit.org/) written in Rust that runs on
Windows, macOS and Linux. It adds file-manager context menus and status icons,
and has the same dialogs as TortoiseGit: Commit, Log, Diff, Merge, Rebase,
Push/Pull/Fetch, Blame, Settings and the rest. KoopaGitMerge is the diff and
conflict viewer.

> **Status: early development.** There are no prebuilt releases yet:
> KoopaGit is built from source on your machine, which the installer does for
> you. The Windows Explorer extension is experimental.

| Platform | File manager integration |
|---|---|
| macOS 12+ | Finder Sync extension (badges + context menu) |
| Linux | Nautilus (GNOME Files) 43+ extension (emblems + context menu) |
| Windows 10/11 | Explorer shell extension (context menu; experimental) |

## Install

**macOS and Linux**, in a terminal:

```sh
curl -fsSL https://raw.githubusercontent.com/jonassaa/koopagit/main/install.sh | sh
```

**Windows**, in PowerShell (not as administrator; it asks for admin rights
once, when it needs them):

```powershell
irm https://raw.githubusercontent.com/jonassaa/koopagit/main/install.ps1 | iex
```

The installer is a full-screen wizard with a progress bar: **Check →
Setup → Prepare → Build → Install → Finish**. After checking your system it
shows the recommended setup. Press Enter to install with it, or choose
**Customize...** to pick the install location, where the `koopagit` command
goes, the file-manager integration and PATH changes yourself. Use the arrow
keys (or the item's number) to choose, and Enter to confirm.

The prerequisites it installs when they're missing:

| | macOS | Linux | Windows |
|---|---|---|---|
| Build tools | Xcode command-line tools | C compiler, pkg-config, fontconfig headers (apt, dnf or pacman) | Visual Studio 2022 Build Tools, C++ workload (winget) |
| GUI libraries | built in | X11, Wayland, xkbcommon and OpenGL libraries (apt, dnf or pacman) | built in |
| Git | comes with the command-line tools | system package | Git for Windows (winget) |
| Rust 1.92+ | rustup | rustup | rustup |

Then it downloads the source into `~/.local/src/koopagit` (Windows:
`%LOCALAPPDATA%\KoopaGit\src`), builds it (the first build takes a few
minutes), installs it, and checks that the result works. It also offers to
set your git name and email if they're missing, like TortoiseGit's
first-start wizard does.

The recommended setup, and the install locations **Customize...** offers:

| | Recommended install location | Other locations | File manager |
|---|---|---|---|
| macOS | `/Applications/KoopaGit.app`, `koopagit` command in `~/.local/bin` | `~/Applications` (only for you), any folder | Finder extension, enabled for you; Finder restarts |
| Linux | `~/.cargo/bin` (set by cargo; change with `CARGO_HOME`), `koopagit` command in `~/.local/bin` | command: `/usr/local/bin`, any folder, or none | Nautilus extension and emblems; log out and back in once |
| Windows | `C:\Program Files\KoopaGit` (asks for admin rights), added to your `PATH` | `%LOCALAPPDATA%\Programs\KoopaGit` (only for you, no admin rights), any folder | Explorer extension off by default (experimental, see [Windows](#windows)) |

An existing installation is updated where it is. The build's output goes to
a log file (`~/Library/Logs/KoopaGit/install.log`,
`~/.local/state/koopagit/install.log`, or `%LOCALAPPDATA%\KoopaGit\install.log`);
if a step fails, the wizard shows the end of the log.

If Finder doesn't show KoopaGit, turn it on in **System Settings → General →
Login Items & Extensions → Finder**. To limit Finder's badges and menus to
some folders, list them one per line in `~/.config/koopagit/finder-roots`.

**Update:** run the same command again.

**Check** what's installed and what's missing, without changing anything,
or **uninstall**:

```sh
curl -fsSL https://raw.githubusercontent.com/jonassaa/koopagit/main/install.sh | sh -s -- --check
curl -fsSL https://raw.githubusercontent.com/jonassaa/koopagit/main/install.sh | sh -s -- --uninstall
```

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/jonassaa/koopagit/main/install.ps1))) -Check
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/jonassaa/koopagit/main/install.ps1))) -Uninstall
```

From a clone, run `./install.sh` (Windows:
`powershell -ExecutionPolicy Bypass -File .\install.ps1`) with the same
options. The installer then builds that clone instead of downloading one.

| Option (`install.sh` / `install.ps1`) | Effect |
|---|---|
| `--check` / `-Check` | Report what's installed and what's missing |
| `--uninstall` / `-Uninstall` | Remove KoopaGit (asks before deleting your settings) |
| `--yes` / `-Yes` | Install with the recommended setup without asking |
| `-Explorer` (Windows only) | Register the Explorer extension without asking |
| `-Dest <folder>` (Windows only) | Install somewhere other than `C:\Program Files\KoopaGit` |

Environment variables: `KOOPAGIT_SRC` (where to download the source),
`KOOPAGIT_BIN_DIR` (where to put the `koopagit` command; macOS and Linux),
`KOOPAGIT_APPS_DIR` (where to put `KoopaGit.app`; macOS), and
`CARGO_BUILD_JOBS`. If you don't set `CARGO_BUILD_JOBS`, the installer allows
one parallel build job per 3 GiB of RAM so the build doesn't run out of
memory.

The Windows installer is new. If it fails, follow the [manual steps](#windows)
and [open an issue](https://github.com/jonassaa/koopagit/issues).

## Manual install

These are the steps the installer runs, if you'd rather do them yourself.

### Requirements (all platforms)

- **Rust 1.92 or newer**, installed with [rustup](https://rustup.rs/).
- **Git** on `PATH`. KoopaGit runs `git` for every operation that changes a
  repository. To use a different binary, set `KOOPAGIT_GIT=/path/to/git`.

Then get the source:

```sh
git clone https://github.com/jonassaa/koopagit.git
cd koopagit
```

### macOS

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

If the script says the extension is not enabled, turn it on as described in
[Install](#install).

For details, see [integrations/macos/README.md](integrations/macos/README.md).

**Uninstall:**

```sh
pluginkit -e ignore -i dev.koopagit.FinderSync
rm -rf /Applications/KoopaGit.app ~/.local/bin/koopagit   # or ~/Applications/KoopaGit.app
killall Finder
```

### Linux (Nautilus)

The extension needs nautilus-python 4.x, which means Nautilus 43 or newer.
That is Ubuntu 23.04+, Debian 12+, Fedora 37+ or a current Arch.

**1. Install build tools, the libraries the dialogs open windows with, and the
Nautilus Python bindings:**

```sh
# Debian / Ubuntu
sudo apt install build-essential pkg-config libfontconfig-dev git python3-nautilus \
    libxkbcommon0 libxkbcommon-x11-0 libx11-6 libx11-xcb1 libxcursor1 libxrandr2 libxi6 \
    libwayland-client0 libwayland-cursor0 libwayland-egl1 libegl1 libgl1
# Fedora
sudo dnf install gcc pkgconf-pkg-config fontconfig-devel git nautilus-python \
    libxkbcommon libxkbcommon-x11 libX11 libX11-xcb libXcursor libXrandr libXi \
    libwayland-client libwayland-cursor libwayland-egl mesa-libEGL mesa-libGL
# Arch
sudo pacman -S --needed base-devel fontconfig git python-nautilus \
    libxkbcommon libxkbcommon-x11 libx11 libxcursor libxrandr libxi wayland libglvnd
```

A desktop install usually has the GUI libraries already. Minimal systems such
as Ubuntu on WSL don't, and the dialogs then stop with
`libxkbcommon-x11.so is not installed`. The dialogs run under WSLg on Windows
11; leave out `python3-nautilus` there if you don't use Nautilus.

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

### Windows

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

See [CONTRIBUTING.md](CONTRIBUTING.md) before you open a pull request, and
[SECURITY.md](SECURITY.md) to report a vulnerability.

## License

KoopaGit is free software: you can redistribute it and/or modify it under the
terms of the [GNU General Public License](LICENSE), version 3 or (at your
option) any later version.

KoopaGit is not affiliated with the TortoiseGit project. It reimplements
TortoiseGit's behaviour, and TortoiseGit is also licensed under the GPL.
