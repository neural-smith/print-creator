#!/usr/bin/env python3
"""Create an installable .flatpakrepo, .flatpakref and static landing page.

No network calls and no third-party Python dependencies. The public GPG key
comes from the signing key used for the OSTree repo (never the private key).
"""
import argparse
import base64
from html import escape
from pathlib import Path
from urllib.parse import urlparse

APP_ID = 'org.neural_smith.PrintCreator'
REMOTE_NAME = 'print-creator'


def render(repo: Path, base_url: str, public_key: Path) -> None:
    base_url = base_url.rstrip('/') + '/'
    parsed = urlparse(base_url)
    if parsed.scheme != 'https' or not parsed.netloc or parsed.query or parsed.fragment:
        raise ValueError('Repository URL must be an absolute HTTPS URL without query or fragment')
    if '\n' in base_url or '\r' in base_url:
        raise ValueError('Invalid repository URL')
    pub = public_key.read_bytes()
    if not pub:
        raise ValueError('The GPG public key is empty')
    key_b64 = base64.b64encode(pub).decode('ascii')
    repo.mkdir(parents=True, exist_ok=True)
    (repo / '.nojekyll').touch()
    (repo / 'index.flatpakrepo').write_text(f'''[Flatpak Repo]
Title=Print Creator
Name={REMOTE_NAME}
Comment=Official Print Creator Flatpak repository
Description=Signed Flatpak releases of Print Creator
Url={base_url}
Homepage={base_url}
GPGKey={key_b64}
''', encoding='utf-8')
    (repo / f'{APP_ID}.flatpakref').write_text(f'''[Flatpak Ref]
Title=Print Creator
Name={APP_ID}
Branch=master
Url={base_url}
IsRuntime=false
RuntimeRepo=https://dl.flathub.org/repo/flathub.flatpakrepo
SuggestRemoteName={REMOTE_NAME}
GPGKey={key_b64}
''', encoding='utf-8')
    (repo / 'index.html').write_text(f'''<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Print Creator — Flatpak repository</title><style>
:root {{color-scheme:dark light;font-family:system-ui,sans-serif}}
body {{max-width:48rem;margin:4rem auto;padding:0 1.5rem;line-height:1.6}}
a {{color:#74c7ec}}code {{overflow-wrap:anywhere}}
</style></head><body>
<h1>Print Creator</h1>
<p>Arrange photographs and create printable PDFs locally in your browser, or install the Linux application.</p>
<p><a href="app/">Open Print Creator in your browser</a></p>
<p>Signed, automatically updatable Flatpak packages.</p>
<p><a href="{escape(APP_ID)}.flatpakref">Install Print Creator (.flatpakref)</a></p>
<p><a href="index.flatpakrepo">Add repository (.flatpakrepo)</a></p>
<pre><code>flatpak install --user {escape(base_url + APP_ID + '.flatpakref')}</code></pre>
<p>Once installed from this repository, run <code>flatpak update</code> or use your desktop software center for updates.</p>
<p>GitHub's immutable release assets and this signed update repository are independent distribution channels.</p>
</body></html>
''', encoding='utf-8')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, required=True)
    parser.add_argument('--url', required=True)
    parser.add_argument('--gpg-public-key', type=Path, required=True)
    args = parser.parse_args()
    render(args.repo, args.url, args.gpg_public_key)


if __name__ == '__main__':
    main()
