#!/bin/sh
# Builds KoopaGit.app into an applications folder, registers and enables the
# Finder Sync extension, restarts Finder, and installs a `koopagit` command
# (koopagit /command:log /path:.) into $KOOPAGIT_BIN_DIR.
#
#   integrations/macos/install.sh [apps-dir]      (default: /Applications)
#   KOOPAGIT_BIN_DIR                              (default: ~/.local/bin)
#
# Re-run to update. Watched folders: ~/.config/koopagit/finder-roots (see
# README.md); the home folder is watched when that file is missing.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
dest=${1:-/Applications}
app="$dest/KoopaGit.app"
appex="$app/Contents/PlugIns/KoopaGitFinderSync.appex"
bin_dir=${KOOPAGIT_BIN_DIR:-"$HOME/.local/bin"}
ext_id=dev.koopagit.FinderSync
lsregister=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister

if [ ! -w "$dest" ]; then
    echo "error: $dest is not writable; pass another folder, e.g. $0 ~/Applications" >&2
    exit 1
fi

"$here/build.sh" "$dest"

# Drop registrations of other copies (e.g. an earlier target/macos build) so
# Finder loads this one.
pluginkit -mAv -i "$ext_id" | awk -F'\t' '$NF ~ /\.appex$/ { print $NF }' |
    while IFS= read -r old; do
        if [ "$old" != "$appex" ]; then
            echo "Unregistering $old"
            pluginkit -r "$old" || true
        fi
    done

# Register without launching the host app (launching it shows a help alert).
"$lsregister" -f "$app"
pluginkit -a "$appex"
pluginkit -e use -i "$ext_id"

killall Finder 2>/dev/null || true

# A wrapper, not a symlink: koopagit-proc finds itself and its siblings via
# current_exe, which a symlink would point outside the bundle.
mkdir -p "$bin_dir"
cat > "$bin_dir/koopagit" <<EOF
#!/bin/sh
exec "$app/Contents/MacOS/koopagit-proc" "\$@"
EOF
chmod +x "$bin_dir/koopagit"
echo "Installed $bin_dir/koopagit"
case ":$PATH:" in
    *":$bin_dir:"*) ;;
    *) echo "note: $bin_dir is not on PATH; add it to use the koopagit command" >&2 ;;
esac

if pluginkit -m -i "$ext_id" | grep -q '^+'; then
    echo "Installed $app; Finder extension enabled."
else
    echo "Installed $app, but the extension is not enabled yet. Enable KoopaGit in" >&2
    echo "System Settings → General → Login Items & Extensions → Finder." >&2
fi
