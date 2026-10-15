#!/bin/sh
# shellcheck disable=SC2088,SC1091 # "~" in labels is display text; sourced files are external
# KoopaGit setup wizard for macOS and Linux (Windows: install.ps1).
#
# Checks every prerequisite, installs the missing ones, builds KoopaGit from
# source, installs the file-manager integration (Finder Sync on macOS,
# Nautilus on Linux) and verifies the result. Choose the recommended setup to
# accept every default with one key press, or customize each option.
#
#   ./install.sh                 run the wizard (also updates an existing install)
#   ./install.sh --check         only report what is installed and what is missing
#   ./install.sh --uninstall     remove KoopaGit
#   ./install.sh --yes           use the recommended setup without asking
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
CRATES="koopagit-shell koopagit-proc koopagit-merge"

mode=install
assume_yes=0
for arg in "$@"; do
    case $arg in
        -y | --yes) assume_yes=1 ;;
        --check) mode=check ;;
        --uninstall) mode=uninstall ;;
        -h | --help)
            sed -n '3,/^set -eu/s/^# \{0,1\}//p' "$0"
            exit 0
            ;;
        *)
            echo "error: unknown option '$arg' (see --help)" >&2
            exit 2
            ;;
    esac
done

# ---------------------------------------------------------------- terminal

# Prompts read from the terminal, so `curl ... | sh` still works.
tty_ok=0
if (exec </dev/tty) 2>/dev/null; then tty_ok=1; fi

if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
    esc=$(printf '\033')
    c_bold="${esc}[1m" c_dim="${esc}[2m" c_red="${esc}[31m" c_green="${esc}[32m"
    c_yellow="${esc}[33m" c_cyan="${esc}[36m" c_off="${esc}[0m"
else
    esc=$(printf '\033')
    c_bold='' c_dim='' c_red='' c_green='' c_yellow='' c_cyan='' c_off=''
fi

# The full-screen wizard (screens, spinners, arrow-key menus) needs a real
# terminal; otherwise everything is printed line by line.
fancy=0
if [ -t 1 ] && [ "$tty_ok" = 1 ] && [ "${TERM:-dumb}" != dumb ] && [ "$mode" != check ]; then
    fancy=1
fi
cols=
if [ "$fancy" = 1 ]; then cols=$(stty size </dev/tty | awk '{ print $2 }' || true); fi
case $cols in '' | *[!0-9]* | 0) cols=80 ;; esac
bar_w=$((cols - 6))
[ "$bar_w" -gt 64 ] && bar_w=64

stty_saved=
bg_pid=
cleanup() {
    if [ -n "$bg_pid" ]; then kill "$bg_pid" 2>/dev/null || true; fi
    if [ -n "$stty_saved" ]; then stty "$stty_saved" </dev/tty 2>/dev/null || true; fi
    if [ "$fancy" = 1 ]; then printf '%s[?25h' "$esc"; fi
}
trap cleanup EXIT
trap 'exit 130' INT TERM

# ---------------------------------------------------------------- output

ok() { printf '  %s✓%s %s\n' "$c_green" "$c_off" "$*"; }
warn() { printf '  %s!%s %s\n' "$c_yellow" "$c_off" "$*"; }
bad() { printf '  %s✗%s %s\n' "$c_red" "$c_off" "$*"; }
info() { printf '    %s\n' "$*"; }
note() { printf '    %s%s%s\n' "$c_dim" "$*" "$c_off"; }
die() {
    printf '\n  %serror:%s %s\n' "$c_red" "$c_off" "$*" >&2
    if [ -n "${log:-}" ] && [ -s "$log" ]; then printf '  Full log: %s\n' "$log" >&2; fi
    exit 1
}

# Show a command, then run it.
run() {
    printf '    %s$ %s%s\n' "$c_dim" "$*" "$c_off"
    "$@"
}

# repeat <text> <n>
repeat() {
    r='' i=0
    while [ "$i" -lt "$2" ]; do r="$r$1" i=$((i + 1)); done
    printf '%s' "$r"
}

fmt_time() {
    if [ "$1" -ge 60 ]; then printf '%dm %02ds' $(($1 / 60)) $(($1 % 60)); else printf '%ds' "$1"; fi
}

# ~ for paths under the home folder, for display.
tilde() {
    case $1 in
        "$HOME") echo '~' ;;
        "$HOME"/*) echo "~${1#"$HOME"}" ;;
        *) echo "$1" ;;
    esac
}

# The header of each wizard screen: a progress bar and the list of steps.
STEPS="Check Setup Prepare Build Install Finish"
screen() {
    step_count=$(echo "$STEPS" | wc -w | tr -d ' ')
    if [ "$fancy" != 1 ]; then
        printf '\n%s==> [%d/%d] %s%s\n' "$c_bold" "$1" "$step_count" "$2" "$c_off"
        return
    fi
    printf '%s[H%s[2J\n' "$esc" "$esc"
    printf '  %sKoopaGit Setup%s  %s%s%s\n\n' "$c_bold" "$c_off" "$c_dim" "$platform_label" "$c_off"
    filled=$((bar_w * $1 / step_count))
    printf '  %s%s%s%s%s\n' "$c_cyan" "$(repeat ━ "$filled")" "$c_dim" "$(repeat ━ $((bar_w - filled)))" "$c_off"
    line='' i=1
    for name in $STEPS; do
        if [ "$i" -lt "$1" ]; then
            line="$line${c_green}✓ $name$c_off   "
        elif [ "$i" -eq "$1" ]; then
            line="$line$c_bold${c_cyan}● $name$c_off   "
        else
            line="$line${c_dim}○ $name$c_off   "
        fi
        i=$((i + 1))
    done
    printf '  %s\n\n' "$line"
    printf '  %sStep %d of %d · %s%s\n\n' "$c_bold" "$1" "$step_count" "$2" "$c_off"
}

# ---------------------------------------------------------------- prompts

interactive() { [ "$assume_yes" = 0 ] && [ "$tty_ok" = 1 ]; }

# In the recommended setup every yes/no question takes its default.
auto=0

# confirm "Question?" y|n  -> status 0 for yes
confirm() {
    if ! interactive || [ "$auto" = 1 ]; then
        [ "$2" = y ]
        return
    fi
    if [ "$2" = y ]; then hint='[Y/n]'; else hint='[y/N]'; fi
    while :; do
        printf '  %s?%s %s %s ' "$c_cyan" "$c_off" "$1" "$hint"
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
        printf '  %s?%s %s %s[%s]%s ' "$c_cyan" "$c_off" "$1" "$c_dim" "$2" "$c_off"
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
        printf '  %s\n  %sPress Enter to continue...%s ' "$1" "$c_dim" "$c_off"
        IFS= read -r _ </dev/tty || true
    fi
}

# menu <default> "Label|Description"...  -> the chosen item's number in $choice.
# Arrow keys (or j/k, or the item's number) move, Enter picks.
menu() {
    choice=$1
    shift
    interactive || return 0
    count=$#
    pad=0
    for item; do
        l=${item%%|*}
        [ ${#l} -gt "$pad" ] && pad=${#l}
    done
    if [ "$fancy" != 1 ]; then
        i=1
        for item; do
            printf '    %d) %-*s  %s\n' "$i" "$pad" "${item%%|*}" "${item#*|}"
            i=$((i + 1))
        done
        while :; do
            printf '  %s?%s Choice %s[%d]%s ' "$c_cyan" "$c_off" "$c_dim" "$choice" "$c_off"
            IFS= read -r reply </dev/tty || reply=
            case $reply in
                '') return 0 ;;
                *[!0-9]*) ;;
                *) if [ "$reply" -ge 1 ] && [ "$reply" -le "$count" ]; then
                    choice=$reply
                    return 0
                fi ;;
            esac
        done
    fi
    printf '%s[?25l' "$esc"
    stty_saved=$(stty -g </dev/tty)
    stty -icanon -echo min 1 time 0 </dev/tty
    draw_menu "$@"
    while :; do
        key=$(dd bs=1 count=1 2>/dev/null </dev/tty)
        case $key in
            "$esc")
                stty min 0 time 1 </dev/tty
                seq=$(dd bs=1 count=2 2>/dev/null </dev/tty)
                stty min 1 time 0 </dev/tty
                case $seq in
                    *A) menu_move -1 ;;
                    *B) menu_move 1 ;;
                esac
                ;;
            k) menu_move -1 ;;
            j) menu_move 1 ;;
            [1-9]) if [ "$key" -le "$count" ]; then choice=$key; fi ;;
            '') break ;;
        esac
        printf '%s[%dA' "$esc" "$count"
        draw_menu "$@"
    done
    stty "$stty_saved" </dev/tty
    stty_saved=
    printf '%s[?25h' "$esc"
}

menu_move() {
    choice=$((choice + $1))
    [ "$choice" -lt 1 ] && choice=$count
    [ "$choice" -gt "$count" ] && choice=1
    return 0
}

draw_menu() {
    i=1
    for item; do
        l=${item%%|*} d=${item#*|}
        if [ "$i" = "$choice" ]; then
            printf '  %s❯ %s%-*s%s  %s%s\033[K\n' "$c_cyan" "$c_bold" "$pad" "$l" "$c_off" "$d" "$c_off"
        else
            printf '    %-*s  %s%s%s\033[K\n' "$pad" "$l" "$c_dim" "$d" "$c_off"
        fi
        i=$((i + 1))
    done
}

# ---------------------------------------------------------------- tasks

# task "Label" command...  runs the command behind a spinner, its output going
# to the log; on failure the end of the log is shown.
task() {
    label=$1
    shift
    printf '\n$ %s\n' "$*" >>"$log"
    if [ "$fancy" != 1 ]; then
        info "$label..."
        if "$@"; then ok "$label"; else die "$label failed"; fi
        return 0
    fi
    start=$(date +%s)
    "$@" >>"$log" 2>&1 </dev/null &
    bg_pid=$!
    n=0
    printf '%s[?25l' "$esc"
    while kill -0 "$bg_pid" 2>/dev/null; do
        printf '\r  %s%s%s %s %s%s%s\033[K' "$c_cyan" "$(spinner "$n")" "$c_off" "$label" \
            "$c_dim" "$(fmt_time $(($(date +%s) - start)))" "$c_off"
        n=$((n + 1))
        sleep 0.1
    done
    task_finish "$label" "$start"
}

# Collects the background job's status and prints the ✓ or ✗ line.
task_finish() {
    if wait "$bg_pid"; then status=0; else status=$?; fi
    bg_pid=
    printf '%s[?25h' "$esc"
    took=$(fmt_time $(($(date +%s) - $2)))
    if [ "$status" = 0 ]; then
        printf '\r  %s✓%s %s %s%s%s\033[K\n' "$c_green" "$c_off" "$1" "$c_dim" "$took" "$c_off"
    else
        printf '\r  %s✗%s %s\033[K\n\n' "$c_red" "$c_off" "$1"
        printf '  %sEnd of the log:%s\n' "$c_dim" "$c_off"
        tail -n 25 "$log" | sed 's/^/    /'
        die "$1 failed"
    fi
}

spinner() {
    # shellcheck disable=SC2046 # one frame per word
    set -- $(($1 % 10 + 1)) ⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏
    shift "$1"
    printf '%s' "$1"
}

# build "Label" command...  like task, with a progress bar fed by cargo's
# "Compiling" lines against the crates `cargo tree` expects. The hundreds of
# small dependencies fill the first 60%; KoopaGit's own crates, which take most
# of the time (Slint's generated code), share the last 40%.
build() {
    label=$1
    shift
    if [ "$fancy" != 1 ]; then
        task "$label" "$@"
        return 0
    fi
    pkgs=
    for c in $CRATES; do pkgs="$pkgs -p $c"; done
    # shellcheck disable=SC2086 # $pkgs is a list of arguments
    tree=$(cargo tree --locked -e normal,build --prefix none --manifest-path "$src/Cargo.toml" $pkgs 2>/dev/null |
        sed 's/ (\*)$//; s/ (proc-macro)$//' | sort -u)
    ws_total=$(printf '%s\n' "$tree" | grep -cF "($src/" || true)
    dep_total=$(($(printf '%s\n' "$tree" | grep -c . || true) - ws_total))
    blog=$log.build
    : >"$blog"
    start=$(date +%s)
    "$@" >"$blog" 2>&1 </dev/null &
    bg_pid=$!
    printf '  %s\n' "$label"
    printf '%s[?25l' "$esc"
    w=$((bar_w - 32))
    [ "$w" -lt 10 ] && w=10
    n=0
    while kill -0 "$bg_pid" 2>/dev/null; do
        if [ $((n % 4)) = 0 ]; then
            built=$(grep -c '^ *Compiling ' "$blog" || true)
            ws_built=$(grep '^ *Compiling ' "$blog" | grep -cF "($src/" || true)
            crate=$(grep '^ *Compiling ' "$blog" | tail -n 1 | awk '{ print $2 }')
            pct=0
            if [ "$dep_total" -gt 0 ]; then pct=$(((built - ws_built) * 60 / dep_total)); fi
            [ "$pct" -gt 60 ] && pct=60
            if [ "$ws_total" -gt 0 ]; then pct=$((pct + ws_built * 40 / ws_total)); fi
            [ "$pct" -gt 99 ] && pct=99
            filled=$((w * pct / 100))
        fi
        printf '\r  %s%s %s%s%s%s %3d%%  %s%-18.18s %s%s\033[K' "$c_cyan" "$(spinner "$n")" "$(repeat █ "$filled")" \
            "$c_dim" "$(repeat ░ $((w - filled)))" "$c_off" "$pct" "$c_dim" "$crate" \
            "$(fmt_time $(($(date +%s) - start)))" "$c_off"
        n=$((n + 1))
        sleep 0.125
    done
    cat "$blog" >>"$log"
    rm -f "$blog"
    printf '\r  %s  %s%s 100%%\033[K\n' "$c_cyan" "$(repeat █ "$w")" "$c_off"
    task_finish "$label" "$start"
}

# ---------------------------------------------------------------- helpers

have() { command -v "$1" >/dev/null 2>&1; }

# ver_ge 1.92.1 1.92  -> status 0 when the first version is >= the second
ver_ge() {
    awk -v a="$1" -v b="$2" 'BEGIN {
        split(a, x, "."); split(b, y, ".")
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
    ok "Added $(tilde "$1") to PATH in $(tilde "$rc") (new terminals pick it up)"
}

# Make an existing rustup install visible to this script.
if ! have cargo && [ -f "${CARGO_HOME:-$HOME/.cargo}/env" ]; then
    . "${CARGO_HOME:-$HOME/.cargo}/env"
fi
cargo_bin=${CARGO_HOME:-$HOME/.cargo}/bin

# ---------------------------------------------------------------- platform

case $(uname -s) in
    Darwin)
        os=macos
        platform_label="macOS $(sw_vers -productVersion)"
        log_dir=$HOME/Library/Logs/KoopaGit
        ;;
    Linux)
        os=linux
        platform_label=Linux
        log_dir=${XDG_STATE_HOME:-$HOME/.local/state}/koopagit
        ;;
    MINGW* | MSYS* | CYGWIN*) die "on Windows, run install.ps1 in PowerShell instead" ;;
    *) die "unsupported system: $(uname -s)" ;;
esac
log=$log_dir/install.log

distro='' pm=''
if [ "$os" = linux ] && [ -r /etc/os-release ]; then
    distro=$(. /etc/os-release && echo "${ID:-} ${ID_LIKE:-}")
    platform_label=$(. /etc/os-release && echo "${PRETTY_NAME:-${NAME:-Linux}}")
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
default_src=${KOOPAGIT_SRC:-$HOME/.local/src/koopagit}
here=$(cd "$(dirname "$0")" 2>/dev/null && pwd || pwd)
if is_checkout "$here"; then
    src=$here
elif is_checkout "$(pwd)"; then
    src=$(pwd)
else
    src=$default_src
fi

# ---------------------------------------------------------------- checks

# Each check prints one line and records what is missing in $missing.
missing=
passed=0
need() { missing="$missing $1"; }
is_missing() { case " $missing " in *" $1 "*) return 0 ;; esac; return 1; }
pass() {
    ok "$@"
    passed=$((passed + 1))
}

check_macos_version() {
    v=$(sw_vers -productVersion)
    if ver_ge "$v" "$MIN_MACOS"; then pass "macOS $v"; else
        bad "macOS $v (KoopaGit needs macOS $MIN_MACOS or newer)"
        need macos
    fi
}

clt_ok() { xcode-select -p >/dev/null 2>&1 && xcrun --find swiftc >/dev/null 2>&1; }

check_clt() {
    if clt_ok; then pass "Xcode command-line tools"; else
        bad "Xcode command-line tools (Swift compiler, codesign) not installed"
        need clt
    fi
}

check_git() {
    # On macOS /usr/bin/git is a stub until the command-line tools exist.
    if have git && { [ "$os" = linux ] || [ "$(command -v git)" != /usr/bin/git ] || clt_ok; }; then
        pass "$(git --version)"
    else
        bad "git not installed"
        need git
    fi
}

check_git_identity() {
    is_missing git && return
    name=$(git config --global user.name 2>/dev/null || true)
    email=$(git config --global user.email 2>/dev/null || true)
    if [ -n "$name" ] && [ -n "$email" ]; then pass "git identity: $name <$email>"; else
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
    if ver_ge "$v" "$MIN_RUST"; then pass "Rust $v"; else
        bad "Rust $v is too old (needs $MIN_RUST or newer)"
        need rust_old
    fi
}

check_curl() {
    if is_missing rust && ! have curl; then
        bad "curl not installed (needed to install Rust)"
        need curl
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
        pass "${mem_gib} GiB RAM, $cpus CPUs (CARGO_BUILD_JOBS=$CARGO_BUILD_JOBS)"
        return
    fi
    jobs=$((mem_gib / 3))
    [ "$jobs" -lt 1 ] && jobs=1
    if [ "$jobs" -lt "$cpus" ]; then
        export CARGO_BUILD_JOBS=$jobs
        pass "${mem_gib} GiB RAM, $cpus CPUs: building with $jobs parallel jobs to avoid running out of memory"
    else
        pass "${mem_gib} GiB RAM, $cpus CPUs"
    fi
    [ "$mem_gib" -lt 4 ] && warn "Less than 4 GiB RAM: the build may still run out of memory"
    return 0
}

check_cc() {
    if have cc || have gcc || have clang; then pass "C compiler"; else
        bad "C compiler not installed"
        need cc
    fi
}

check_fontconfig() {
    if have pkg-config && pkg-config --exists fontconfig; then
        pass "pkg-config + fontconfig headers"
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
    pass "Nautilus ${v:-(unknown version)}"
    if nautilus_python_lib >/dev/null; then pass "nautilus-python 4.x"; else
        bad "nautilus-python 4.x not installed"
        need nautilus_python
    fi
}

check_source() {
    if is_checkout "$src"; then
        rev=$(git -C "$src" log -1 --format='%h %s' 2>/dev/null || echo 'not a git checkout')
        pass "Source: $(tilde "$src") ($rev)"
    else
        warn "Source: not downloaded yet ($(tilde "$src"))"
        need source
    fi
}

installed_app=
check_installed() {
    upd=
    [ "$mode" = install ] && upd=' (will be updated)'
    if [ "$os" = macos ]; then
        for d in /Applications "$HOME/Applications"; do
            if [ -d "$d/KoopaGit.app" ]; then
                installed_app=$d
                ok "Installed: $(tilde "$d")/KoopaGit.app$upd"
            fi
        done
    elif [ -x "$cargo_bin/koopagit-proc" ]; then
        ok "Installed: $("$cargo_bin/koopagit-proc" --version 2>/dev/null) in $(tilde "$cargo_bin")$upd"
    fi
    return 0
}

run_checks() {
    missing='' passed=0
    if [ "$os" = macos ]; then
        check_macos_version
        check_clt
    else
        info "Package manager: ${pm:-unknown}"
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

# ---------------------------------------------------------------- setup

# The recommended setup; Customize changes these.
recommend() {
    bin_dir=${KOOPAGIT_BIN_DIR:-$HOME/.local/bin}
    fix_path=
    if [ "$os" = macos ]; then
        if [ -n "${KOOPAGIT_APPS_DIR:-}" ]; then
            apps_dir=$KOOPAGIT_APPS_DIR
        elif [ -n "$installed_app" ]; then
            apps_dir=$installed_app
        elif [ -w /Applications ]; then
            apps_dir=/Applications
        else
            apps_dir=$HOME/Applications
        fi
        on_path "$bin_dir" || fix_path=$bin_dir
    else
        with_nautilus=0 link_cmd=1 env_d=0 restart_nautilus=0
        if ! is_missing nautilus; then
            with_nautilus=1 env_d=1
            pgrep -x nautilus >/dev/null 2>&1 && restart_nautilus=1
        fi
        for d in "$cargo_bin" "$bin_dir"; do
            on_path "$d" || fix_path="$fix_path $d"
        done
    fi
}

# What the prepare step will install, in words.
prereq_plan() {
    p=
    is_missing clt && p="$p, Xcode command-line tools"
    if [ "$os" = linux ] && { is_missing cc || is_missing fontconfig || is_missing git ||
        is_missing curl || is_missing nautilus_python; }; then
        p="$p, system packages (${pm:-manual})"
    fi
    is_missing rust && p="$p, Rust (rustup)"
    is_missing rust_old && p="$p, a newer Rust (rustup)"
    is_missing source && p="$p, KoopaGit's source"
    echo "${p#, }"
}

plan_row() { printf '    %s%-14s%s %s\n' "$c_dim" "$1" "$c_off" "$2"; }

print_plan() {
    pre=$(prereq_plan)
    plan_row Prerequisites "${pre:-all present}"
    if [ "$os" = macos ]; then
        plan_row 'Install to' "$(tilde "$apps_dir")/KoopaGit.app"
        plan_row Command "$(tilde "$bin_dir")/koopagit"
        plan_row Finder 'enable the KoopaGit extension (Finder restarts)'
    else
        plan_row 'Install to' "$(tilde "$cargo_bin")  (koopagit-shell, koopagit-proc, koopagit-merge)"
        if [ "$link_cmd" = 1 ]; then plan_row Command "$(tilde "$bin_dir")/koopagit"; else plan_row Command 'none'; fi
        if [ "$with_nautilus" = 1 ]; then
            n='menus and emblems'
            [ "$restart_nautilus" = 1 ] && n="$n, restart Nautilus"
            plan_row Nautilus "$n"
        else
            plan_row Nautilus 'not installed'
        fi
    fi
    p=
    for d in $fix_path; do p="$p, $(tilde "$d")"; done
    if [ -n "$p" ]; then plan_row PATH "add ${p#, } in $(tilde "$(shell_rc)")"; fi
    if is_checkout "$src"; then plan_row Source "$(tilde "$src")"; else plan_row Source "download to $(tilde "$src")"; fi
    return 0
}

# location_menu "Title" "What it is" default-dir "dir|label"...  -> $answer
location_menu() {
    title=$1 what=$2 def=$3
    shift 3
    printf '\n  %s%s%s\n' "$c_bold" "$title" "$c_off"
    note "$what"
    n=1 sel=1
    for item; do
        [ "${item%%|*}" = "$def" ] && sel=$n
        n=$((n + 1))
    done
    menu "$sel" "$@" "Other folder...|type a path"
    if [ "$choice" -eq "$n" ]; then
        ask "Folder" "$(tilde "$def")"
    else
        eval "answer=\${$choice}"
        answer=${answer%%|*}
        case $answer in '~'/*) answer=$HOME/${answer#'~/'} ;; esac
    fi
}

customize() {
    screen 2 "Customize"
    if [ "$os" = macos ]; then
        location_menu "Install location" "The folder KoopaGit.app is installed into." "$(tilde "$apps_dir")" \
            "/Applications|for everyone on this Mac" "~/Applications|only for you"
        apps_dir=$answer
        location_menu "Command location" "The folder for the koopagit terminal command (a small launcher script)." \
            "$(tilde "$bin_dir")" "~/.local/bin|recommended" "/usr/local/bin|needs to be writable"
        bin_dir=$answer
        fix_path=
        if ! on_path "$bin_dir"; then
            printf '\n  %sPATH%s\n' "$c_bold" "$c_off"
            note "$(tilde "$bin_dir") is not on your PATH, so the koopagit command won't be found."
            menu 1 "Add it|in $(tilde "$(shell_rc)")" "Leave PATH alone|"
            [ "$choice" = 1 ] && fix_path=$bin_dir
        fi
    else
        printf '\n  %sInstall location%s\n' "$c_bold" "$c_off"
        note "cargo installs the programs into $(tilde "$cargo_bin") (set CARGO_HOME to change it)."
        if ! is_missing nautilus; then
            printf '\n  %sNautilus extension%s\n' "$c_bold" "$c_off"
            note "Adds the KoopaGit menu and status emblems to Nautilus (GNOME Files)."
            menu 1 "Install it|recommended" "Skip it|dialogs and the terminal command only"
            if [ "$choice" = 1 ]; then with_nautilus=1; else with_nautilus=0 env_d=0 restart_nautilus=0; fi
        fi
        location_menu "Command location" "The folder for the koopagit terminal command (a link to koopagit-proc)." \
            "$(tilde "$bin_dir")" "~/.local/bin|recommended" "/usr/local/bin|needs to be writable" \
            "none|don't create the command"
        if [ "$answer" = none ]; then link_cmd=0; else link_cmd=1 bin_dir=$answer; fi
        if [ "$with_nautilus" = 1 ]; then
            printf '\n  %sDesktop session%s\n' "$c_bold" "$c_off"
            note "Desktop sessions often lack ~/.cargo/bin on PATH, so Nautilus can't find"
            note "koopagit-shell. An environment.d entry points it there directly."
            menu 1 "Write ~/.config/environment.d/koopagit.conf|recommended" "Skip|"
            if [ "$choice" = 1 ]; then env_d=1; else env_d=0; fi
            if pgrep -x nautilus >/dev/null 2>&1; then
                menu 1 "Restart Nautilus when done|closes its windows" "Don't restart|"
                if [ "$choice" = 1 ]; then restart_nautilus=1; else restart_nautilus=0; fi
            fi
        fi
        fix_path=
        for d in "$cargo_bin" "$bin_dir"; do
            [ "$d" = "$bin_dir" ] && [ "$link_cmd" = 0 ] && continue
            on_path "$d" && continue
            printf '\n  %sPATH%s\n' "$c_bold" "$c_off"
            note "$(tilde "$d") is not on your PATH."
            menu 1 "Add it|in $(tilde "$(shell_rc)")" "Leave PATH alone|"
            [ "$choice" = 1 ] && fix_path="$fix_path $d"
        done
    fi
    return 0
}

ask_identity() {
    if [ -n "$(git config --global user.name 2>/dev/null)" ] &&
        [ -n "$(git config --global user.email 2>/dev/null)" ]; then
        return 0
    fi
    interactive || return 0
    printf '\n  %sYour git identity%s\n' "$c_bold" "$c_off"
    note "Like TortoiseGit's first-start wizard: commits need a name and an email."
    note "Leave empty to skip."
    ask "Name" "$(git config --global user.name 2>/dev/null || true)"
    [ -n "$answer" ] && git config --global user.name "$answer"
    ask "Email" "$(git config --global user.email 2>/dev/null || true)"
    [ -n "$answer" ] && git config --global user.email "$answer"
    return 0
}

# ---------------------------------------------------------------- prepare

install_rustup() {
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
}

prepare() {
    is_missing macos && die "KoopaGit needs macOS $MIN_MACOS or newer"
    if [ -z "$(prereq_plan)" ]; then
        ok "Everything needed is already installed"
        return 0
    fi

    if is_missing clt; then
        confirm "Install the Xcode command-line tools?" y ||
            die "the Xcode command-line tools are required (xcode-select --install)"
        xcode-select --install 2>/dev/null || true
        while ! clt_ok; do
            interactive || die "finish the command-line tools installation, then run this script again"
            pause "Finish the installation in the window that opened."
            clt_ok || confirm "Still not installed. Check again?" y || die "the Xcode command-line tools are required"
        done
        ok "Xcode command-line tools"
    fi

    if [ "$os" = linux ] && { is_missing cc || is_missing fontconfig || is_missing git ||
        is_missing curl || is_missing nautilus_python; }; then
        if [ -z "$pm" ]; then
            bad "Unknown distribution; install these yourself, then run this script again:"
            info "a C compiler, pkg-config, fontconfig headers, git, curl, nautilus-python 4.x"
            die "missing system packages"
        fi
        note "Packages: $(linux_packages)"
        confirm "Install them with $pm (uses sudo)?" y || die "the system packages are required"
        if [ "$(id -u)" != 0 ]; then
            note "sudo may ask for your password."
            sudo -v || die "sudo failed"
        fi
        task "Installing system packages ($pm)" install_linux_packages
        { have cc || have gcc || have clang; } || die "still no C compiler"
        { have pkg-config && pkg-config --exists fontconfig; } || die "still no fontconfig headers"
        if is_missing nautilus_python && ! nautilus_python_lib >/dev/null; then
            warn "nautilus-python is still missing; the Nautilus extension will not load"
        fi
    fi
    have git || die "git is still not installed"

    if is_missing rust || { is_missing rust_old && ! have rustup; }; then
        note "Rust is installed per user with rustup (https://rustup.rs) into ~/.cargo."
        confirm "Install Rust with rustup?" y || die "Rust $MIN_RUST or newer is required"
        task "Installing Rust (rustup)" install_rustup
        . "${CARGO_HOME:-$HOME/.cargo}/env"
    elif is_missing rust_old; then
        confirm "Update the stable toolchain (rustup update stable)?" y || die "Rust $MIN_RUST or newer is required"
        task "Updating Rust" rustup update stable
    fi
    ver_ge "$(rustc --version | awk '{ print $2 }')" "$MIN_RUST" ||
        die "rustc is still older than $MIN_RUST; check 'rustup default' or a rust-toolchain override"

    if is_missing source; then
        confirm "Download KoopaGit's source into $(tilde "$src")?" y || die "no source checkout"
        mkdir -p "$(dirname "$src")"
        task "Downloading the source" git clone "$REPO_URL" "$src"
    fi
}

# Only update the checkout this script cloned, never a developer's own.
update_source() {
    if [ "$src" = "$default_src" ] && [ -d "$src/.git" ] && ! is_missing source; then
        task "Updating the source (git pull)" git -C "$src" pull --ff-only
    fi
    return 0
}

# ---------------------------------------------------------------- install

cargo_build() {
    # shellcheck disable=SC2086 # $pkgs is a list of arguments
    cargo build --release --locked --manifest-path "$src/Cargo.toml" $pkgs
}

cargo_install_all() {
    for c in $CRATES; do
        cargo install --locked --path "$src/crates/$c" || return 1
    done
}

install_macos() {
    mkdir -p "$apps_dir"
    [ -w "$apps_dir" ] || die "$apps_dir is not writable; choose another install location, e.g. ~/Applications"
    task "Building KoopaGit.app and registering the Finder extension" \
        env KOOPAGIT_BIN_DIR="$bin_dir" "$src/integrations/macos/install.sh" "$apps_dir"
    for d in $fix_path; do add_to_path "$d"; done
    return 0
}

install_linux() {
    if [ "$with_nautilus" = 1 ]; then
        task "Installing the Nautilus extension and emblems" "$src/integrations/nautilus/install.sh"
    fi
    if [ "$link_cmd" = 1 ]; then
        mkdir -p "$bin_dir"
        ln -sf "$cargo_bin/koopagit-proc" "$bin_dir/koopagit"
        ok "Linked $(tilde "$bin_dir")/koopagit"
    fi
    if [ "$env_d" = 1 ]; then
        conf=${XDG_CONFIG_HOME:-$HOME/.config}/environment.d/koopagit.conf
        mkdir -p "$(dirname "$conf")"
        echo "KOOPAGIT_SHELL=$cargo_bin/koopagit-shell" >"$conf"
        ok "Wrote $(tilde "$conf")"
    fi
    for d in $fix_path; do add_to_path "$d"; done
    if [ "$restart_nautilus" = 1 ]; then
        nautilus -q 2>/dev/null || true
        ok "Restarted Nautilus"
    fi
    return 0
}

verify() {
    failed=0
    if [ "$os" = macos ]; then
        proc=$apps_dir/KoopaGit.app/Contents/MacOS/koopagit-proc
        shell=$apps_dir/KoopaGit.app/Contents/MacOS/koopagit-shell
    else
        proc=$cargo_bin/koopagit-proc
        shell=$cargo_bin/koopagit-shell
    fi
    if v=$("$proc" --version 2>/dev/null); then ok "$v runs"; else
        bad "$proc does not run"
        failed=1
    fi
    if "$shell" status "$src" >/dev/null 2>&1; then ok "koopagit-shell reads repository status"; else
        bad "koopagit-shell status failed"
        failed=1
    fi
    if [ -x "$bin_dir/koopagit" ]; then ok "koopagit command: $(tilde "$bin_dir")/koopagit"; fi

    if [ "$os" = macos ]; then
        if pluginkit -m -i "$FINDER_EXT_ID" | grep -q '^+'; then ok "Finder extension enabled"; else
            warn "Finder extension not enabled yet: turn on KoopaGit in System Settings →"
            info "General → Login Items & Extensions → Finder."
            if interactive; then
                menu 1 "Open System Settings|to turn the extension on" "Skip|"
                if [ "$choice" = 1 ]; then
                    open "x-apple.systempreferences:com.apple.ExtensionsPreferences" 2>/dev/null || true
                fi
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
    printf '\n  %s%sKoopaGit is installed.%s\n\n' "$c_bold" "$c_green" "$c_off"
    if [ "$os" = macos ]; then
        info "Right-click a file or folder inside a git repository in Finder."
    elif [ "$with_nautilus" = 1 ]; then
        info "Right-click a file or folder inside a git repository in Nautilus."
        [ "$env_d" = 1 ] && info "Log out and back in once so Nautilus picks up koopagit.conf."
    fi
    [ -n "$fix_path" ] && info "Open a new terminal to pick up the PATH change."
    info "From a terminal:  koopagit log   |   koopagit commit   |   koopagit help"
    info "Update by running this installer again; remove with --uninstall."
    note "Log: $(tilde "$log")"
    printf '\n'
}

# ---------------------------------------------------------------- uninstall

uninstall_macos() {
    for d in /Applications "$HOME/Applications"; do
        app=$d/KoopaGit.app
        [ -d "$app" ] || continue
        pluginkit -e ignore -i "$FINDER_EXT_ID" 2>/dev/null || true
        pluginkit -r "$app/Contents/PlugIns/KoopaGitFinderSync.appex" 2>/dev/null || true
        rm -rf "$app"
        ok "Removed $(tilde "$app")"
    done
    remove_command
    killall Finder 2>/dev/null || true
    return 0
}

uninstall_linux() {
    if have cargo; then
        for c in $CRATES; do
            if cargo install --list | grep -q "^$c "; then
                cargo uninstall "$c" >>"$log" 2>&1
                ok "Removed $c"
            fi
        done
    fi
    data=${XDG_DATA_HOME:-$HOME/.local/share}
    for f in "$data/nautilus-python/extensions/koopagit_nautilus.py" \
        "${XDG_CONFIG_HOME:-$HOME/.config}/environment.d/koopagit.conf"; do
        if [ -e "$f" ]; then
            rm -f "$f"
            ok "Removed $(tilde "$f")"
        fi
    done
    if ls "$data"/icons/hicolor/scalable/emblems/emblem-koopagit-*.svg >/dev/null 2>&1; then
        rm -f "$data"/icons/hicolor/scalable/emblems/emblem-koopagit-*.svg
        ok "Removed the emblems"
    fi
    gtk-update-icon-cache -q -t "$data/icons/hicolor" 2>/dev/null || true
    remove_command
    if pgrep -x nautilus >/dev/null 2>&1; then nautilus -q 2>/dev/null || true; fi
    return 0
}

# Remove the koopagit command, but only if it is ours.
remove_command() {
    cmd=${KOOPAGIT_BIN_DIR:-$HOME/.local/bin}/koopagit
    if [ -L "$cmd" ] || { [ -f "$cmd" ] && grep -q koopagit-proc "$cmd" 2>/dev/null; }; then
        rm -f "$cmd"
        ok "Removed $(tilde "$cmd")"
    fi
    return 0
}

uninstall() {
    STEPS="Confirm Remove Finish"
    screen 1 "Uninstall KoopaGit"
    found=0
    if [ "$os" = macos ]; then
        for d in /Applications "$HOME/Applications"; do
            [ -d "$d/KoopaGit.app" ] && info "$(tilde "$d")/KoopaGit.app and its Finder extension" && found=1
        done
    else
        [ -x "$cargo_bin/koopagit-proc" ] && info "$(tilde "$cargo_bin")/koopagit-{shell,proc,merge}" && found=1
        [ -f "${XDG_DATA_HOME:-$HOME/.local/share}/nautilus-python/extensions/koopagit_nautilus.py" ] &&
            info "The Nautilus extension and emblems" && found=1
    fi
    [ -e "${KOOPAGIT_BIN_DIR:-$HOME/.local/bin}/koopagit" ] && info "The koopagit command" && found=1
    if [ "$found" = 0 ]; then
        info "KoopaGit is not installed."
        return 0
    fi
    printf '\n'
    menu 1 "Remove KoopaGit|" "Cancel|"
    [ "$choice" = 1 ] || return 0
    settings=${XDG_CONFIG_HOME:-$HOME/.config}/koopagit
    drop_settings=0
    if [ -d "$settings" ]; then
        printf '\n  %sYour settings%s  %s%s%s\n' "$c_bold" "$c_off" "$c_dim" "$(tilde "$settings")" "$c_off"
        menu 1 "Keep them|in case you reinstall" "Delete them|"
        [ "$choice" = 2 ] && drop_settings=1
    fi

    screen 2 "Removing KoopaGit"
    if [ "$os" = macos ]; then uninstall_macos; else uninstall_linux; fi
    if [ "$drop_settings" = 1 ]; then
        rm -rf "$settings"
        ok "Removed $(tilde "$settings")"
    fi

    screen 3 "Done"
    printf '  %s%sKoopaGit is removed.%s\n\n' "$c_bold" "$c_green" "$c_off"
    rc=$(shell_rc)
    if [ -f "$rc" ] && grep -qF "$MARKER" "$rc"; then
        info "PATH lines marked '$MARKER' remain in $(tilde "$rc");"
        info "remove them if you don't need them."
    fi
    printf '\n'
}

# ---------------------------------------------------------------- main

if [ "$mode" = check ]; then
    printf '%sKoopaGit setup%s (%s): checking\n\n' "$c_bold" "$c_off" "$platform_label"
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
fi

{ interactive || [ "$assume_yes" = 1 ]; } || die "no terminal to ask questions on; rerun with --yes"
mkdir -p "$log_dir"
: >"$log"

if [ "$mode" = uninstall ]; then
    uninstall
    exit 0
fi

# 1. Check
screen 1 "Checking your system"
run_checks
recommend

# 2. Setup
screen 2 "Choose a setup"
to_install=$(prereq_plan)
if [ -n "$to_install" ]; then
    warn "$passed checks passed; still needed: $to_install"
else
    ok "$passed checks passed"
fi
is_missing nautilus && warn "No usable Nautilus: the file-manager integration is skipped"
printf '\n  %sRecommended setup%s\n' "$c_bold" "$c_off"
print_plan
printf '\n'
menu 1 "Install|use the recommended setup above" "Customize...|choose the install location and each option" "Cancel|"
case $choice in
    1) auto=1 ;;
    2)
        customize
        screen 2 "Review"
        print_plan
        printf '\n'
        menu 1 "Install|with these settings" "Cancel|"
        [ "$choice" = 1 ] || exit 0
        ;;
    *) exit 0 ;;
esac
ask_identity

# 3. Prepare
screen 3 "Preparing"
prepare
update_source

# 4. Build
screen 4 "Building KoopaGit"
note "The first build takes a few minutes; later updates are much faster."
printf '\n'
pkgs=
for c in $CRATES; do pkgs="$pkgs -p $c"; done
if [ "$os" = macos ]; then
    build "Compiling" cargo_build
else
    build "Compiling and installing into $(tilde "$cargo_bin")" cargo_install_all
fi

# 5. Install
screen 5 "Installing"
if [ "$os" = macos ]; then install_macos; else install_linux; fi

# 6. Finish
screen 6 "Checking the installation"
if verify; then next_steps; else
    die "the installation finished with problems (see above); rerun with --check for details"
fi
