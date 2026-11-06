"""Sanity checks for source-of-truth author propagation into Linux metadata."""
import subprocess
import tempfile
import tomllib
import unittest
from pathlib import Path
from xml.etree import ElementTree

ROOT = Path(__file__).resolve().parent.parent


class MetadataTest(unittest.TestCase):
    def test_generated_author_is_from_cargo(self):
        cargo = tomllib.loads((ROOT / 'Cargo.toml').read_text(encoding='utf-8'))['package']
        with tempfile.TemporaryDirectory() as output:
            subprocess.run(
                ['python3', str(ROOT / 'scripts/generate-metadata.py'), '--output-dir', output],
                check=True, capture_output=True, text=True,
            )
            desktop = (Path(output) / 'org.neural_smith.PrintCreator.desktop').read_text(encoding='utf-8')
            appstream = ElementTree.parse(Path(output) / 'org.neural_smith.PrintCreator.metainfo.xml')
            self.assertIn('Name=Print Creator\n', desktop)
            self.assertIn('Exec=print-creator gui\n', desktop)
            self.assertIn(f'X-PrintCreator-Author={cargo["authors"][0]}\n', desktop)
            self.assertEqual(appstream.findtext('name'), 'Print Creator')
            self.assertEqual(appstream.findtext('developer_name'), cargo['authors'][0].split('<')[0].strip())


if __name__ == '__main__':
    unittest.main()
