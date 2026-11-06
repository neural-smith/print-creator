#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
command -v cargo >/dev/null || { echo 'Cargo is required to build WebAssembly' >&2; exit 1; }
command -v wasm-pack >/dev/null || { echo 'Install wasm-pack: cargo install wasm-pack --locked' >&2; exit 1; }
# Keep a fresh lockfile for reproducibility; CI requires it to be committed.
if [[ ! -f Cargo.lock ]]; then cargo generate-lockfile; fi
# wasm-pack flags MUST come before `--`. Cargo-only flags MUST come after it.
# Passing --features before -- causes wasm-pack to forward subsequent flags
# (including --out-dir) to Cargo, which rejects them.
wasm-pack build --release --target web \
  --out-dir target/wasm-pack --out-name print_creator \
  -- --no-default-features --features web --locked
mkdir -p dist/web/app/pkg
cp -a web/site/. dist/web/app/
cp target/wasm-pack/print_creator.js target/wasm-pack/print_creator_bg.wasm dist/web/app/pkg/
cp assets/icons/org.neural_smith.PrintCreator-256.png dist/web/app/icon.png
# The web app lives below /app/, not over the existing Flatpak OSTree root.
test -s dist/web/app/pkg/print_creator_bg.wasm
test -s dist/web/app/index.html
printf 'Built web app in dist/web/app/\nServe dist/web with an HTTP server (not file://).\n'
