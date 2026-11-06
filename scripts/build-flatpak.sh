#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${RPC_DIST_DIR:-${ROOT}/dist}"
ID=org.neural_smith.PrintCreator
VER="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
ARCH="$(uname -m)"
STAGE="$OUT/.flatpak-work"
SRC="$STAGE/source"
REPO="$OUT/flatpak-repo"
for c in cargo flatpak flatpak-builder; do
 command -v "$c" >/dev/null || { echo "Missing $c" >&2; exit 1; }
done
[[ -f "$ROOT/Cargo.lock" ]] || (cd "$ROOT" && cargo generate-lockfile)
rm -rf "$STAGE"
mkdir -p "$SRC" "$OUT/packages" "$SRC/tools"
cp -a "$ROOT/Cargo.toml" "$ROOT/Cargo.lock" "$ROOT/LICENSE" "$SRC/"
cp -a "$ROOT/src" "$ROOT/assets" "$ROOT/packaging" "$SRC/"
python3 "$ROOT/scripts/generate-metadata.py" --output-dir "$SRC/packaging/desktop"
mkdir -p "$SRC/.cargo"
# Vendor on the host, then build fully offline in the Flatpak SDK.
(cd "$SRC" && cargo vendor --locked vendor > .cargo/config.toml)
# Include the static UPX binary in the source: Flatpak's SDK cannot use the
# host's ordinary /usr/bin/upx, and the sandbox build has no network access.
"$ROOT/scripts/prepare-upx.sh" "$SRC/tools/upx"
cp "$ROOT/packaging/flatpak/${ID}.json" "$STAGE/${ID}.json"
flatpak-builder --user --force-clean --install-deps-from=flathub \
  --repo="$REPO" "$STAGE/build" "$STAGE/${ID}.json"
flatpak build-update-repo "$REPO"
BUNDLE="$OUT/packages/PrintCreator-${VER}-${ARCH}.flatpak"
rm -f -- "$BUNDLE"
flatpak build-bundle "$REPO" "$BUNDLE" "$ID"
[[ -s "$BUNDLE" ]] || { echo 'Flatpak bundle was not created.' >&2; exit 1; }
echo "Created: $BUNDLE"
