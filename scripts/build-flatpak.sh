#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/native-binary.sh"
OUT="${RPC_DIST_DIR:-${ROOT}/dist}"
ID=org.neural_smith.PrintCreator
VER="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
ARCH="$(uname -m)"
STAGE="$OUT/.flatpak-work"
SRC="$STAGE/source"
REPO="$OUT/flatpak-repo"
for c in flatpak flatpak-builder python3; do
  command -v "$c" >/dev/null || { echo "Missing $c" >&2; exit 1; }
done
ensure_native_binary "$ROOT"
rm -rf "$STAGE"
mkdir -p "$SRC/native" "$SRC/tools" "$SRC/assets" "$SRC/packaging/desktop" "$OUT/packages"
# Do not compile in Flatpak SDK: package the same binary used for AppImage.
# This is suitable for controlled CI builders whose ELF dependencies are
# compatible with the selected Flatpak runtime.
install -m755 "$RPC_NATIVE_BINARY" "$SRC/native/print-creator"
install -m644 "$ROOT/LICENSE" "$SRC/LICENSE"
cp -a "$ROOT/assets/icons" "$SRC/assets/"
python3 "$ROOT/scripts/generate-metadata.py" --output-dir "$SRC/packaging/desktop"
bash "$ROOT/scripts/prepare-upx.sh" "$SRC/tools/upx"
cp "$ROOT/packaging/flatpak/${ID}.json" "$STAGE/${ID}.json"
flatpak-builder --user --force-clean --install-deps-from=flathub \
  --repo="$REPO" "$STAGE/build" "$STAGE/${ID}.json"
flatpak build-update-repo "$REPO"
BUNDLE="$OUT/packages/PrintCreator-${VER}-${ARCH}.flatpak"
rm -f -- "$BUNDLE"
flatpak build-bundle "$REPO" "$BUNDLE" "$ID"
[[ -s "$BUNDLE" ]] || { echo 'Flatpak bundle was not created.' >&2; exit 1; }
echo "Created: $BUNDLE"
# When explicitly enabled, install and launch in the actual Flatpak runtime
# (not the SDK) to catch host-versus-runtime shared-library incompatibilities.
if [[ "${RPC_FLATPAK_SMOKE_TEST:-0}" == 1 ]]; then
  flatpak install --user --noninteractive -y "$REPO" "$ID"
  flatpak run --user "$ID" --version
fi
