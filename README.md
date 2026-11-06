# Print Creator

<p align="center">
  <img src="assets/icons/org.neural_smith.PrintCreator-256.png" width="112" alt="Print Creator application icon">
</p>

**A standalone photo-layout and print-export application for Linux, written in Rust.**
Inspired by digiKam's Print Creator, with compatible XML photo templates, a native desktop interface, and a scriptable CLI. No digiKam library, photo database, Qt, or KDE installation is needed.

- **Version:** 0.5.0
- **Application ID:** `org.neural_smith.PrintCreator` (the public organization name is `neural-smith`)
- **Executable / Cargo package:** `print-creator`
- **Author:** configured in [`Cargo.toml`](Cargo.toml), not duplicated across package metadata
- **License:** GPL-2.0-or-later (see [LICENSE](LICENSE))
- **Platforms:** Linux x86_64 (Flatpak, AppImage, native build); other architectures may be built from source

> **Project scope:** This is an independent Rust implementation, not an official digiKam/KDE product. The adaptive algorithm and color-management behavior are not exact replicas of digiKam.

## Project identity and author

The app's public name is **Print Creator**, the executable is `print-creator`, and the desktop/Flatpak ID is `org.neural_smith.PrintCreator`. The underscore is intentional: Flatpak does not permit a hyphen in a middle application-ID segment. The publisher/organization name remains **neural-smith**. This rename changes the installed application ID, so an older installation may need to be uninstalled separately; the new launcher does not replace the old ID automatically.

The publisher is configured in **one location**, [`Cargo.toml`](Cargo.toml), under `[package]`:

```toml
authors = ["neural-smith"]
```

This is the actual publisher name, not a placeholder. Update it in `Cargo.toml` if the organization name ever changes. The GUI's **About** dialog and CLI help use Cargo's `CARGO_PKG_AUTHORS` value at compile time. The build scripts generate the desktop file's `X-PrintCreator-Author` and AppStream's `developer_name` from the same value, via:

```bash
python3 scripts/generate-metadata.py --output-dir dist/desktop
```

Only `.desktop.in` and `.metainfo.xml.in` are source-controlled; the generated files are never edited manually. This requires Python 3.11 or newer, with no extra Python dependencies.

**Important when upgrading from the old package name:** Cargo's lockfile needs to record the renamed local package. In an existing checkout, run `cargo check` once (without `--locked`), review the updated `Cargo.lock`, and commit it. This source archive does not include the original lockfile, which existed only in your local checkout. Do not tag a release until the lockfile is committed; CI checks this.

## Features

| Area | Capabilities |
| --- | --- |
| Photo import | JPEG, PNG, TIFF, WebP and BMP; multi-file and folder import; drag-and-drop of local files and folders |
| Print layouts | A-series and Letter paper, custom dimensions, grids, adaptive layouts, **63 bundled digiKam-style templates**, XML import/export |
| Photo placement | Fill/crop or fit/no-crop, global crop override, rotation, EXIF auto-orientation, flips, crop position, zoom and duplicate copies |
| Page layout | Measurements in **centimeters**, margins, gaps, borderless template adjustment, printable-area guides |
| Captions | Filename, date, comments and custom text; configurable type size and appearance |
| Outputs | Multipage PDF, page images (PNG/JPEG/TIFF), CUPS printer or exported pages for GIMP |
| Image fidelity | Standard DPI-based PDF export or **lossless PDF embedding at source pixel resolution**, without another JPEG compression pass |
| Workflow | JSON job save/load; **Copy CLI command** reproduces the complete current GUI job |
| Interface | Catppuccin **Mocha Sapphire** / **Latte Sapphire**, automatic KDE/system appearance following, responsive preview, output directory defaults to XDG Documents |
| Distribution | Linux Flatpak, AppImage, local build, UPX-packed distributables and tag-triggered GitHub Releases |

Page-edge and layout-frame outlines are **preview guides only**. They do not appear in exported pages. A borderless preset removes template-defined margins/gutters; physical printers can still have non-printable areas. Fitting an image without cropping may leave white space if its aspect ratio differs from the frame.

## Quick start

With a recent Rust toolchain installed:

```bash
cargo run --release
```

For an optimized standalone executable:

```bash
cargo build --release --locked
./target/release/print-creator
```

To install its desktop entry and icon for KDE Plasma (or another XDG desktop):

```bash
./scripts/install-desktop.sh
```

This installs the launcher under `~/.local/share/applications/`. It points at the compiled executable, so run the installer again if you move the project.

**First checkout:** Keep `Cargo.lock` in Git. If the source copy does not contain it, run `cargo generate-lockfile` and commit it before tagging a release. `Cargo.lock` is intentionally not ignored. After renaming the package, refresh the existing lockfile once using `cargo check` before switching back to `--locked`.

## Desktop workflow

1. Add photos using **Add images**, **Add folder**, or drag-and-drop. Existing photos stay in the job when more are added.
2. Select the paper size and orientation, then choose a grid, adaptive layout or bundled/imported digiKam template.
3. Choose **Do not crop** to show entire photos, or **Fill / crop** to fill photo frames. Optional **Borderless template** removes template margins and gaps. Explicitly rotating a photo by 90°, 180° or 270° takes priority over template auto-rotation; with no manual rotation, the original automatic long-edge matching is retained.
4. Use the preview to inspect physical paper and layout boundaries (all physical measurements are displayed in **cm**).
5. Add captions, adjust individual photos and select the output format. File dialogs default to your XDG **Documents** folder, including localized paths such as `Documentos`.
6. Select **Preserve source image pixels** for lossless PDF embedding, if needed; then export or print.

The output location is remembered separately for PDF and page-image exports. Rendering of previews and folder discovery happens off the UI thread. The window's sidebar has bounded width and scrollable controls.

**Theme:** Use the appearance selector to follow the desktop or force Catppuccin Mocha (dark) / Latte (light), both with Sapphire accents. KDE theme changes can be detected while the app is open. Automatic detection uses the XDG portal where possible and KDE-specific fallback configuration otherwise.

**KDE Wayland drag-and-drop:** This application's `eframe`/`winit` version may not receive drag-and-drop events on native Wayland. When XWayland is available the application uses it for local file dropping from Dolphin. Set `RPC_NATIVE_WAYLAND=1` to prefer native Wayland instead (drag-and-drop may not function there).

## Command-line interface

The binary supports `gui`, `quick`, `render` and `list-templates`. Use `--help` on each subcommand for the complete options.

```bash
# Generate an A4, four-photo PDF, keeping entire images visible.
print-creator quick ~/Pictures \
  --page a4 --rows 2 --columns 2 \
  --margin-cm 0.5 --gap-cm 0.3 --no-crop \
  --output "$HOME/Documents/print.pdf"

# Use a digiKam-style photo template without outer whitespace.
print-creator quick photo1.jpg photo2.jpg \
  --template-xml assets/templates.xml \
  --template-name '6_photos_3.5x4cm' \
  --borderless-templates --preserve-quality \
  --output passport.pdf

# Render a saved JSON job, optionally overriding its output path.
print-creator render --job examples/job.json --output layout.pdf

# Render JSON supplied on stdin (used by the GUI's Copy CLI command).
print-creator render --job - < examples/job.json

# List bundled templates, including sizes in centimeters.
print-creator list-templates

# Build a CLI-only executable, without the graphical dependencies.
cargo build --release --locked --no-default-features
```

**Copy CLI command:** The GUI serializes the current print job into JSON, then copies a shell command that sends it to `render --job -` via a quoted heredoc. It preserves per-photo adjustments and output preferences. Photos still need to be present at their referenced paths. Review any pasted command before sharing: it may contain private filesystem paths and metadata.

**JSON compatibility:** Existing projects retain internal `*_mm` fields and caption point sizes for backward compatibility, even though the interface and physical CLI inputs are in centimeters. Legacy `--margin-mm` and `--gap-mm` options are supported but hidden.

## Building Linux packages

For Linux builds, install Rust/Cargo, a compiler, `pkg-config`, `curl`, Flatpak and `flatpak-builder`, plus the X11/Wayland and system-library development packages required by egui. A working Flathub remote is needed for the Flatpak SDK/runtime. For example:

```bash
flatpak remote-add --user --if-not-exists flathub \
  https://dl.flathub.org/repo/flathub.flatpakrepo

./scripts/build-all.sh            # Flatpak + AppImage
./scripts/build-all.sh flatpak    # Flatpak only
./scripts/build-all.sh appimage   # AppImage only
./scripts/build-all.sh standalone # Single UPX-compressed Linux executable
```

Artifacts are created under `dist/packages/`, e.g.:

```text
PrintCreator-0.5.0-x86_64.flatpak
PrintCreator-0.5.0-x86_64.AppImage
PrintCreator-0.5.0-x86_64-linux
```

The combined Linux packaging script compiles the native executable **once on the host**, then stages the same binary independently for Flatpak and AppImage. The Flatpak manifest only installs prebuilt files; it does not invoke Cargo. Separate `flatpak` and `appimage` script invocations compile on demand, unless `RPC_NATIVE_BINARY` points to an existing executable. AppImage packaging uses `linuxdeploy` for library collection and `appimagetool` for the final image.

**Compatibility:** Reusing a host-built executable means the build runner must provide an ELF/GLIBC baseline and library dependencies compatible with `org.freedesktop.Platform//25.08` as well as the intended AppImage hosts. Run `RPC_FLATPAK_SMOKE_TEST=1 bash scripts/build-all.sh` in an environment that permits Flatpak installation/execution to check the packaged application inside the *actual runtime*. The normal build does not perform this runtime check automatically. For maximum portability, building natively against the Flatpak SDK remains preferable, but costs an extra compile. The WebAssembly target must still compile independently. The standalone ELF (`*-linux`) is also published directly on GitHub Releases, with `UPX --best --lzma` applied to a separate copy of the shared native binary. Download it, run `chmod +x` and execute it on a compatible Linux distribution; it is **not** a fully portable/static binary. **Both** package builders apply `upx --best --lzma` to the application executable **before packaging**; the uncompressed Cargo binary in `target/release/` is left alone. UPX can affect security scanners or hardened systems, and the final package size is not guaranteed.

The default Flatpak permissions include the home directory (for photo input/output), display access and the CUPS socket. The Flatpak sandbox may prevent launching a host-installed GIMP. The AppImage compatibility floor depends on the libc and libraries of the machine used to build it.

### Install packages

```bash
flatpak install --user dist/packages/PrintCreator-0.5.0-x86_64.flatpak
flatpak run org.neural_smith.PrintCreator

chmod +x dist/packages/PrintCreator-0.5.0-x86_64.AppImage
./dist/packages/PrintCreator-0.5.0-x86_64.AppImage

chmod +x dist/packages/PrintCreator-0.5.0-x86_64-linux
./dist/packages/PrintCreator-0.5.0-x86_64-linux --version
PrintCreator-0.5.0-x86_64-linux
```

## CI, signed provenance and immutable GitHub Releases

The workflows in [`.github/workflows/`](.github/workflows/) have two roles:

- **`rust.yml`** runs project checks and builds the Linux packages. On tagged releases, it creates `SHA256SUMS` and uses `actions/attest@v4` to sign **SLSA build-provenance attestations** for the Flatpak, AppImage, standalone Linux binary and checksum file, with GitHub OIDC/Sigstore.
- **`release.yml`** runs on version tags (`v*`), validates that the tag matches `Cargo.toml`, requires a committed `Cargo.lock`, reuses `rust.yml`, checks the uploaded build artifacts against their checksums, creates a **draft release**, uploads everything, validates the draft asset list and **then publishes** it.

**Before your first release, enable GitHub Immutable Releases:**

1. Open your repository's **Settings → Releases** section and turn on **Enable release immutability**. This is a repository setting, not a setting that this workflow can implicitly enable.
2. Commit the generated `Cargo.lock` alongside the application sources, `.gitignore`, workflows and packaging files.
3. Confirm the normal CI passes. Match the version in `Cargo.toml` with a new Git tag, then push it:

```bash
git add .gitignore README.md Cargo.toml Cargo.lock src assets packaging scripts tests .github/workflows
git commit -m "refactor: rename application and centralize author metadata"
git tag -a v0.5.0 -m "Print Creator v0.5.0"
git push origin HEAD
git push origin v0.5.0
```

If `v0.5.0` has already been published, **bump the application version and use a new tag**. Do not retag or attempt to replace assets on an immutable release.

Once immutability is enabled, the published release's assets are locked and its tag is protected. GitHub also automatically creates a **signed release attestation** tying the tag, commit and final assets together. This release attestation is distinct from the **build provenance** generated by `actions/attest`. If a draft upload or verification fails, the release remains a draft instead of publishing an incomplete immutable release.

### Verify a published release

Use a recent [GitHub CLI](https://cli.github.com/):

```bash
# Verify that a published release has GitHub's signed release attestation.
gh release verify v0.5.0 --repo OWNER/REPOSITORY

# Download packages and SHA256SUMS.
gh release download v0.5.0 --repo OWNER/REPOSITORY --dir release-files
(cd release-files && sha256sum --check SHA256SUMS)

# Verify individual packages against the immutable release record.
gh release verify-asset v0.5.0 release-files/PrintCreator-0.5.0-x86_64.AppImage \
  --repo OWNER/REPOSITORY

# Verify the separate build provenance signature.
gh attestation verify release-files/PrintCreator-0.5.0-x86_64.AppImage \
  --repo OWNER/REPOSITORY
```

Replace `OWNER/REPOSITORY` with your GitHub repository. The workflow uses the built-in `GITHUB_TOKEN`; it requires `contents: write` to publish, and `id-token`, `attestations` and `artifact-metadata` permissions to generate provenance. For GitHub Free/Pro/Team, build attestations require a **public** repository; private-repository attestations have additional plan restrictions.

References: [GitHub Immutable Releases](https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases), [build provenance attestations](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations), and [release verification](https://docs.github.com/en/code-security/how-tos/secure-your-supply-chain/secure-your-dependencies/verify-release-integrity).

## Optional self-hosted Flatpak auto-updates through GitHub Pages

You can host **a real, GPG-signed Flatpak/OSTree repository** on GitHub Pages for this project. It is not a fake `.flatpakrepo` file pointing to versioned GitHub Release bundles: the Pages site contains the actual OSTree objects, signed summary, and an installable `.flatpakref`. Installation from this repository registers the remote, so `flatpak update` and KDE Discover can find future releases. Automatic *installation* of updates depends on the user's software-center settings.

The publication job is **opt-in**, to preserve ordinary immutable GitHub Releases until Pages and signing are configured. When enabled, version-tag CI builds Linux packages once, verifies checksums, transfers the **already-built Flatpak OSTree ref** into a persistent signed OSTree repository (no `.flatpak` bundle re-import), and deploys it to Pages. The immutable GitHub Release is independent of the optional Pages deployment, so a Pages failure does not block release artifacts. The OSTree history is persisted in the `gh-pages` branch, with older commits pruned to limit growth. The update repository remains intentionally **mutable** so clients can receive updates; published GitHub Release assets remain **immutable**.

### One-time setup

1. In your GitHub repository, open **Settings → Pages** and select **GitHub Actions** as the publishing source. The workflow will initialize the `gh-pages` branch automatically. Allow tag deployments matching `v*` in the `github-pages` environment's deployment branch/tag protection rules, if configured.
   Under **Settings → Environments → github-pages → Deployment branches and tags**, select **Selected branches and tags**, add a rule of type **Tag** with the pattern `v*`, and save it. GitHub Pages frequently defaults to permitting only the default branch; release-tag runs will otherwise fail with *Tag is not allowed to deploy to github-pages due to environment protection rules*. This is a GitHub repository setting and cannot be overridden in workflow YAML.
   The optional Pages deployment is independent of GitHub Release publication: if Pages fails, the signed AppImage and Flatpak bundle release can still publish, but `flatpak update` will not see the new version until Pages deployment succeeds.
2. Configure a **dedicated Flatpak signing key**, not your personal/master PGP key. **If you already deployed a signed Flatpak repository, keep using its existing key**. For example, the existing key can be re-uploaded without Base64:

   ```bash
   # Use the original GNUPGHOME if the key was generated in an isolated keyring.
   gpg --list-secret-keys --keyid-format LONG
   gpg --armor --export-secret-keys EXISTING_KEY_ID | \
     gh secret set FLATPAK_SIGNING_KEY --repo neural-smith/print-creator
   gh variable set FLATPAK_PAGES_ENABLED --repo neural-smith/print-creator --body true
   ```

   Replace `EXISTING_KEY_ID` with the ID or fingerprint of **your original private signing key**. If it is not in the current keyring, import its backup first (`gpg --import /secure/path/signing-backup.asc`) or set `GNUPGHOME` to the original keyring directory. The previously reported `base64: invalid input` error does **not** mean that the key needs to be rotated.

   If no repository has been signed yet, or you deliberately intend to initialize a **new** trust chain, create a new CI-only key in a persistent isolated keyring:

   ```bash
   export GNUPGHOME="${XDG_DATA_HOME:-$HOME/.local/share}/print-creator/gnupg"
   install -d -m 700 "$GNUPGHOME"
   gpg --batch --pinentry-mode loopback --passphrase '' \
     --quick-generate-key 'neural-smith Print Creator Flatpak Signing <change-me@example.org>' ed25519 sign 2y
   FINGERPRINT="$(gpg --batch --with-colons --list-secret-keys | awk -F: '$1 == "fpr" {print $10; exit}')"
   test -n "$FINGERPRINT" || { echo 'Signing key creation failed' >&2; exit 1; }
   gpg --armor --export-secret-keys "$FINGERPRINT" | \
     gh secret set FLATPAK_SIGNING_KEY --repo neural-smith/print-creator
   gh variable set FLATPAK_PAGES_ENABLED --repo neural-smith/print-creator --body true
   # Make an encrypted backup outside the Git repository; save the passphrase securely.
   gpg --armor --export-secret-keys "$FINGERPRINT" | \
     gpg --symmetric --cipher-algo AES256 --output "$HOME/print-creator-flatpak-signing.asc.gpg"
   ```

   Replace the placeholder email before generating the key. The preferred GitHub Actions secret is `FLATPAK_SIGNING_KEY` containing **ASCII-armored private key text** (no Base64); the previous `FLATPAK_SIGNING_KEY_B64` secret is supported for compatibility. The workflow verifies noninteractive signing. Never commit the secret or a private-key backup. Protect repository release permissions, and keep a separate encrypted backup because an unexpected key change invalidates the trust chain for existing Flatpak clients. The workflow also refuses key rotation when a previous signing fingerprint exists in the published repository. A passphrase-free key is a trade-off for unattended CI signing.
3. By default the repository URL is computed as `https://OWNER.github.io/REPO/` (for example, `https://neural-smith.github.io/print-creator/`). If you configured a **custom Pages domain**, set the repository variable `FLATPAK_PAGES_URL` to the final HTTPS URL *before the first published release*. Changing the URL later can disrupt clients.
4. Confirm that `Cargo.toml` and all Flatpak/desktop/icon files use `org.neural_smith.PrintCreator`. The dash in the public publisher name is normalized to underscore in the Flatpak ID; **`org.neural-smith.PrintCreator` is invalid**. Push a **new version tag** (`v0.5.0` if still unused) and allow the release workflow to finish. The Pages site will contain `index.flatpakrepo`, `org.neural_smith.PrintCreator.flatpakref`, a landing page, and the signed OSTree repository.

### Install once, update normally

Replace `OWNER` and `REPO` with the GitHub organization (`neural-smith`) and repository name, for example `neural-smith/print-creator`:

```bash
flatpak install --user https://OWNER.github.io/REPO/org.neural_smith.PrintCreator.flatpakref
flatpak update --user org.neural_smith.PrintCreator
```

Alternatively, add the remote first:

```bash
flatpak remote-add --user --if-not-exists print-creator \
  https://OWNER.github.io/REPO/index.flatpakrepo
flatpak install --user print-creator org.neural_smith.PrintCreator
```

**Important:** Installing the standalone `.flatpak` file downloaded from a GitHub Release is not the same as installing from this remote; use the `.flatpakref` or `.flatpakrepo` for normal automatic update discovery. Existing bundle-only installs may need to be reinstalled from the signed remote.

### Hosting and security limits

- GitHub Pages is suitable for a **small-project, low-traffic** Flatpak repository. GitHub specifies a **1 GB published-site limit**, **100 GB/month soft bandwidth limit**, and possible rate limits; Flatpak clients may download many OSTree objects. The workflow refuses to deploy over 950 MB. A CDN/OCI distribution or Flathub is a better option for substantial traffic.
- `gh-pages` stores a compacted OSTree repository across releases; its **Git history still grows** and counts toward normal repository storage limits. Static deltas improve update performance but consume additional space. Avoid force-pushing or deleting the published branch/objects.
- OSTree commit and summary GPG signing is **required**. The workflow checks the public-key fingerprint against previous deployments and refuses unplanned key rotation. GitHub build attestations and immutable GitHub Releases are **separate** from Flatpak's GPG trust chain.
- Only Linux x86_64 is built by the current CI; the Pages repository will not supply ARM64 packages until CI supports that architecture.

A local maintainer can also update a **local** signed OSTree repo with `FLATPAK_GPG_KEY_ID=... bash scripts/update-flatpak-pages.sh DIRECTORY dist/flatpak-repo https://example.org/repo/`. Pass the **built OSTree repository**, not the `.flatpak` file. The script recreates the empty `refs/remotes` directory (not preserved by Git) on each run before generating the signed summary. Publishing that directory to an HTTPS static host is a separate step.

References: [Flatpak repository hosting](https://docs.flatpak.org/en/latest/hosting-a-repository.html), [Flatpak single-file bundles](https://docs.flatpak.org/en/latest/single-file-bundles.html), and [GitHub Pages limits](https://docs.github.com/en/pages/getting-started-with-github-pages/github-pages-limits).

### Publishing repair: previously failing consecutive updates

Earlier releases used `flatpak build-import-bundle` to reconstruct the repository
on the Pages runner. That operation failed with `g_str_has_prefix` and an OSTree
null-remote assertion, and subsequently `flatpak build-update-repo` failed with
`opendir(refs/remotes)`. The corrected workflow transfers the Flatpak ref from
`dist/flatpak-repo` (the repository emitted by `flatpak-builder`) using
`ostree pull-local --untrusted`, signs the installed commit via
`flatpak build-sign`, and then signs the summary. The build repository is shipped
between Actions jobs as a private `.tar.gz` artifact, **not** attached to the
public GitHub Release. The publisher recreates empty `refs/remotes` on each
run because Git does not preserve empty directories in `gh-pages`.

Do not delete the existing `gh-pages` branch or rotate the GPG key to fix this
error. Publish a new tag after this workflow patch has been committed. The
build and publication stages do not compile the application a second time.

## Quality checks

```bash
cargo fmt --all -- --check
cargo check --all-targets --all-features
cargo test --all-features
cargo check --no-default-features
```

Optional tools used by particular features include `lp`/CUPS for printing, `gimp` for launching an image editor, and system-provided font support. The lossless PDF option does not restore lost JPEG detail and does not implement complete ICC print color management; for precise prints, verify the printer's color handling and hardware margins.

## digiKam compatibility and license

The built-in XML templates use digiKam-style print-layout fields. You can import other templates (including original digiKam `templates.xml`) or export compatible XML; different versions and layout algorithms may not render identically. See [digiKam Print Creator documentation](https://docs.digikam.org/en/post_processing/print_creator.html).

The application's Rust GUI, layout and rendering implementations are independent of digiKam's C++ implementation. The bundled template collection includes both adapted sample coordinates and original layouts. This project is licensed under **GPL-2.0-or-later**; see [LICENSE](LICENSE).

### Shell script permissions

On Linux, `scripts/*.sh` are tracked as executable (`100755`). If a ZIP extraction
loses those permissions, restore them before committing:

```bash
chmod +x scripts/*.sh
git add scripts/
```

Packaging workflows invoke the scripts through Bash as a second safeguard,
but CI also checks the committed executable bits.


## Browser version (WebAssembly)

The same Rust layout, template, EXIF-orientation, raster renderer, and PDF
writers power the browser edition. Its egui interface is adapted for the web:
files are imported into **tab-local memory**, never uploaded to a server.

### Build locally

Requires stable Rust, `rustup target add wasm32-unknown-unknown` and
[`wasm-pack`](https://rustwasm.github.io/wasm-pack/installer/).
After adding the new WASM dependencies, **regenerate and commit `Cargo.lock`**
with `cargo check` (without `--locked`) before publishing a new version.

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --locked
cargo check  # refresh the lockfile for the web dependencies
bash scripts/build-web.sh
python3 -m http.server 8080 --directory dist/web
# Open http://localhost:8080/app/
```

Do not open `index.html` with `file://`; the browser must serve Wasm with HTTP.
The production build is at `dist/web/app/` and can be hosted on any static
HTTPS server, under any path. The offline-capable PWA uses relative asset URLs.

### GitHub Pages alongside the signed Flatpak repository

The independent WebAssembly CI workflow builds a `print-creator-web` artifact
on branches and version tags. When `FLATPAK_PAGES_ENABLED=true`, the existing release-time
Pages workflow restores the persistent signed OSTree repository from `gh-pages`
and places the browser editor **only under `/app/`** before redeploying Pages:

- Browser editor: `https://neural-smith.github.io/print-creator/app/`
- Flatpak repository and `.flatpakref`: existing URLs at the site root.

The web artifact does **not** overwrite the Flatpak objects or signing keys.
Pages requires `github-pages` environment permissions for release tags (`v*`).
A web build failure does not block immutable Linux GitHub Releases, but will
block the optional combined Pages deployment.

### Browser behavior and limitations

- Import JPEG, PNG, WebP, TIFF and BMP through the picker, drag-and-drop, or
  folders dropped by supported browsers. Images are held in memory, not uploaded.
- Crop, rotate, flip, import built-in digiKam layouts and custom XML layouts, adjust paper sizes in cm,
  and export either high-quality native-pixel PDF or rasterized PDF.
- `Save project` downloads a JSON bundle with the **embedded source images**;
  reopening that bundle restores edits and photos across browser sessions.
  Desktop JSON job files referencing local paths are not portable to browsers.
- The browser cannot access CUPS, GIMP, RAW `dcraw`, native filesystem paths,
  or arbitrary OS font files. Web captions use a compact bitmap font, whereas
  desktop captions use installed system fonts.
- A single image is limited to 80 MiB, total imported images to 350 MiB, and
  exports to 100 pages. High-resolution lossless exports can still use
  substantial browser memory and may temporarily pause the interface.
- No backend service, API key, or third-party image upload is necessary.
  The service worker caches app assets for offline use after first load.

### wasm-opt compatibility (WebAssembly builds)

The bundled `wasm-opt` (Binaryen) may reject bulk-memory and nontrapping
float-to-int instructions emitted by a newer Rust compiler. Print Creator
disables only the `wasm-opt` postprocessing stage for `wasm-pack` release
builds through `[package.metadata.wasm-pack.profile.release]`. Rust's release
optimizations, including LTO, remain enabled. The `.wasm` file may be larger.

Build again using `bash scripts/build-web.sh`. This is not a reason to
disable Rust bulk-memory features or change your existing CLI flags.
