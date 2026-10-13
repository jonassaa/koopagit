#!/bin/sh
# Assembles KoopaGit.app with the Finder Sync extension and the Rust binaries,
# ad-hoc signed for local use.
#
#   integrations/macos/build.sh [output-dir]      (default: target/macos)
#
# Then: open <output-dir>/KoopaGit.app   and enable the extension
#       (pluginkit -e use -i dev.koopagit.FinderSync).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
out=${1:-"$root/target/macos"}
app="$out/KoopaGit.app"
appex="$app/Contents/PlugIns/KoopaGitFinderSync.appex"
arch=$(uname -m)
target="$arch-apple-macos12.0"
sdk=$(xcrun --sdk macosx --show-sdk-path)

cargo build --release --manifest-path "$root/Cargo.toml" -p koopagit-shell -p koopagit-proc

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$appex/Contents/MacOS"

# Host app.
cp "$here/Host/Info.plist" "$app/Contents/Info.plist"
cp "$here/Host/AppIcon.icns" "$app/Contents/Resources/"
xcrun swiftc -O -sdk "$sdk" -target "$target" \
    -o "$app/Contents/MacOS/KoopaGit" "$here/Host/main.swift"
cp "$root/target/release/koopagit-shell" "$root/target/release/koopagit-proc" "$app/Contents/MacOS/"

# Finder Sync extension (an app extension's entry point is NSExtensionMain).
# -parse-as-library: otherwise the single file is compiled as a script whose
# globals are initialised by a `main` that never runs.
cp "$here/FinderSync/Info.plist" "$appex/Contents/Info.plist"
xcrun swiftc -O -sdk "$sdk" -target "$target" -parse-as-library \
    -application-extension -module-name KoopaGitFinderSync \
    -framework FinderSync \
    -Xlinker -e -Xlinker _NSExtensionMain \
    -o "$appex/Contents/MacOS/KoopaGitFinderSync" "$here/FinderSync/FinderSync.swift"

# Sign inside-out: helpers, extension (sandboxed), then the app.
codesign --force --sign - "$app/Contents/MacOS/koopagit-shell" "$app/Contents/MacOS/koopagit-proc"
codesign --force --sign - --entitlements "$here/FinderSync/FinderSync.entitlements" "$appex"
codesign --force --sign - "$app"

echo "Built $app"
