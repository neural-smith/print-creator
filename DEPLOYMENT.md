# Print Creator

The same GitHub Actions run produces native `x86_64` and `aarch64` release
packages. Read `README.md` for local GUI/build dependencies. No cross-compiling,
QEMU emulation or extra Rust native builds are necessary.

## What is built

On `ubuntu-24.04` and `ubuntu-24.04-arm` **independently**, the release
workflow runs one `cargo build --release` per architecture, then reuses that
exact executable for Flatpak, AppImage and UPX-compressed standalone ELF.
`wasm32-unknown-unknown` is a separate build for the browser app.

Result on a `vX.Y.Z` tag:

- `PrintCreator-X.Y.Z-x86_64.flatpak`, `.AppImage`, `-linux`
- `PrintCreator-X.Y.Z-aarch64.flatpak`, `.AppImage`, `-linux`
- one `SHA256SUMS` covering all six files
- GitHub Actions signed build attestations for generated package assets

The Flatpak source repositories for both architectures are temporary CI
artifacts. The publishing job pulls each architecture into the **same** signed
OSTree repository and runs `flatpak build-update-repo` once. The `.flatpakref`
auto-selects the native architecture when installed on a matching machine.

## First-time configuration

1. Push to a public GitHub repository (GitHub's standard ARM64 runner is
   available for public repos; review your plan if repository is private).
2. Under Settings → Releases, enable immutable releases **before** publishing.
3. Under Settings → Pages, choose **GitHub Actions** as deployment source.
4. In `github-pages` environment deployment rules, allow the `v*` **tag**.
5. Set Actions secret `FLATPAK_SIGNING_KEY` to the ASCII-armored secret key
   of the **existing** dedicated Flatpak signer. Do not generate a new key to
   solve a CI error: that would break existing update trust.
6. Set Actions variable `FLATPAK_PAGES_ENABLED` to `true` using
   `gh variable set FLATPAK_PAGES_ENABLED --body true`.
7. Keep `Cargo.lock` committed and all `scripts/*.sh` executable in Git.

The Flatpak Pages site has a 1 GB published site limit. Multiarch objects
and history may approach the limit faster; the workflow rejects oversized
publishes without replacing the old Pages branch. Move the repository to a
larger static host before reaching the limit, preserving URL or using an
explicit remote migration.

## Local builds and verification

```bash
cargo test --all-features --locked
bash scripts/build-all.sh       # packages the current machine's architecture
# Or run per-format: bash scripts/build-all.sh flatpak|appimage|standalone
# Optional real Flatpak smoke test:
RPC_FLATPAK_SMOKE_TEST=1 bash scripts/build-all.sh
```

ARM64 packaging requires an ARM64 Linux machine; GitHub Actions handles it
using `ubuntu-24.04-arm`. UPX is run only on staged ELF copies, after
`linuxdeploy` dependency inspection and before final packaging.

For a new release, update Cargo version and lockfile, commit, push `main`,
then push a **new** annotated version tag. Never retag an immutable release.

End users can install either architecture from one URL:

```bash
flatpak install --user https://neural-smith.github.io/print-creator/org.neural_smith.PrintCreator.flatpakref
flatpak update --user org.neural_smith.PrintCreator
```

AppImage and standalone ELF artifacts do not auto-update through Flatpak.
Before announcing ARM support, validate Flatpak launches on aarch64 and test
AppImage/ELF on an ARM64 desktop, not just that packaging produced a file.
