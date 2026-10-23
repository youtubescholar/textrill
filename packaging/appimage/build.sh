#!/usr/bin/env bash
# Build a self-contained AppImage of the textrill GUI (with the CLI engine
# alongside it).
#
# This is a local/test packaging path, not a substitute for the Flatpak: an
# AppImage built here inherits this host's glibc, so it runs on distributions
# at least as new as Ubuntu 24.04. See packaging/README.md.
#
# Usage:
#   packaging/appimage/build.sh
#
# Environment:
#   APPIMAGETOOL   path to an appimagetool binary (otherwise downloaded once to
#                  build/appimage/tools/). The download needs network the first
#                  time; afterwards the build is offline.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
out="$root/build/appimage"
appdir="$out/textrill-gui.AppDir"
app_id="io.github.youtubescholar.Textrill"

echo ">> building release binaries"
cargo build --release --manifest-path "$root/textrill/Cargo.toml"
cargo build --release --manifest-path "$root/textrill-gui-rs/Cargo.toml"

echo ">> assembling $appdir"
rm -rf "$appdir"
install -Dm755 "$root/textrill-gui-rs/target/release/textrill-gui" "$appdir/usr/bin/textrill-gui"
install -Dm755 "$root/textrill/target/release/textrill" "$appdir/usr/bin/textrill"

# The desktop file and its matching icon live at the AppDir root, which is what
# appimagetool reads; the same pair is copied into the hicolor theme so a
# desktop that integrates the AppImage can find the icon.
install -Dm644 "$here/$app_id.desktop" "$appdir/$app_id.desktop"
mkdir -p "$appdir/usr/share/applications" "$appdir/usr/share/metainfo" \
    "$appdir/usr/share/icons/hicolor/256x256/apps" \
    "$appdir/usr/share/icons/hicolor/scalable/apps"
install -Dm644 "$here/$app_id.desktop" "$appdir/usr/share/applications/$app_id.desktop"
install -Dm644 "$here/../$app_id.metainfo.xml" "$appdir/usr/share/metainfo/$app_id.metainfo.xml"
# appimagetool still looks for the legacy `.appdata.xml` name when deciding
# whether AppStream metadata is present. A symlink keeps one component file.
ln -sf "$app_id.metainfo.xml" "$appdir/usr/share/metainfo/$app_id.appdata.xml"
install -Dm644 "$here/$app_id.svg" "$appdir/usr/share/icons/hicolor/scalable/apps/$app_id.svg"

icon_png="$here/$app_id.png"
if [ ! -f "$icon_png" ]; then
    echo ">> rendering icon from SVG"
    if command -v inkscape >/dev/null; then
        inkscape --export-type=png --export-filename="$icon_png" \
            --export-width=256 --export-height=256 "$here/$app_id.svg" >/dev/null
    elif command -v convert >/dev/null; then
        convert -background none -resize 256x256 "$here/$app_id.svg" "$icon_png"
    else
        echo "!! need inkscape or ImageMagick to render $app_id.svg; install one" >&2
        exit 1
    fi
fi
install -Dm644 "$icon_png" "$appdir/$app_id.png"
install -Dm644 "$icon_png" "$appdir/.DirIcon"
install -Dm644 "$icon_png" "$appdir/usr/share/icons/hicolor/256x256/apps/$app_id.png"

install -Dm755 "$here/AppRun" "$appdir/AppRun"

echo ">> fetching appimagetool if needed"
tool="${APPIMAGETOOL:-$out/tools/appimagetool}"
if [ ! -x "$tool" ]; then
    mkdir -p "$(dirname "$tool")"
    curl -fL -o "$tool" \
        https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
    chmod +x "$tool"
fi

echo ">> packaging"
# No FUSE here, so ask appimagetool to unpack itself and run from there. Set
# unconditionally: it is harmless where FUSE is available.
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$tool" "$appdir" \
    "$out/textrill-gui-x86_64.AppImage"

echo ">> done: $out/textrill-gui-x86_64.AppImage"
