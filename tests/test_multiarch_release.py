"""Release contract: exactly one native release build per architecture."""
from pathlib import Path
import unittest
ROOT = Path(__file__).resolve().parents[1]
class MultiarchContract(unittest.TestCase):
    def test_two_native_runners_and_no_cross_build(self):
        ci = (ROOT/'.github/workflows/rust.yml').read_text()
        self.assertIn('ubuntu-24.04-arm', ci)
        self.assertIn('{arch: x86_64, runner: ubuntu-24.04}', ci)
        self.assertIn('{arch: aarch64, runner: ubuntu-24.04-arm}', ci)
        self.assertEqual(ci.count('bash ./scripts/build-all.sh'), 1)
        self.assertIn('print-creator-flatpak-ostree-${{ matrix.variant.arch }}', ci)
        self.assertIn('SHA256SUMS-$(uname -m)', ci)
    def test_publisher_combines_refs_and_checksums(self):
        release = (ROOT/'.github/workflows/release.yml').read_text()
        self.assertIn('pattern: print-creator-linux-*',release)
        self.assertIn('pattern: print-creator-flatpak-ostree-*',release)
        self.assertIn('SHA256SUMS-aarch64',release)
        self.assertIn('SHA256SUMS-x86_64',release)
        self.assertIn('aarch64:$BUILT_REPO_AARCH64',release)
        pub = (ROOT/'scripts/update-flatpak-pages.sh').read_text()
        self.assertIn('SOURCES+=("$@")',pub)
        self.assertIn('flatpak build-sign',pub)
        self.assertEqual(pub.count('flatpak build-update-repo --title'),1)
        self.assertNotIn('flatpak build-import-bundle "',pub)
    def test_arm_native_packaging_and_upx(self):
        for name in ['build-appimage.sh','prepare-upx.sh']:
            self.assertIn('aarch64', (ROOT/'scripts'/name).read_text())
        self.assertIn('ensure_native_binary "$ROOT"', (ROOT/'scripts/build-all.sh').read_text())
if __name__ == '__main__': unittest.main()
