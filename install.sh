#!/bin/sh
# KoopaGit installer for macOS and Linux (Windows: install.ps1).
#
# An interactive wizard: checks every prerequisite, offers to install the
# missing ones, builds KoopaGit from source, installs the file-manager
# integration (Finder Sync on macOS, Nautilus on Linux) and verifies the result.
#
#   ./install.sh                 run the wizard (also updates an existing install)
#   ./install.sh --check         only report what is installed and what is missing
#   ./install.sh --uninstall     remove KoopaGit
#   ./install.sh --yes           accept every default without asking
#
# Outside a checkout it clones the sources into $KOOPAGIT_SRC
# (default: ~/.local/src/koopagit), so it also works as
#   curl -fsSL https://raw.githubusercontent.com/jonassaa/koopagit/main/install.sh | sh
set -eu

MIN_RUST=1.92
MIN_MACOS=12
MIN_NAUTILUS=43
REPO_URL=https://github.com/jonassaa/koopagit.git
FINDER_EXT_ID=dev.koopagit.FinderSync
MARKER="# added by the KoopaGit installer"

mode=install
assume_yes=0
for arg in "$@"; do
    case $arg in
        -y | --yes) assume_yes=1 ;;
        --check) mode=check ;;
        --uninstall) mode=uninstall ;;
        -h | --help)
            sed -n '2,16s/^# \{0,1\}//p' "$0"
            exit 0
            ;;
        *)
            echo "error: unknown option '$arg' (see --help)" >&2
            exit 2
            ;;
    esac
done

# ---------------------------------------------------------------- output

if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
    c_bold=$(printf '\033[1m') c_red=$(printf '\033[31m') c_green=$(printf '\033[32m')
    c_yellow=$(printf '\033[33m') c_blue=$(printf '\033[34m') c_off=$(printf '\033[0m')
else
    c_bold='' c_red='' c_green='' c_yellow='' c_blue='' c_off=''
fi

step() { printf '\n%s==>%s %s%s%s\n' "$c_blue" "$c_off" "$c_bold" "$*" "$c_off"; }
ok() { printf '  %s✓%s %s\n' "$c_green" "$c_off" "$*"; }
warn() { printf '  %s!%s %s\n' "$c_yellow" "$c_off" "$*"; }
bad() { printf '  %s✗%s %s\n' "$c_red" "$c_off" "$*"; }
info() { printf '    %s\n' "$*"; }
die() {
    printf '\n%serror:%s %s\n' "$c_red" "$c_off" "$*" >&2
    exit 1
}

# Show a command, then run it.
run() {
    printf '    %s$ %s%s\n' "$c_bold" "$*" "$c_off"
    "$@"
}

# ---------------------------------------------------------------- prompts

# Prompts read from the terminal, so `curl ... | sh` still works.
tty_ok=0
if (exec </dev/tty) 2>/dev/null; then tty_ok=1; fi

interactive() { [ "$assume_yes" = 0 ] && [ "$tty_ok" = 1 ]; }

# confirm "Question?" y|n  -> status 0 for yes
confirm() {
    if ! interactive; then
        [ "$2" = y ]
        return
    fi
    if [ "$2" = y ]; then hint='[Y/n]'; else hint='[y/N]'; fi
    while :; do
        printf '  %s?%s %s %s ' "$c_bold" "$c_off" "$1" "$hint"
        IFS= read -r reply </dev/tty || reply=
        case $reply in
            '')
                [ "$2" = y ]
                return
                ;;
            [yY]*) return 0 ;;
            [nN]*) return 1 ;;
        esac
    done
}

# ask "Question" default  -> answer in $answer, "~" expanded
ask() {
    answer=$2
    if interactive; then
        printf '  %s?%s %s [%s] ' "$c_bold" "$c_off" "$1" "$2"
        IFS= read -r reply </dev/tty || reply=
        [ -n "$reply" ] && answer=$reply
    fi
    case $answer in
        '~') answer=$HOME ;;
        '~/'*) answer=$HOME/${answer#'~/'} ;;
    esac
}

pause() {
    if interactive; then
        printf '  %s\n  Press Enter to continue... ' "$1"
        IFS= read -r _ </dev/tty || true
    fi
}

# ---------------------------------------------------------------- helpers

have() { command -v "$1" >/dev/null 2>&1; }

# ver_ge 1.92.1 1.92  -> status 0 when the first version is >= the second
ver_ge() {
    awk -v a="$1" -v b="$2" 'BEGIN {
        n = split(a, x, "."); m = split(b, y, ".")
        for (i = 1; i <= 3; i++) {
            if (x[i] + 0 > y[i] + 0) exit 0
            if (x[i] + 0 < y[i] + 0) exit 1
        }
        exit 0 }'
}

as_root() {
    if [ "$(id -u)" = 0 ]; then
        run "$@"
    elif have sudo; then
        run sudo "$@"
    else
        die "need root for '$*', and sudo is not installed"
    fi
}

on_path() {
    case ":$PATH:" in *":$1:"*) return 0 ;; esac
    return 1
}

shell_rc() {
    case $(basename "${SHELL:-sh}") in
        zsh) echo "$HOME/.zshrc" ;;
        bash) if [ "$os" = macos ]; then echo "$HOME/.bash_profile"; else echo "$HOME/.bashrc"; fi ;;
        fish) echo "${XDG_CONFIG_HOME:-$HOME/.config}/fish/config.fish" ;;
        *) echo "$HOME/.profile" ;;
    esac
}

add_to_path() {
    rc=$(shell_rc)
    mkdir -p "$(dirname "$rc")"
    case $rc in
        *.fish) line="fish_add_path \"$1\" $MARKER" ;;
        *) line="export PATH=\"$1:\$PATH\" $MARKER" ;;
    esac
    if [ -f "$rc" ] && grep -qF "$line" "$rc"; then return; fi
    printf '\n%s\n' "$line" >>"$rc"
    ok "Added $1 to PATH in $rc (open a new terminal to pick it up)"
}

# Make an existing rustup install visible to this script.
if ! have cargo && [ -f "${CARGO_HOME:-$HOME/.cargo}/env" ]; then
    . "${CARGO_HOME:-$HOME/.cargo}/env"
fi
cargo_bin=${CARGO_HOME:-$HOME/.cargo}/bin

# ---------------------------------------------------------------- platform

case $(uname -s) in
    Darwin) os=macos ;;
    Linux) os=linux ;;
    MINGW* | MSYS* | CYGWIN*) die "on Windows, run install.ps1 in PowerShell instead" ;;
    *) die "unsupported system: $(uname -s)" ;;
esac

distro= distro_name= pm=
if [ "$os" = linux ] && [ -r /etc/os-release ]; then
    distro=$(. /etc/os-release && echo "${ID:-} ${ID_LIKE:-}")
    distro_name=$(. /etc/os-release && echo "${PRETTY_NAME:-${NAME:-Linux}}")
    case " $distro " in
        *" debian "* | *" ubuntu "*) pm=apt ;;
        *" fedora "* | *" rhel "* | *" centos "*) pm=dnf ;;
        *" arch "* | *" manjaro "* | *" endeavouros "*) pm=pacman ;;
    esac
fi

# nautilus-python is only added when Nautilus itself is present, so a KDE or
# Xfce desktop doesn't get Nautilus libraries pulled in.
linux_packages() {
    case $pm in
        apt) pkgs='build-essential pkg-config libfontconfig-dev git curl' np=python3-nautilus ;;
        dnf) pkgs='gcc pkgconf-pkg-config fontconfig-devel git curl' np=nautilus-python ;;
        pacman) pkgs='base-devel fontconfig git curl' np=python-nautilus ;;
    esac
    if is_missing nautilus; then echo "$pkgs"; else echo "$pkgs $np"; fi
}

install_linux_packages() {
    # shellcheck disable=SC2046 # word splitting of the package list is intended
    case $pm in
        apt) as_root apt-get update && as_root apt-get install -y $(linux_packages) ;;
        dnf) as_root dnf install -y $(linux_packages) ;;
        pacman) as_root pacman -S --needed --noconfirm $(linux_packages) ;;
    esac
}

# Locate the source checkout: next to this script, the current directory,
# or $KOOPAGIT_SRC.
is_checkout() { [ -f "$1/Cargo.toml" ] && [ -d "$1/crates/koopagit-proc" ]; }
here=$(cd "$(dirname "$0")" 2>/dev/null && pwd || pwd)
if is_checkout "$here"; then
    src=$here
elif is_checkout "$(pwd)"; then
    src=$(pwd)
else
    src=${KOOPAGIT_SRC:-$HOME/.local/src/koopagit}
fi

# ---------------------------------------------------------------- checks

# Each check prints one line and records what is missing in $missing.
missing=
need() { missing="$missing $1"; }
is_missing() { case " $missing " in *" $1 "*) return 0 ;; esac; return 1; }

check_macos_version() {
    v=$(sw_vers -productVersion)
    if ver_ge "$v" "$MIN_MACOS"; then ok "macOS $v"; else
        bad "macOS $v (KoopaGit needs macOS $MIN_MACOS or newer)"
        need macos
    fi
}

clt_ok() { xcode-select -p >/dev/null 2>&1 && xcrun --find swiftc >/dev/null 2>&1; }

check_clt() {
    if clt_ok; then ok "Xcode command-line tools ($(xcode-select -p))"; else
        bad "Xcode command-line tools (Swift compiler, codesign) not installed"
        need clt
    fi
}

check_git() {
    # On macOS /usr/bin/git is a stub until the command-line tools exist.
    if have git && { [ "$os" = linux ] || [ "$(command -v git)" != /usr/bin/git ] || clt_ok; }; then
        ok "$(git --version)"
    else
        bad "git not installed"
        need git
    fi
}

check_git_identity() {
    is_missing git && return
    name=$(git config --global user.name 2>/dev/null || true)
    email=$(git config --global user.email 2>/dev/null || true)
    if [ -n "$name" ] && [ -n "$email" ]; then ok "git identity: $name <$email>"; else
        warn "git user.name / user.email not set (needed to commit)"
        need identity
    fi
}

check_rust() {
    if ! have cargo || ! have rustc; then
        bad "Rust not installed (needs $MIN_RUST or newer)"
        need rust
        return
    fi
    v=$(rustc --version | awk '{ print $2 }')
    if ver_ge "$v" "$MIN_RUST"; then ok "Rust $v"; else
        bad "Rust $v is too old (needs $MIN_RUST or newer)"
        need rust_old
    fi
}

# The release build (Slint's generated code especially) needs a lot of memory
# per rustc process; with one job per core an 8 GiB machine can run out. Cap
# the parallel jobs at one per 3 GiB unless CARGO_BUILD_JOBS is already set.
check_memory() {
    if [ "$os" = macos ]; then
        mem_kib=$(($(sysctl -n hw.memsize) / 1024))
        cpus=$(sysctl -n hw.ncpu)
    else
        mem_kib=$(awk '/^MemTotal:/ { print $2 }' /proc/meminfo)
        cpus=$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 1)
    fi
    mem_gib=$((mem_kib / 1024 / 1024))
    if [ -n "${CARGO_BUILD_JOBS:-}" ]; then
        ok "${mem_gib} GiB RAM, $cpus CPUs (CARGO_BUILD_JOBS=$CARGO_BUILD_JOBS)"
        return
    fi
    jobs=$((mem_gib / 3))
    [ "$jobs" -lt 1 ] && jobs=1
    if [ "$jobs" -lt "$cpus" ]; then
        export CARGO_BUILD_JOBS=$jobs
        ok "${mem_gib} GiB RAM, $cpus CPUs: building with $jobs parallel jobs to avoid running out of memory"
    else
        ok "${mem_gib} GiB RAM, $cpus CPUs"
    fi
    [ "$mem_gib" -lt 4 ] && warn "Less than 4 GiB RAM: the build may still run out of memory"
    return 0
}

check_curl() {
    if is_missing rust && ! have curl; then
        bad "curl not installed (needed to install Rust)"
        need curl
    fi
}

check_cc() {
    if have cc || have gcc || have clang; then ok "C compiler"; else
        bad "C compiler not installed"
        need cc
    fi
}

check_fontconfig() {
    if have pkg-config && pkg-config --exists fontconfig; then
        ok "pkg-config + fontconfig headers"
    else
        bad "pkg-config or the fontconfig development headers are missing"
        need fontconfig
    fi
}

nautilus_python_lib() {
    for d in /usr/lib/nautilus/extensions-4 /usr/lib64/nautilus/extensions-4 \
        /usr/lib/*/nautilus/extensions-4 /usr/local/lib/nautilus/extensions-4 \
        /usr/local/lib/*/nautilus/extensions-4; do
        if [ -e "$d/libnautilus-python.so" ]; then
            echo "$d/libnautilus-python.so"
            return 0
        fi
    done
    return 1
}

check_nautilus() {
    if ! have nautilus; then
        warn "Nautilus not found: the dialogs will work, but there is no file-manager integration"
        need nautilus
        return
    fi
    # `nautilus --version` needs a display; fall back to the package database.
    v=$(nautilus --version 2>/dev/null | sed 's/[^0-9.]*//g')
    [ -n "$v" ] || v=$(dpkg-query -W -f '${Version}' nautilus 2>/dev/null ||
        rpm -q --qf '%{VERSION}' nautilus 2>/dev/null ||
        pacman -Q nautilus 2>/dev/null | awk '{ print $2 }' || true)
    v=$(echo "$v" | sed 's/^[0-9]*://; s/[^0-9.].*//')
    if [ -n "$v" ] && ! ver_ge "$v" "$MIN_NAUTILUS"; then
        warn "Nautilus $v is too old for the extension (needs $MIN_NAUTILUS+): dialogs only"
        need nautilus
        return
    fi
    ok "Nautilus ${v:-(unknown version)}"
    if nautilus_python_lib >/dev/null; then ok "nautilus-python 4.x"; else
        bad "nautilus-python 4.x not installed"
        need nautilus_python
    fi
}

check_source() {
    if is_checkout "$src"; then
        rev=$(git -C "$src" log -1 --format='%h %s' 2>/dev/null || echo 'not a git checkout')
        ok "Source: $src ($rev)"
    else
        warn "Source: not found, will clone $REPO_URL into $src"
        need source
    fi
}

check_installed() {
    if [ "$os" = macos ]; then
        for d in /Applications "$HOME/Applications"; do
            [ -d "$d/KoopaGit.app" ] && ok "Installed: $d/KoopaGit.app"
        done
        if pluginkit -m -i "$FINDER_EXT_ID" 2>/dev/null | grep -q '^+'; then
            ok "Finder extension enabled"
        fi
    else
        [ -x "$cargo_bin/koopagit-proc" ] && ok "Installed: $("$cargo_bin/koopagit-proc" --version 2>/dev/null) in $cargo_bin"
        [ -f "${XDG_DATA_HOME:-$HOME/.local/share}/nautilus-python/extensions/koopagit_nautilus.py" ] &&
            ok "Nautilus extension installed"
    fi
    return 0
}

run_checks() {
    missing=
    step "Checking prerequisites"
    if [ "$os" = macos ]; then
        check_macos_version
        check_clt
    else
        info "System: ${distro_name:-Linux} (package manager: ${pm:-unknown})"
        check_cc
        check_fontconfig
        check_nautilus
    fi
    check_git
    check_git_identity
    check_rust
    check_curl
    check_memory
    check_source
    check_installed
}

# ---------------------------------------------------------------- fixes

fix_prerequisites() {
    is_missing macos && die "KoopaGit needs macOS $MIN_MACOS or newer"

    if is_missing clt; then
        step "Installing the Xcode command-line tools"
        if confirm "Open the Xcode command-line tools installer?" y; then
            xcode-select --install 2>/dev/null || true
            while ! clt_ok; do
                interactive || die "finish the command-line tools installation, then run this script again"
                pause "Finish the installation in the window that opened."
                clt_ok || confirm "Still not installed. Check again?" y || die "the Xcode command-line tools are required"
            done
            ok "Xcode command-line tools installed"
        else
            die "the Xcode command-line tools are required (xcode-select --install)"
        fi
    fi

    if [ "$os" = linux ] && { is_missing cc || is_missing fontconfig || is_missing git ||
        is_missing curl || is_missing nautilus_python; }; then
        step "Installing system packages"
        if [ -z "$pm" ]; then
            bad "Unknown distribution; install these yourself, then run this script again:"
            info "a C compiler, pkg-config, fontconfig headers, git, curl, nautilus-python 4.x"
            die "missing system packages"
        fi
        info "Packages: $(linux_packages)"
        if confirm "Install them with $pm (uses sudo)?" y; then
            install_linux_packages
        else
            die "the system packages are required"
        fi
    fi

    if is_missing rust; then
        step "Installing Rust"
        info "Rust is installed per user with rustup (https://rustup.rs) into ~/.cargo."
        if confirm "Install Rust with rustup?" y; then
            curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
            . "${CARGO_HOME:-$HOME/.cargo}/env"
        else
            die "Rust $MIN_RUST or newer is required"
        fi
    elif is_missing rust_old; then
        step "Updating Rust"
        if have rustup; then
            if confirm "Update the stable toolchain (rustup update stable)?" y; then
                run rustup update stable
                ver_ge "$(rustc --version | awk '{ print $2 }')" "$MIN_RUST" ||
                    die "rustc is still older than $MIN_RUST; check 'rustup default' or a rust-toolchain override"
            else
                die "Rust $MIN_RUST or newer is required"
            fi
        else
            info "This Rust is not managed by rustup (probably a system package)."
            if confirm "Install a current Rust with rustup alongside it?" y; then
                curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
                . "${CARGO_HOME:-$HOME/.cargo}/env"
            else
                die "Rust $MIN_RUST or newer is required"
            fi
        fi
    fi

    # Re-check the things a package install may have fixed.
    if [ "$os" = linux ]; then
        { have cc || have gcc || have clang; } || die "still no C compiler"
        { have pkg-config && pkg-config --exists fontconfig; } || die "still no fontconfig headers"
        if is_missing nautilus_python && ! nautilus_python_lib >/dev/null; then
            warn "nautilus-python is still missing; the Nautilus extension will not load"
        fi
    fi
    have git || die "git is still not installed"

    # Checked again here: git may have only just been installed.
    if [ -z "$(git config --global user.name)" ] || [ -z "$(git config --global user.email)" ]; then
        step "Git identity"
        info "Like TortoiseGit's first-start wizard: commits need a name and an email."
        if confirm "Set your global git name and email now?" y && interactive; then
            ask "Name" "$(git config --global user.name 2>/dev/null || id -un)"
            git config --global user.name "$answer"
            ask "Email" "$(git config --global user.email 2>/dev/null || true)"
            [ -n "$answer" ] && git config --global user.email "$answer"
            ok "Set git identity"
        fi
    fi

    if is_missing source; then
        step "Getting the source"
        if confirm "Clone $REPO_URL into $src?" y; then
            mkdir -p "$(dirname "$src")"
            run git clone "$REPO_URL" "$src"
        else
            die "no source checkout"
        fi
    elif [ "$src" = "${KOOPAGIT_SRC:-$HOME/.local/src/koopagit}" ] && [ -d "$src/.git" ]; then
        # Only update the checkout this script cloned, never a developer's own.
        if confirm "Update the source in $src (git pull)?" y; then
            run git -C "$src" pull --ff-only
        fi
    fi
}

# ---------------------------------------------------------------- install

choose_options_macos() {
    step "Options"
    if [ -w /Applications ]; then def_apps=/Applications; else def_apps=$HOME/Applications; fi
    ask "Install KoopaGit.app into" "${KOOPAGIT_APPS_DIR:-$def_apps}"
    apps_dir=$answer
    ask "Install the koopagit command into" "${KOOPAGIT_BIN_DIR:-$HOME/.local/bin}"
    bin_dir=$answer
    fix_path=0
    if ! on_path "$bin_dir" && confirm "$bin_dir is not on PATH. Add it to $(shell_rc)?" y; then
        fix_path=1
    fi
}

choose_options_linux() {
    step "Options"
    with_nautilus=0 link_cmd=0 env_d=0 fix_path=0 restart_nautilus=0
    if ! is_missing nautilus; then
        confirm "Install the Nautilus extension (menus + emblems)?" y && with_nautilus=1
    fi
    ask "Install the koopagit command into" "${KOOPAGIT_BIN_DIR:-$HOME/.local/bin}"
    bin_dir=$answer
    confirm "Create the koopagit command ($bin_dir/koopagit)?" y && link_cmd=1
    if [ "$with_nautilus" = 1 ]; then
        info "Desktop sessions often lack ~/.cargo/bin on PATH, so Nautilus can't find"
        info "koopagit-shell. An environment.d entry points it there directly."
        confirm "Write ~/.config/environment.d/koopagit.conf?" y && env_d=1
        if pgrep -x nautilus >/dev/null 2>&1; then
            confirm "Restart Nautilus when done (closes its windows)?" y && restart_nautilus=1
        fi
    fi
    for d in "$cargo_bin" "$bin_dir"; do
        if ! on_path "$d" && confirm "$d is not on PATH. Add it to $(shell_rc)?" y; then
            fix_path="$fix_path $d"
        fi
    done
}

install_macos() {
    mkdir -p "$apps_dir"
    [ -w "$apps_dir" ] || die "$apps_dir is not writable; choose another folder, e.g. ~/Applications"
    step "Building and installing KoopaGit.app (the first build takes a few minutes)"
    KOOPAGIT_BIN_DIR=$bin_dir "$src/integrations/macos/install.sh" "$apps_dir"
    [ "$fix_path" = 1 ] && add_to_path "$bin_dir"
    return 0
}

install_linux() {
    step "Building and installing the binaries into $cargo_bin (the first build takes a few minutes)"
    for crate in koopagit-shell koopagit-proc koopagit-merge; do
        run cargo install --locked --path "$src/crates/$crate"
    done

    if [ "$with_nautilus" = 1 ]; then
        step "Installing the Nautilus extension"
        "$src/integrations/nautilus/install.sh"
    fi
    if [ "$link_cmd" = 1 ]; then
        mkdir -p "$bin_dir"
        ln -sf "$cargo_bin/koopagit-proc" "$bin_dir/koopagit"
        ok "Linked $bin_dir/koopagit -> $cargo_bin/koopagit-proc"
    fi
    if [ "$env_d" = 1 ]; then
        conf=${XDG_CONFIG_HOME:-$HOME/.config}/environment.d/koopagit.conf
        mkdir -p "$(dirname "$conf")"
        echo "KOOPAGIT_SHELL=$cargo_bin/koopagit-shell" >"$conf"
        ok "Wrote $conf (takes effect after you log out and back in)"
    fi
    for d in $fix_path; do
        [ "$d" = 0 ] || add_to_path "$d"
    done
    if [ "$restart_nautilus" = 1 ]; then
        nautilus -q 2>/dev/null || true
        ok "Restarted Nautilus"
    fi
    return 0
}

verify() {
    step "Verifying"
    failed=0
    if [ "$os" = macos ]; then
        proc=$apps_dir/KoopaGit.app/Contents/MacOS/koopagit-proc
        shell=$apps_dir/KoopaGit.app/Contents/MacOS/koopagit-shell
    else
        proc=$cargo_bin/koopagit-proc
        shell=$cargo_bin/koopagit-shell
    fi
    if v=$("$proc" --version 2>/dev/null); then ok "$v ($proc)"; else
        bad "$proc does not run"
        failed=1
    fi
    if "$shell" status "$src" >/dev/null 2>&1; then ok "koopagit-shell reads repository status"; else
        bad "koopagit-shell status failed"
        failed=1
    fi
    if [ -x "$bin_dir/koopagit" ]; then ok "koopagit command: $bin_dir/koopagit"; fi

    if [ "$os" = macos ]; then
        if pluginkit -m -i "$FINDER_EXT_ID" | grep -q '^+'; then ok "Finder extension enabled"; else
            warn "Finder extension not enabled yet: turn on KoopaGit in System Settings →"
            info "General → Login Items & Extensions → Finder."
            if interactive && confirm "Open System Settings there now?" y; then
                open "x-apple.systempreferences:com.apple.ExtensionsPreferences" 2>/dev/null || true
            fi
        fi
    elif [ "$with_nautilus" = 1 ]; then
        data=${XDG_DATA_HOME:-$HOME/.local/share}
        if [ -f "$data/nautilus-python/extensions/koopagit_nautilus.py" ]; then ok "Nautilus extension installed"; else
            bad "Nautilus extension missing"
            failed=1
        fi
        nautilus_python_lib >/dev/null || warn "nautilus-python is not installed, so Nautilus won't load the extension"
    fi
    return "$failed"
}

next_steps() {
    step "Done"
    if [ "$os" = macos ]; then
        info "Right-click a file or folder inside a git repository in Finder."
    elif [ "$with_nautilus" = 1 ]; then
        info "Right-click a file or folder inside a git repository in Nautilus."
        [ "$env_d" = 1 ] && info "Log out and back in once so Nautilus picks up koopagit.conf."
    fi
    info "From a terminal:  koopagit log   |   koopagit commit   |   koopagit help"
    info "Update later by running this script again; remove with --uninstall."
}

# ---------------------------------------------------------------- uninstall

uninstall_macos() {
    found=0
    for d in /Applications "$HOME/Applications"; do
        app=$d/KoopaGit.app
        [ -d "$app" ] || continue
        found=1
        if confirm "Remove $app?" y; then
            pluginkit -e ignore -i "$FINDER_EXT_ID" 2>/dev/null || true
            pluginkit -r "$app/Contents/PlugIns/KoopaGitFinderSync.appex" 2>/dev/null || true
            rm -rf "$app"
            ok "Removed $app"
        fi
    done
    [ "$found" = 1 ] || info "No KoopaGit.app in /Applications or ~/Applications"
    remove_command
    killall Finder 2>/dev/null || true
}

uninstall_linux() {
    if have cargo; then
        for crate in koopagit-shell koopagit-proc koopagit-merge; do
            if cargo install --list | grep -q "^$crate "; then run cargo uninstall "$crate"; fi
        done
    fi
    data=${XDG_DATA_HOME:-$HOME/.local/share}
    for f in "$data/nautilus-python/extensions/koopagit_nautilus.py" \
        "$data"/icons/hicolor/scalable/emblems/emblem-koopagit-*.svg \
        "${XDG_CONFIG_HOME:-$HOME/.config}/environment.d/koopagit.conf"; do
        [ -e "$f" ] && rm -f "$f" && ok "Removed $f"
    done
    gtk-update-icon-cache -q -t "$data/icons/hicolor" 2>/dev/null || true
    remove_command
    if pgrep -x nautilus >/dev/null 2>&1; then nautilus -q 2>/dev/null || true; fi
}

# Remove the koopagit command, but only if it is ours.
remove_command() {
    cmd=${KOOPAGIT_BIN_DIR:-$HOME/.local/bin}/koopagit
    if [ -L "$cmd" ] || { [ -f "$cmd" ] && grep -q koopagit-proc "$cmd" 2>/dev/null; }; then
        rm -f "$cmd"
        ok "Removed $cmd"
    fi
}

uninstall() {
    step "Uninstalling KoopaGit"
    confirm "Remove KoopaGit from this computer?" y || exit 0
    if [ "$os" = macos ]; then uninstall_macos; else uninstall_linux; fi
    settings=${XDG_CONFIG_HOME:-$HOME/.config}/koopagit
    if [ -d "$settings" ] && confirm "Also delete your settings ($settings)?" n; then
        rm -rf "$settings"
        ok "Removed $settings"
    fi
    rc=$(shell_rc)
    if [ -f "$rc" ] && grep -qF "$MARKER" "$rc"; then
        info "PATH lines marked '$MARKER' remain in $rc; remove them if you don't need them."
    fi
    step "KoopaGit removed"
}

# ---------------------------------------------------------------- main

printf '%sKoopaGit installer%s (%s)\n' "$c_bold" "$c_off" "$os"

case $mode in
    check)
        run_checks
        # Nautilus, the git identity and the checkout are optional or fetched later.
        blocking=
        for m in $missing; do
            case $m in nautilus | identity | source) ;; *) blocking="$blocking $m" ;; esac
        done
        if [ -n "$blocking" ]; then
            printf '\nMissing:%s\nRun ./install.sh to install them.\n' "$blocking"
            exit 1
        fi
        printf '\nEverything needed to build and install KoopaGit is present.\n'
        exit 0
        ;;
    uninstall)
        { interactive || [ "$assume_yes" = 1 ]; } || die "no terminal to ask questions on; rerun with --yes"
        uninstall
        exit 0
        ;;
esac

{ interactive || [ "$assume_yes" = 1 ]; } || die "no terminal to ask questions on; rerun with --yes"

run_checks
if [ -n "$missing" ]; then fix_prerequisites; fi
if [ "$os" = macos ]; then choose_options_macos; else choose_options_linux; fi

step "Ready to install"
if [ "$os" = macos ]; then
    info "App:      $apps_dir/KoopaGit.app (with the Finder extension; Finder restarts)"
else
    info "Binaries: $cargo_bin/koopagit-{shell,proc,merge}"
    [ "$with_nautilus" = 1 ] && info "Nautilus: extension + emblems in ${XDG_DATA_HOME:-$HOME/.local/share}"
fi
info "Command:  $bin_dir/koopagit"
confirm "Continue?" y || exit 0

if [ "$os" = macos ]; then install_macos; else install_linux; fi

if verify; then next_steps; else
    die "installation finished with problems (see above); rerun with --check for details"
fi
