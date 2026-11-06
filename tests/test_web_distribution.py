import pathlib
import tomllib
import unittest

ROOT = pathlib.Path(__file__).resolve().parent.parent

class WebDistributionTests(unittest.TestCase):
    def test_web_build_shares_native_print_engine(self):
        cargo = tomllib.loads((ROOT / 'Cargo.toml').read_text())
        self.assertEqual(cargo['package']['name'], 'print-creator')
        self.assertIn('web', cargo['features'])
        lib = (ROOT / 'src/lib.rs').read_text()
        self.assertIn('pub mod native_pdf;', lib)
        self.assertIn('pub mod render;', lib)
        self.assertIn('pub mod web_app;', lib)
        self.assertIn('pub mod web_assets;', lib)
        web = (ROOT / 'src/web_output.rs').read_text()
        self.assertIn('NativePdfWriter', web)
        self.assertIn('PdfWriter', web)

    def test_browser_privacy_and_project_roundtrip_wiring(self):
        src = (ROOT / 'src/web_app.rs').read_text()
        self.assertIn('import_image', src)
        self.assertIn('import_project', src)
        self.assertIn('web_output::save_project', src)
        self.assertIn('web_output::pdf_bytes', src)
        asset = (ROOT / 'src/web_assets.rs').read_text()
        self.assertIn('thread_local!', asset)
        self.assertNotIn('std::fs::write', asset)

    def test_pages_keeps_signed_repository_and_web_subdirectory(self):
        release = (ROOT / '.github/workflows/release.yml').read_text()
        self.assertIn('download-artifact@v4', release)
        self.assertIn('name: print-creator-web', release)
        self.assertIn('"$PAGES_WORKTREE/app"', release)
        self.assertIn('bash ./scripts/update-flatpak-pages.sh', release)
        root_page = (ROOT / 'scripts/generate-flatpak-pages.py').read_text()
        self.assertIn('href="app/"', root_page)
        self.assertTrue((ROOT / 'web/site/sw.js').exists())

    def test_avoids_stale_namespaces_and_build_products(self):
        for dir in ('src','web','packaging','scripts'):
            for file in (ROOT / dir).rglob('*'):
                if file.suffix in ('.rs','.sh','.js','.html','.py','.toml','.json','.yml'):
                    content = file.read_text(errors='ignore')
                    self.assertNotIn('fptbb', content, file)
        ignore = (ROOT / '.gitignore').read_text()
        self.assertIn('/dist/', ignore)
        self.assertIn('/pkg/', ignore)

if __name__ == '__main__':
    unittest.main()
