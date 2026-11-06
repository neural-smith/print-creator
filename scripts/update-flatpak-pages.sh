#!/usr/bin/env bash
# Update a persistent GPG-signed Flatpak OSTree repository from an attested .flatpak bundle.
# Usage: FLATPAK_GPG_KEY_ID=<fingerprint> bash scripts/update-flatpak-pages.sh REPO_DIR BUNDLE HTTPS_URL
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if (( $# != 3 )); then
    echo "Usage: FLATPAK_GPG_KEY_ID=FINGERPRINT $0 REPO_DIR BUNDLE HTTPS_REPOSITORY_URL" >&2
    exit 2
fi
REPO_DIR="$(realpath -m -- "$1")"
BUNDLE="$(realpath -- "$2")"
URL="$3"
KEY="${FLATPAK_GPG_KEY_ID:?Set FLATPAK_GPG_KEY_ID to the signing key fingerprint}"
for cmd in flatpak ostree gpg python3; do
    command -v "$cmd" >/dev/null || { echo "Missing required program: $cmd" >&2; exit 1; }
done
[[ -s "$BUNDLE" ]] || { echo "Missing Flatpak bundle: $BUNDLE" >&2; exit 1; }
gpg --batch --list-secret-keys "$KEY" >/dev/null
FINGERPRINT="$(gpg --batch --with-colons --fingerprint "$KEY" | awk -F: '$1 == "fpr" { print $10; exit }')"
[[ -n "$FINGERPRINT" ]] || { echo 'Could not identify signing key' >&2; exit 1; }
mkdir -p "$REPO_DIR"
if [[ -f "$REPO_DIR/flatpak-signing-fingerprint.txt" ]]; then
    [[ "$(cat "$REPO_DIR/flatpak-signing-fingerprint.txt")" == "$FINGERPRINT" ]] || {
        echo 'Signing key changed: existing users will not trust the new key. Refusing unsafe rotation.' >&2
        exit 1
    }
fi
if [[ ! -f "$REPO_DIR/config" ]]; then
    ostree --repo="$REPO_DIR" init --mode=archive-z2
fi

# Work around a Flatpak/OSTree bundle import failure observed when importing the
# second release directly into a persistent repo (GLib assertion in remote lookup).
# A freshly initialized staging repo provides an isolated import, and OSTree's
# pull-local transfers the signed commits + AppStream refs without clearing history.
STAGING_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/print-creator-flatpak-import.XXXXXXXX")"
PUBLIC_KEY="$(mktemp)"
trap 'rm -rf -- "$STAGING_ROOT"; rm -f -- "$PUBLIC_KEY"' EXIT
STAGING_REPO="$STAGING_ROOT/repo"
ostree --repo="$STAGING_REPO" init --mode=archive-z2

# Sign both the imported application commit and AppStream metadata with the
# existing signing key. The temporary repo is *never* the published repository.
flatpak build-import-bundle --no-update-summary --update-appstream \
    --gpg-sign="$FINGERPRINT" --gpg-homedir="${GNUPGHOME:-$HOME/.gnupg}" \
    "$STAGING_REPO" "$BUNDLE"

APP_REF='app/org.neural_smith.PrintCreator/x86_64/master'
IMPORTED_COMMIT="$(ostree --repo="$STAGING_REPO" rev-parse "$APP_REF")"
[[ "$IMPORTED_COMMIT" =~ ^[[:xdigit:]]{64}$ ]] || {
    echo "Unexpected or missing application ref in staged Flatpak: $APP_REF" >&2
    exit 1
}

# Transfer all staged refs: app and generated AppStream branches. Unlike
# re-importing the bundle into the persistent repo, this is an ordinary local
# OSTree pull; it does not touch older published objects or signing identity.
ostree --repo="$REPO_DIR" pull-local --untrusted "$STAGING_REPO"
PUBLISHED_COMMIT="$(ostree --repo="$REPO_DIR" rev-parse "$APP_REF")"
[[ "$PUBLISHED_COMMIT" == "$IMPORTED_COMMIT" ]] || {
    echo 'Imported commit differs from the ref in the persistent repository' >&2
    exit 1
}

# The app and AppStream commits were already signed during staging. Sign the
# summary, generate deltas for upgrades, and retain a bounded history.
flatpak build-update-repo --title='Print Creator' --gpg-sign="$FINGERPRINT" \
    --gpg-homedir="${GNUPGHOME:-$HOME/.gnupg}" \
    --generate-static-deltas --prune --prune-depth=2 "$REPO_DIR"

gpg --batch --export "$FINGERPRINT" > "$PUBLIC_KEY"
[[ -s "$PUBLIC_KEY" ]] || { echo 'Failed to export public GPG key' >&2; exit 1; }
python3 "$ROOT/scripts/generate-flatpak-pages.py" --repo "$REPO_DIR" \
    --url "$URL" --gpg-public-key "$PUBLIC_KEY"
printf '%s\n' "$FINGERPRINT" > "$REPO_DIR/flatpak-signing-fingerprint.txt"
[[ -s "$REPO_DIR/summary" && -s "$REPO_DIR/summary.sig" ]] || {
    echo 'Missing signed OSTree summary: refusing to publish' >&2
    exit 1
}
echo "Updated signed Flatpak repository: $REPO_DIR"
echo "Current $APP_REF: $PUBLISHED_COMMIT"
echo "Install URL: ${URL%/}/org.neural_smith.PrintCreator.flatpakref"
