#!/usr/bin/env bash
# Import a dedicated Flatpak signing key from a GitHub Actions repository secret.
# Prefer FLATPAK_SIGNING_KEY (ASCII-armored). The old *_B64 secret is supported.
set -euo pipefail

: "${RUNNER_TEMP:?RUNNER_TEMP must be defined}"
: "${GITHUB_ENV:?GITHUB_ENV must be defined}"

if [[ -z "${FLATPAK_SIGNING_KEY:-}" && -z "${FLATPAK_SIGNING_KEY_B64:-}" ]]; then
  echo 'Set the FLATPAK_SIGNING_KEY Actions secret (ASCII-armored private key).' >&2
  echo 'The legacy FLATPAK_SIGNING_KEY_B64 secret is also supported.' >&2
  exit 1
fi

export GNUPGHOME="${RUNNER_TEMP}/print-creator-gnupg"
install -d -m 700 "$GNUPGHOME"
umask 077
key_file="$(mktemp "${RUNNER_TEMP}/print-creator-key.XXXXXXXX")"
trap 'rm -f "$key_file"' EXIT

if [[ -n "${FLATPAK_SIGNING_KEY:-}" ]]; then
  if [[ "$FLATPAK_SIGNING_KEY" != *'-----BEGIN PGP PRIVATE KEY BLOCK-----'* ]]; then
    echo 'FLATPAK_SIGNING_KEY must contain the ASCII-armored private key.' >&2
    echo 'Re-upload with: gpg --armor --export-secret-keys "$FINGERPRINT" | gh secret set FLATPAK_SIGNING_KEY --repo OWNER/REPO' >&2
    exit 1
  fi
  printf '%s\n' "$FLATPAK_SIGNING_KEY" > "$key_file"
else
  # Decode completely *before* importing. An invalid Base64 value must not
  # import a valid prefix and then fail the pipe, as the previous workflow did.
  # Permit copy/paste line breaks, but reject all other invalid characters.
  if ! printf '%s' "$FLATPAK_SIGNING_KEY_B64" | tr -d '[:space:]' | base64 --decode > "$key_file"; then
    echo 'FLATPAK_SIGNING_KEY_B64 contains invalid Base64 data.' >&2
    echo 'Re-upload your existing key as FLATPAK_SIGNING_KEY (ASCII-armored) instead.' >&2
    exit 1
  fi
fi

if ! gpg --batch --import "$key_file"; then
  echo 'GPG could not import the Flatpak signing key.' >&2
  exit 1
fi
fingerprint="$(gpg --batch --with-colons --list-secret-keys | awk -F: '$1 == "fpr" { print $10; exit }')"
if [[ ! "$fingerprint" =~ ^[A-Fa-f0-9]{40}$ ]]; then
  echo 'No usable 40-character GPG secret key fingerprint was imported.' >&2
  exit 1
fi

# Confirm that the key can sign unattended before attempting an OSTree update.
signature_file="$(mktemp "${RUNNER_TEMP}/print-creator-signature.XXXXXXXX")"
trap 'rm -f "$key_file" "$signature_file"' EXIT
if ! printf 'Print Creator Flatpak signing verification\n' | \
  gpg --batch --yes --pinentry-mode loopback --local-user "$fingerprint" \
  --output "$signature_file" --detach-sign; then
  echo 'Imported key cannot sign noninteractively. Use a dedicated key without a passphrase.' >&2
  exit 1
fi

printf 'GNUPGHOME=%s\n' "$GNUPGHOME" >> "$GITHUB_ENV"
printf 'FLATPAK_GPG_KEY_ID=%s\n' "$fingerprint" >> "$GITHUB_ENV"
echo 'Flatpak signing key imported and signing verified.'
