"""Static contract checks for the Flatpak signing key workflow."""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]


class TestSigningWorkflow(unittest.TestCase):
    def test_armored_secret_is_preferred(self):
        workflow = (ROOT / ".github/workflows/release.yml").read_text()
        self.assertIn("secrets.FLATPAK_SIGNING_KEY }}", workflow)
        self.assertIn("secrets.FLATPAK_SIGNING_KEY_B64 }}", workflow)
        self.assertIn("bash ./scripts/import-flatpak-signing-key.sh", workflow)

    def test_import_is_validated_before_gpg_import(self):
        source = (ROOT / "scripts/import-flatpak-signing-key.sh").read_text()
        self.assertLess(source.index("base64 --decode >"), source.index("gpg --batch --import"))
        self.assertIn("--detach-sign", source)
        self.assertIn("FLATPAK_GPG_KEY_ID", source)


if __name__ == "__main__":
    unittest.main()
