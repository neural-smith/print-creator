#!/usr/bin/env bash
# Append a prebuilt Flatpak bundle to a persistent, GPG-signed OSTree repository.
# Usage: FLATPAK_GPG_KEY_ID=<fingerprint> ./scripts/update-flatpak-pages.sh REPO_DIR BUNDLE HTTPS_URL
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
# Import the exact Flatpak bundle already built and attested by Linux CI.
# Sign the commit and summary; GPG verification is required for clients.
flatpak build-import-bundle --no-update-summary --update-appstream \
    --gpg-sign="$FINGERPRINT" "$REPO_DIR" "$BUNDLE"
flatpak build-update-repo --title='Print Creator' --gpg-sign="$FINGERPRINT" \
    --generate-static-deltas --prune --prune-depth=2 "$REPO_DIR"
TEMP_PUBLIC="$(mktemp)"
trap 'rm -f "$TEMP_PUBLIC"' EXIT
gpg --batch --export "$FINGERPRINT" > "$TEMP_PUBLIC"
[[ -s "$TEMP_PUBLIC" ]] || { echo 'Failed to export public GPG key' >&2; exit 1; }
python3 "$ROOT/scripts/generate-flatpak-pages.py" --repo "$REPO_DIR" \
    --url "$URL" --gpg-public-key "$TEMP_PUBLIC"
printf '%s\n' "$FINGERPRINT" > "$REPO_DIR/flatpak-signing-fingerprint.txt"
[[ -s "$REPO_DIR/summary" && -s "$REPO_DIR/summary.sig" ]] || {
    echo 'Missing signed OSTree summary: refusing to publish' >&2
    exit 1
}
echo "Updated signed Flatpak repository: $REPO_DIR"
echo "Install URL: ${URL%/}/org.neural_smith.PrintCreator.flatpakref"
