#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ID=org.neural_smith.PrintCreator
BIN="${1:-$ROOT/target/release/print-creator}"
[[ -x "$BIN" ]] || { echo 'Build with cargo build --release first' >&2; exit 1; }
BIN="$(realpath "$BIN")"
APP="$HOME/.local/share/applications"
mkdir -p "$APP" "$HOME/.local/share/icons/hicolor/scalable/apps" "$HOME/.local/share/icons/hicolor/256x256/apps"
TEMP_META="$(mktemp -d)"
trap 'rm -rf "$TEMP_META"' EXIT
python3 "$ROOT/scripts/generate-metadata.py" --output-dir "$TEMP_META"
install -m644 "$ROOT/assets/icons/${ID}.svg" "$HOME/.local/share/icons/hicolor/scalable/apps/${ID}.svg"
install -m644 "$ROOT/assets/icons/${ID}-256.png" "$HOME/.local/share/icons/hicolor/256x256/apps/${ID}.png"
# Printf-safe desktop Exec (avoid using sed with an unescaped user path).
python3 - "$TEMP_META/${ID}.desktop" "$APP/${ID}.desktop" "$BIN" <<'PYCODE'
from pathlib import Path
import sys
source,target,exe=sys.argv[1:]
# Escape backslashes and quotes in desktop-file double-quoted tokens.
quoted='"'+exe.replace('\\','\\\\').replace('"','\\"')+'"'
text=Path(source).read_text()
text='\n'.join('Exec='+quoted+' gui' if line.startswith('Exec=') else line for line in text.split('\n'))
Path(target).write_text(text)
PYCODE
command -v update-desktop-database >/dev/null && update-desktop-database "$APP" || true
command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 >/dev/null 2>&1 || true
echo "Installed KDE launcher $APP/${ID}.desktop"
