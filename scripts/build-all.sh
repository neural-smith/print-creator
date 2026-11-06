#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
case "${1:-all}" in
 all) "$ROOT/scripts/build-flatpak.sh"; "$ROOT/scripts/build-appimage.sh";;
 flatpak|--flatpak-only) "$ROOT/scripts/build-flatpak.sh";;
 appimage|--appimage-only) "$ROOT/scripts/build-appimage.sh";;
 -h|--help) echo "Usage: $0 [all|flatpak|appimage]";;
 *) echo "Usage: $0 [all|flatpak|appimage]" >&2; exit 2;;
esac
