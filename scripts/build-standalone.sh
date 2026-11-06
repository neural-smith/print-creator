#!/usr/bin/env bash
# Package the already-compiled Linux ELF as a directly downloadable release asset.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/native-binary.sh"
ensure_native_binary "$ROOT"
OUT="${RPC_DIST_DIR:-$ROOT/dist}"
ARCH="$(uname -m)"
VER="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$ROOT/Cargo.toml" | head -n 1)"
[[ -n "$VER" ]] || { echo 'Could not determine Cargo version' >&2; exit 1; }
BIN="$OUT/packages/PrintCreator-${VER}-${ARCH}-linux"
mkdir -p "$OUT/packages"
install -m 755 "$RPC_NATIVE_BINARY" "$BIN"
# Compress a *copy*; keep the shared Cargo release binary untouched.
bash "$ROOT/scripts/prepare-upx.sh" "$OUT/.standalone-work/upx"
"$OUT/.standalone-work/upx" --best --lzma "$BIN"
"$OUT/.standalone-work/upx" --test "$BIN"
[[ -s "$BIN" && -x "$BIN" ]] || { echo 'Standalone release executable missing' >&2; exit 1; }
echo "Created: $BIN"
