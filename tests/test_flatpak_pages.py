"""Smoke tests for the static repository install metadata."""
import base64
import importlib.util
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

MODULE = Path(__file__).resolve().parent.parent / 'scripts' / 'generate-flatpak-pages.py'
spec = importlib.util.spec_from_file_location('flatpak_pages', MODULE)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class FlatpakPagesTest(unittest.TestCase):
    def test_ref_repo_and_nojekyll(self):
        with TemporaryDirectory() as tmp:
            root = Path(tmp)
            gpg = root / 'key.gpg'
            gpg.write_bytes(b'binary-public-key\0\1')
            repo = root / 'public'
            module.render(repo, 'https://neural-smith.github.io/print-creator', gpg)
            ref = (repo / 'org.neural_smith.PrintCreator.flatpakref').read_text()
            remote = (repo / 'index.flatpakrepo').read_text()
            for item in (ref, remote):
                self.assertIn('Url=https://neural-smith.github.io/print-creator/', item)
                self.assertIn('GPGKey=' + base64.b64encode(gpg.read_bytes()).decode('ascii'), item)
            self.assertIn('Name=org.neural_smith.PrintCreator', ref)
            self.assertIn('Branch=master', ref)
            self.assertIn('IsRuntime=false', ref)
            self.assertIn('RuntimeRepo=https://dl.flathub.org/repo/flathub.flatpakrepo', ref)
            self.assertTrue((repo / '.nojekyll').exists())
            self.assertIn('flatpak install --user', (repo / 'index.html').read_text())

    def test_untrusted_urls_rejected(self):
        with TemporaryDirectory() as tmp:
            root = Path(tmp)
            key = root / 'key'
            key.write_bytes(b'key')
            for bad in ('http://example.com/', 'file:///tmp', 'https://example.com/?bad=1', 'https://example.com/\nfoo'):
                with self.subTest(bad=bad), self.assertRaises(ValueError):
                    module.render(root / 'output', bad, key)

    def test_missing_key_rejected(self):
        with TemporaryDirectory() as tmp:
            root = Path(tmp)
            key = root / 'key'
            key.write_bytes(b'')
            with self.assertRaises(ValueError):
                module.render(root / 'output', 'https://example.com/', key)


if __name__ == '__main__':
    unittest.main()
