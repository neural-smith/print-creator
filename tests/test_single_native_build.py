"""Regression: Linux packages share one host-built release executable."""

import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APPID = "org.neural_smith.PrintCreator"


class SharedBinaryTest(unittest.TestCase):
    def test_manifest_installs_prebuilt_binary_without_cargo(self):
        manifest = json.loads((ROOT / "packaging/flatpak" / f"{APPID}.json").read_text())
        commands = manifest["modules"][0]["build-commands"]
        self.assertIn("install -Dm755 native/print-creator /app/bin/print-creator", commands)
        self.assertFalse(any("cargo " in command for command in commands))

    def test_combined_packages_compile_once(self):
        # Mock external packagers; this is about build-script orchestration,
        # not a substitute for a real Rust/Flatpak/AppImage runtime test.
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder) / "print-creator"
            tools = Path(folder) / "tools"
            tools.mkdir()
            root.mkdir()
            for directory in ("scripts", "assets/icons", "packaging/flatpak", "packaging/desktop"):
                shutil.copytree(ROOT / directory, root / directory)
            for file in ("Cargo.toml", "LICENSE"):
                shutil.copy2(ROOT / file, root / file)
            (root / "Cargo.lock").write_text("# Fake lockfile for packaging script test\n")

            def executable(name, body):
                p = tools / name
                p.write_text("#!/usr/bin/env bash\nset -euo pipefail\n" + body)
                p.chmod(0o755)
                return p

            cargo = executable("cargo", '''
if [[ "$1" == build ]]; then
  echo "$*" >> "$BUILD_LOG"
  mkdir -p "$CARGO_TARGET_DIR/release"
  printf '#!/bin/sh\\necho print-creator mock\\n' > "$CARGO_TARGET_DIR/release/print-creator"
  chmod +x "$CARGO_TARGET_DIR/release/print-creator"
else
  echo "Unexpected cargo invocation: $*" >&2
  exit 1
fi
''')
            executable("flatpak-builder", '''
for arg in "$@"; do
  if [[ "$arg" == --repo=* ]]; then mkdir -p "${arg#--repo=}"; fi
done
''')
            executable("flatpak", '''
case "$1" in
  build-update-repo) ;;
  build-bundle) printf 'mock flatpak package\\n' > "$3" ;;
  *) echo "Unexpected flatpak command: $*" >&2; exit 1 ;;
esac
''')
            upx = executable("upx", '''
case "${1:-}" in
  --version) echo "UPX mock" ;;
  --best|--test) ;;
  *) exit 1 ;;
esac
''')
            deploy = executable("linuxdeploy", '''
for arg in "$@"; do
  [[ "$arg" == --appdir ]] && next=1 && continue
  if [[ "${next:-}" == 1 ]]; then appdir="$arg"; next=0; fi
done
mkdir -p "$appdir"
printf '#!/bin/sh\\nexit 0\\n' > "$appdir/AppRun"
chmod +x "$appdir/AppRun"
''')
            packer = executable("appimagetool", '''
printf 'mock appimage package\\n' > "${@: -1}"
''')
            build_log = Path(folder) / "cargo-builds.log"
            env = dict(os.environ, PATH=f"{tools}:{os.environ['PATH']}",
                       CARGO_TARGET_DIR=str(Path(folder) / "cargo-target"),
                       RPC_DIST_DIR=str(Path(folder) / "dist"),
                       RPC_UPX_BIN=str(upx), LINUXDEPLOY=str(deploy),
                       APPIMAGETOOL=str(packer), BUILD_LOG=str(build_log))
            result = subprocess.run(["bash", "scripts/build-all.sh"], cwd=root,
                                    env=env, text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stdout + "\n" + result.stderr)
            self.assertEqual(len(build_log.read_text().splitlines()), 1)
            packages = list((Path(folder) / "dist/packages").iterdir())
            self.assertEqual(len(packages), 2, [str(p) for p in packages])
            self.assertEqual({p.suffix for p in packages}, {".flatpak", ".AppImage"})
            native = (Path(folder) / "cargo-target/release/print-creator").read_bytes()
            staged = (Path(folder) / "dist/.flatpak-work/source/native/print-creator").read_bytes()
            installed = (Path(folder) / "dist/.appimage-work/AppDir/usr/bin/print-creator").read_bytes()
            self.assertEqual(native, staged)
            self.assertEqual(native, installed)


if __name__ == "__main__":
    unittest.main()
