"""Run the WebAssembly packaging shell script with a mocked wasm-pack.

This verifies the CLI boundary between wasm-pack options and forwarded Cargo
options without requiring a Rust toolchain in the test environment.
"""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent


class WebBuildScriptTests(unittest.TestCase):
    def test_wasm_pack_flags_do_not_leak_into_cargo_arguments(self):
        with tempfile.TemporaryDirectory() as work:
            root = Path(work)
            (root / "scripts").mkdir()
            (root / "web/site").mkdir(parents=True)
            (root / "assets/icons").mkdir(parents=True)
            (root / "fake-bin").mkdir()
            shutil.copy2(ROOT / "scripts/build-web.sh", root / "scripts/build-web.sh")
            (root / "Cargo.lock").write_text("# mock lockfile\n")
            (root / "web/site/index.html").write_text("<!doctype html><title>Print Creator</title>")
            (root / "assets/icons/org.neural_smith.PrintCreator-256.png").write_bytes(b"icon")

            cargo = root / "fake-bin/cargo"
            cargo.write_text("#!/usr/bin/env bash\nexit 0\n")
            cargo.chmod(0o755)
            wasm_pack = root / "fake-bin/wasm-pack"
            wasm_pack.write_text("""#!/usr/bin/env bash
set -euo pipefail
printf '%s\\n' "$@" > "$WASM_PACK_ARGS"
mkdir -p target/wasm-pack
printf 'export default function init() {}\\n' > target/wasm-pack/print_creator.js
printf 'fake wasm output' > target/wasm-pack/print_creator_bg.wasm
""")
            wasm_pack.chmod(0o755)

            env = os.environ.copy()
            env["PATH"] = f"{root / 'fake-bin'}:{env['PATH']}"
            env["WASM_PACK_ARGS"] = str(root / "args.txt")
            result = subprocess.run(
                ["bash", str(root / "scripts/build-web.sh")], cwd=root,
                env=env, capture_output=True, text=True,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual((root / "args.txt").read_text().splitlines(), [
                "build", "--release", "--target", "web", "--out-dir",
                "target/wasm-pack", "--out-name", "print_creator", "--",
                "--no-default-features", "--features", "web", "--locked",
            ])
            self.assertTrue((root / "dist/web/app/pkg/print_creator_bg.wasm").is_file())
            self.assertTrue((root / "dist/web/app/pkg/print_creator.js").is_file())
            self.assertTrue((root / "dist/web/app/index.html").is_file())
            self.assertTrue((root / "dist/web/app/icon.png").is_file())


if __name__ == "__main__":
    unittest.main()
