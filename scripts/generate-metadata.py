#!/usr/bin/env python3
"""Generate Linux desktop metadata from Cargo.toml (author's single source of truth).

Requires Python 3.11+ (tomllib); no third-party Python packages.
Example: python3 scripts/generate-metadata.py --output-dir dist/desktop
"""
import argparse
import sys
import tomllib
from pathlib import Path
from xml.sax.saxutils import escape

ROOT = Path(__file__).resolve().parent.parent
APP_ID = 'org.neural_smith.PrintCreator'


def author_name(author: str) -> str:
    return author.rsplit('<', 1)[0].strip() if '<' in author else author.strip()


def generate(output: Path) -> list[Path]:
    package = tomllib.loads((ROOT / 'Cargo.toml').read_text(encoding='utf-8'))['package']
    authors = package.get('authors', [])
    if len(authors) != 1 or not authors[0].strip():
        raise ValueError('Set exactly one nonempty author in Cargo.toml [package].authors')
    author = authors[0]
    name = ' '.join(word.capitalize() for word in package['name'].split('-'))
    fields = {
        '@APP_ID@': APP_ID,
        '@APP_NAME@': name,
        '@BIN_NAME@': package['name'],
        '@AUTHOR_DESKTOP@': author.replace('\\', '\\\\').replace('\n', ' '),
        '@AUTHOR_XML@': escape(author_name(author)),
        '@LICENSE_XML@': escape(package['license']),
        '@DESCRIPTION_XML@': escape(package['description']),
    }
    output.mkdir(parents=True, exist_ok=True)
    names = (f'{APP_ID}.desktop', f'{APP_ID}.metainfo.xml')
    results = []
    for name in names:
        template = ROOT / 'packaging' / 'desktop' / (name + '.in')
        data = template.read_text(encoding='utf-8')
        for key, value in fields.items():
            data = data.replace(key, value)
        if '@' in data and any(k in data for k in fields):
            raise ValueError(f'unresolved metadata template variable: {template}')
        target = output / name
        target.write_text(data, encoding='utf-8')
        results.append(target)
    return results


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output-dir', type=Path, required=True, help='directory for generated .desktop/.metainfo.xml')
    args = p.parse_args()
    try:
        for file in generate(args.output_dir):
            print(file)
    except (ValueError, OSError, KeyError) as exc:
        print(f'Metadata generation failed: {exc}', file=sys.stderr)
        return 1
    return 0

if __name__ == '__main__':
    raise SystemExit(main())
