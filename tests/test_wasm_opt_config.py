"""Regression test for the incompatible bundled wasm-opt configuration."""
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parent.parent

class WasmOptConfigTests(unittest.TestCase):
    def test_wasm_opt_is_disabled_only_in_wasm_pack_release(self):
        manifest = tomllib.loads((ROOT / 'Cargo.toml').read_text(encoding='utf-8'))
        self.assertIs(
            manifest['package']['metadata']['wasm-pack']['profile']['release']['wasm-opt'],
            False,
        )
        self.assertEqual(manifest['profile']['release']['lto'], 'thin')
        self.assertEqual(manifest['profile']['release']['codegen-units'], 1)

if __name__ == '__main__':
    unittest.main()
