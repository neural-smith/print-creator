#!/usr/bin/env bash
# Supply the official, statically linked UPX executable for use both on the
# build host and within Flatpak's offline SDK build sandbox.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${1:-$ROOT/dist/tools/upx}"
ARCH="$(uname -m)"
VERSION="5.2.1"
case "$ARCH" in
  x86_64) TARGET=amd64; CHECKSUM=402162aad30af47e60dbd767fb2e64ca394ace9727ba1f40283641f1d1b91657 ;;
  aarch64) TARGET=arm64; CHECKSUM=a72d112c5970a904a31da0b9c84f919bc16b9a311787c12245508544a78c7d36 ;;
  *) echo "UPX is not configured for $ARCH (supported: x86_64, aarch64)." >&2; exit 1 ;;
esac
mkdir -p "$(dirname "$DEST")"
# Use RPC_UPX_BIN to point to a manually installed UPX (must be runnable inside
# the Flatpak SDK, e.g. an official statically linked UPX release).
if [[ -n "${RPC_UPX_BIN:-}" ]]; then
  [[ -x "$RPC_UPX_BIN" ]] || { echo "RPC_UPX_BIN is not executable: $RPC_UPX_BIN" >&2; exit 1; }
  [[ "$(realpath "$RPC_UPX_BIN")" == "$(realpath -m "$DEST")" ]] || cp -- "$RPC_UPX_BIN" "$DEST"
else
  for tool in curl tar sha256sum; do command -v "$tool" >/dev/null || { echo "Missing dependency: $tool" >&2; exit 1; }; done
  TARBALL="upx-${VERSION}-${TARGET}_linux.tar.xz"
  TMP="$(mktemp -d)"
  trap 'rm -rf "$TMP"' EXIT
  curl --fail --location --retry 3 --silent --show-error \
    "https://github.com/upx/upx/releases/download/v${VERSION}/${TARBALL}" -o "$TMP/$TARBALL"
  echo "${CHECKSUM}  $TMP/$TARBALL" | sha256sum --check --status || {
    echo 'UPX archive checksum mismatch; aborting build.' >&2; exit 1;
  }
  tar -xJf "$TMP/$TARBALL" -C "$TMP" "upx-${VERSION}-${TARGET}_linux/upx"
  cp -- "$TMP/upx-${VERSION}-${TARGET}_linux/upx" "$DEST"
fi
chmod 755 "$DEST"
"$DEST" --version | head -n 1
