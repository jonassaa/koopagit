#!/bin/sh
# Regenerates the raster icons from koopagit-logo.svg. Needs Inkscape, and the
# JetBrains Mono font visible to fontconfig (the "GIT" text is live text);
# macOS's iconutil builds the .icns.
#
#   assets/logo/render.sh
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
svg="$here/koopagit-logo.svg"

png() {
    inkscape "$svg" --export-type=png --export-filename="$2" -w "$1" -h "$1" >/dev/null 2>&1
}

# Slint window icon (title bar / taskbar).
png 256 "$root/crates/koopagit-dialogs/ui/images/koopagit.png"

# macOS app bundle icon, also used for the Finder menu.
if command -v iconutil >/dev/null 2>&1; then
    set_dir=$(mktemp -d)/AppIcon.iconset
    mkdir -p "$set_dir"
    for s in 16 32 128 256 512; do
        png "$s" "$set_dir/icon_${s}x${s}.png"
        png $((s * 2)) "$set_dir/icon_${s}x${s}@2x.png"
    done
    iconutil -c icns -o "$root/integrations/macos/Host/AppIcon.icns" "$set_dir"
    rm -rf "$(dirname "$set_dir")"
fi
