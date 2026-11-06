#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${RPC_DIST_DIR:-${ROOT}/dist}"
ARCH="$(uname -m)"
VER="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
ID=org.neural_smith.PrintCreator
BUILD="$OUT/.appimage-work"
APPDIR="$BUILD/AppDir"
[[ "$ARCH" == x86_64 || "$ARCH" == aarch64 ]] || { echo "Unsupported arch: $ARCH" >&2; exit 1; }
for c in cargo curl; do command -v "$c" >/dev/null || { echo "Missing $c" >&2; exit 1; }; done
[[ -f "$ROOT/Cargo.lock" ]] || (cd "$ROOT" && cargo generate-lockfile)
(cd "$ROOT" && cargo build --release --locked --features gui)
rm -rf "$BUILD"
mkdir -p "$APPDIR/usr/bin" "$BUILD/tools" "$OUT/packages"
python3 "$ROOT/scripts/generate-metadata.py" --output-dir "$BUILD/metadata"
install -m755 "$ROOT/target/release/print-creator" "$APPDIR/usr/bin/print-creator"
install -Dm644 "$ROOT/LICENSE" "$APPDIR/usr/share/licenses/print-creator/LICENSE"
install -Dm644 "$BUILD/metadata/${ID}.metainfo.xml" "$APPDIR/usr/share/metainfo/${ID}.metainfo.xml"
# Match the desktop entry Icon= name; the source has a -256 suffix.
install -Dm644 "$ROOT/assets/icons/${ID}-256.png" "$BUILD/tools/${ID}.png"
# linuxdeploy must inspect the uncompressed ELF to harvest runtime libraries.
# Running UPX before linuxdeploy can break its dependency discovery.
DEPLOY="${LINUXDEPLOY:-$BUILD/tools/linuxdeploy-${ARCH}.AppImage}"
if [[ ! -f "$DEPLOY" ]]; then
  curl --fail --location --retry 3 \
    "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-${ARCH}.AppImage" \
    -o "$DEPLOY"
fi
chmod +x "$DEPLOY"
DEPLOY="$(realpath "$DEPLOY")"
APPIMAGE_EXTRACT_AND_RUN=1 "$DEPLOY" --appdir "$APPDIR" \
    --executable "$APPDIR/usr/bin/print-creator" \
    --desktop-file "$BUILD/metadata/${ID}.desktop" \
    --icon-file "$BUILD/tools/${ID}.png"
[[ -f "$APPDIR/AppRun" ]] || { echo 'linuxdeploy did not create AppRun.' >&2; exit 1; }
# Compress the deployed executable, *after* dependency harvesting and *before*
# the final AppImage squashfs is generated. Keep the Cargo binary unchanged.
bash "$ROOT/scripts/prepare-upx.sh" "$BUILD/tools/upx"
BIN="$APPDIR/usr/bin/print-creator"
BEFORE="$(stat -c%s "$BIN")"
"$BUILD/tools/upx" --best --lzma "$BIN"
"$BUILD/tools/upx" --test "$BIN"
AFTER="$(stat -c%s "$BIN")"
echo "AppImage executable: ${BEFORE} -> ${AFTER} bytes (UPX --best --lzma)"
# AppImageKit packs the modified AppDir WITHOUT redeploying/rescanning ELF.
PACKER="${APPIMAGETOOL:-$BUILD/tools/appimagetool-${ARCH}.AppImage}"
if [[ ! -f "$PACKER" ]]; then
  curl --fail --location --retry 3 \
    "https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-${ARCH}.AppImage" \
    -o "$PACKER"
fi
chmod +x "$PACKER"
PACKER="$(realpath "$PACKER")"
BUNDLE="$OUT/packages/PrintCreator-${VER}-${ARCH}.AppImage"
rm -f -- "$BUNDLE"
ARCH="$ARCH" APPIMAGE_EXTRACT_AND_RUN=1 "$PACKER" "$APPDIR" "$BUNDLE"
[[ -s "$BUNDLE" ]] || { echo 'AppImage was not created.' >&2; exit 1; }
chmod +x "$BUNDLE"
echo "Created: $BUNDLE"
