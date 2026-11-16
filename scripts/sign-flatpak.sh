#!/usr/bin/env bash
# Signs a Flatpak repository that scripts/package-flatpak.sh built without a
# key, then writes the bundle from it.
#
#   scripts/sign-flatpak.sh --repo DIR --output DIR --gpg-sign KEY-ID
#     --gpg-homedir DIR --repo-url URL --bundle NAME
#     [--app-id ID] [--branch stable]
#
# Every ref's commit gets a signature, the appstream ones included, and the
# summary is signed. No static delta and no summary index is made (see
# scripts/package-flatpak.sh), and any found is removed first. The public key goes to
# OUTPUT/chilpa-flatpak.gpg and into the bundle, which updates from URL.
# Needs flatpak, ostree and gpg; runs nothing from the repository it signs.
set -euo pipefail

repo=''
output=''
key=''
gpg_home=''
repo_url=''
bundle=''
app_id='app.chilpa.Chilpa'
branch='stable'

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo) repo="$2"; shift 2 ;;
    --output) output="$2"; shift 2 ;;
    --gpg-sign) key="$2"; shift 2 ;;
    --gpg-homedir) gpg_home="$2"; shift 2 ;;
    --repo-url) repo_url="$2"; shift 2 ;;
    --bundle) bundle="$2"; shift 2 ;;
    --app-id) app_id="$2"; shift 2 ;;
    --branch) branch="$2"; shift 2 ;;
    *) printf 'unknown option: %s\n' "$1" >&2; exit 2 ;;
  esac
done
for required in repo output key gpg_home repo_url bundle; do
  [[ -n "${!required}" ]] || { printf 'pass --%s\n' "${required//_/-}" >&2; exit 2; }
done
[[ "$repo_url" == https://* ]] || { printf 'the repository URL must be https://: %s\n' "$repo_url" >&2; exit 2; }
[[ -f "$repo/config" && -d "$repo/objects" ]] || { printf 'not an OSTree repository: %s\n' "$repo" >&2; exit 2; }
mkdir -p "$output"

mapfile -t refs < <(ostree --repo="$repo" refs)
[[ ${#refs[@]} -gt 0 ]] || { printf 'the repository holds no ref: %s\n' "$repo" >&2; exit 1; }
printf '%s\n' "${refs[@]}" | grep -qx "app/$app_id/[^/]*/$branch" ||
  { printf 'the repository holds no app/%s/*/%s\n' "$app_id" "$branch" >&2; exit 1; }
for ref in "${refs[@]}"; do
  ostree --repo="$repo" gpg-sign --gpg-homedir="$gpg_home" "$ref" "$key"
done

rm -rf "$repo/deltas" "$repo/delta-indexes" "$repo/summaries"
flatpak build-update-repo --title=Chilpa --default-branch="$branch" --no-summary-index \
  --prune --gpg-sign="$key" --gpg-homedir="$gpg_home" "$repo"
rm -rf "$repo/summaries"

gpg --homedir "$gpg_home" --batch --export "$key" > "$output/chilpa-flatpak.gpg"
[[ -s "$output/chilpa-flatpak.gpg" ]] || { printf 'no public key exported for %s\n' "$key" >&2; exit 1; }
flatpak build-bundle --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo \
  --gpg-keys="$output/chilpa-flatpak.gpg" --repo-url="$repo_url" "$repo" "$output/$bundle" "$app_id" "$branch"
printf 'sign-flatpak: %s, %d refs signed, bundle %s\n' "$repo" "${#refs[@]}" "$output/$bundle"
