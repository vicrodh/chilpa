#!/usr/bin/env bash
# Puts the key that signs the Flatpak repository in a keyring of its own,
# usable without a prompt, or makes a throwaway one for a dry run.
#
#   scripts/flatpak-signing-key.sh import HOMEDIR
#     FLATPAK_GPG_PRIVATE_KEY (armored secret key) and FLATPAK_GPG_KEY_ID in
#     the environment; FLATPAK_GPG_PASSPHRASE when the key has one.
#   scripts/flatpak-signing-key.sh throwaway HOMEDIR
#
# Prints the signing key's fingerprint on standard output and nothing else.
# HOMEDIR is created with mode 700 and must not exist yet. The passphrase is
# handed to gpg on a descriptor and preset in the keyring's own agent, which
# OSTree and Flatpak sign through; it is never an argument. Removing the
# key: gpgconf --homedir HOMEDIR --kill all, then rm -rf HOMEDIR.
set -euo pipefail

mode="${1:?import or throwaway}"
home="${2:?HOMEDIR}"
[[ ! -e "$home" ]] || { printf 'the keyring folder already exists: %s\n' "$home" >&2; exit 2; }
install -d -m 700 "$home"
export GNUPGHOME="$home"
# Signing through gpgme asks the agent, which takes a preset passphrase.
printf 'allow-preset-passphrase\n' > "$home/gpg-agent.conf"

case "$mode" in
  import)
    : "${FLATPAK_GPG_PRIVATE_KEY:?FLATPAK_GPG_PRIVATE_KEY is empty}"
    : "${FLATPAK_GPG_KEY_ID:?FLATPAK_GPG_KEY_ID is empty}"
    passphrase="${FLATPAK_GPG_PASSPHRASE:-}"
    pass=()
    [[ -z "$passphrase" ]] || pass=(--pinentry-mode loopback --passphrase-fd 3)
    printf '%s\n' "$FLATPAK_GPG_PRIVATE_KEY" | gpg --batch --quiet "${pass[@]}" --import 3< <(printf '%s' "$passphrase")
    fingerprint="$(gpg --batch --with-colons --list-secret-keys -- "$FLATPAK_GPG_KEY_ID" | awk -F: '$1 == "fpr" { print $10; exit }')"
    [[ -n "$fingerprint" ]] || { printf 'FLATPAK_GPG_KEY_ID names no secret key that was imported\n' >&2; exit 1; }
    if [[ -n "$passphrase" ]]; then
      preset=''
      for candidate in /usr/lib/gnupg/gpg-preset-passphrase /usr/libexec/gpg-preset-passphrase /usr/lib/gnupg2/gpg-preset-passphrase; do
        [[ -x "$candidate" ]] && { preset="$candidate"; break; }
      done
      [[ -n "$preset" ]] || { printf 'gpg-preset-passphrase is not installed\n' >&2; exit 1; }
      gpgconf --launch gpg-agent
      gpg --batch --with-colons --with-keygrip --list-secret-keys -- "$fingerprint" | awk -F: '$1 == "grp" { print $10 }' |
        while IFS= read -r keygrip; do
          "$preset" --preset "$keygrip" < <(printf '%s' "$passphrase")
        done
    fi
    ;;
  throwaway)
    gpg --batch --quiet --passphrase '' --quick-generate-key 'Chilpa Flatpak dry run <dry-run@invalid>' ed25519 sign never 2>/dev/null
    fingerprint="$(gpg --batch --with-colons --list-secret-keys | awk -F: '$1 == "fpr" { print $10; exit }')"
    ;;
  *) printf 'unknown mode: %s\n' "$mode" >&2; exit 2 ;;
esac

# The key signs without a prompt, or the signing would stop at one.
printf 'chilpa\n' | gpg --batch --quiet --local-user "$fingerprint" --detach-sign --output /dev/null 2>/dev/null ||
  { printf 'the key cannot sign without a prompt\n' >&2; exit 1; }
printf '%s\n' "$fingerprint"
