#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# Build the native Rust executable only once, then install the exact same
# uncompressed bytes into the Flatpak and AppImage staging directories.
case "${1:-all}" in
  all)
    source "$ROOT/scripts/native-binary.sh"
    ensure_native_binary "$ROOT"
    bash "$ROOT/scripts/build-flatpak.sh"
    bash "$ROOT/scripts/build-appimage.sh"
    ;;
  flatpak|--flatpak-only) bash "$ROOT/scripts/build-flatpak.sh" ;;
  appimage|--appimage-only) bash "$ROOT/scripts/build-appimage.sh" ;;
  -h|--help) echo "Usage: $0 [all|flatpak|appimage]" ;;
  *) echo "Usage: $0 [all|flatpak|appimage]" >&2; exit 2 ;;
esac
