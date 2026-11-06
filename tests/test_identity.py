"""Prevent invalid Flatpak reverse-DNS IDs and mismatched Linux application identity."""
import json
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parent.parent
APP_ID = 'org.neural_smith.PrintCreator'


class LinuxIdentityTests(unittest.TestCase):
    def test_flatpak_app_id_has_valid_namespace(self):
        parts = APP_ID.split('.')
        self.assertGreaterEqual(len(parts), 3)
        for part in parts[:-1]:
            self.assertRegex(part, r'^[a-z_][a-z0-9_]*$')
        self.assertRegex(parts[-1], r'^[A-Za-z_][A-Za-z0-9_-]*$')
        self.assertNotIn('-', '.'.join(parts[:-1]))

    def test_author_is_neural_smith_single_source(self):
        cargo = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']
        self.assertEqual(cargo['authors'], ['neural-smith'])

    def test_everything_uses_same_id(self):
        manifest = json.loads((ROOT / 'packaging/flatpak' / f'{APP_ID}.json').read_text())
        self.assertEqual(manifest['app-id'], APP_ID)
        self.assertEqual(manifest['command'], 'print-creator')
        self.assertIn(f'pub const APP_ID: &str = "{APP_ID}";', (ROOT / 'src/lib.rs').read_text())
        self.assertIn(f'{APP_ID}-256.png', (ROOT / 'src/gui.rs').read_text())
        for filename in [
            f'assets/icons/{APP_ID}-256.png',
            f'assets/icons/{APP_ID}.svg',
            f'packaging/desktop/{APP_ID}.desktop.in',
            f'packaging/desktop/{APP_ID}.metainfo.xml.in',
        ]:
            self.assertTrue((ROOT / filename).is_file(), filename)
        for command in manifest['modules'][0]['build-commands']:
            if '/app/share/applications/' in command or '/app/share/metainfo/' in command or '/app/share/icons/' in command:
                self.assertIn(APP_ID, command)

    def test_no_stale_namespace(self):
        for path in ROOT.rglob('*'):
            if not path.is_file() or path.suffix not in {'.rs', '.py', '.sh', '.yml', '.md', '.json', '.in', '.toml'}:
                continue
            contents = path.read_text(encoding='utf-8')
            if path == Path(__file__):
                continue
            for old in ('org.fptbb.PrintCreator', 'org.neural-smith.PrintCreator'):
                # README deliberately warns about the invalid form.
                if path.name == 'README.md' and old == 'org.neural-smith.PrintCreator':
                    continue
                self.assertNotIn(old, contents, f'Found obsolete or invalid app ID in {path}')


if __name__ == '__main__':
    unittest.main()
