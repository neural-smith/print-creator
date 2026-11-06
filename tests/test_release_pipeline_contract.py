"""CI packaging contract: source OSTree and standalone executable are transferred."""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]

class ReleasePipelineContractTest(unittest.TestCase):
    def test_pages_uses_build_repo_not_flatpak_bundle_for_import(self):
        rust = (ROOT / '.github/workflows/rust.yml').read_text()
        release = (ROOT / '.github/workflows/release.yml').read_text()
        publisher = (ROOT / 'scripts/update-flatpak-pages.sh').read_text()
        self.assertIn('tar -C dist/flatpak-repo -czf dist/flatpak-ostree-repo.tar.gz .', rust)
        self.assertIn('name: print-creator-flatpak-ostree-${{ matrix.variant.arch }}', rust)
        self.assertIn('pattern: print-creator-flatpak-ostree-*', release)
        self.assertIn('"aarch64:$BUILT_REPO_AARCH64"', release)
        self.assertIn('pull-local --untrusted', publisher)
        self.assertIn('mkdir -p "$REPO_DIR/refs/heads" "$REPO_DIR/refs/remotes"', publisher)
        self.assertIn('flatpak build-sign', publisher)
        self.assertNotRegex(publisher, r'(?m)^\s*flatpak build-import-bundle\b')

    def test_release_uploads_standalone_and_attests_all_outputs(self):
        rust = (ROOT / '.github/workflows/rust.yml').read_text()
        release = (ROOT / '.github/workflows/release.yml').read_text()
        combined = (ROOT / 'scripts/build-all.sh').read_text()
        standalone = (ROOT / 'scripts/build-standalone.sh').read_text()
        self.assertIn('build-standalone.sh', combined)
        self.assertIn('dist/packages/*-linux', rust)
        self.assertIn('sha256sum -- *.flatpak *.AppImage *-linux > "SHA256SUMS-$(uname -m)"', rust)
        self.assertIn("subject-path: 'dist/packages/*'", rust)
        self.assertIn('dist/packages/*-linux dist/packages/SHA256SUMS', release)
        self.assertIn('"$OUT/.standalone-work/upx" --best --lzma "$BIN"', standalone)

if __name__ == '__main__':
    unittest.main()
