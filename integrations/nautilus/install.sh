#!/bin/sh
# Installs the KoopaGit Nautilus extension and emblems for the current user.
# Requires nautilus-python (package: python3-nautilus / nautilus-python) and
# koopagit-shell, koopagit-proc and koopagit-merge on PATH (or KOOPAGIT_SHELL set for Nautilus).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
data=${XDG_DATA_HOME:-"$HOME/.local/share"}
ext_dir="$data/nautilus-python/extensions"
icon_dir="$data/icons/hicolor/scalable/emblems"

mkdir -p "$ext_dir" "$icon_dir"
cp "$here/koopagit_nautilus.py" "$ext_dir/"
cp "$here"/emblems/*.svg "$icon_dir/"
gtk-update-icon-cache -q -t "$data/icons/hicolor" 2>/dev/null || true

if ! command -v koopagit-shell >/dev/null 2>&1; then
    echo "warning: koopagit-shell is not on PATH; install it (cargo install --path crates/koopagit-shell)" >&2
fi
echo "Installed. Restart Nautilus with: nautilus -q"
