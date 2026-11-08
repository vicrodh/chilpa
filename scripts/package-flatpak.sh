#!/usr/bin/env bash
# Builds the Flatpak from packaging/flatpak/app.chilpa.Chilpa.yml with
# org.flatpak.Builder, exports it to an OSTree repository and writes a
# single-file bundle from that repository.
#
#   scripts/package-flatpak.sh --output DIR [--commit SHA] [--branch stable]
#     [--manifest PATH] [--app-id ID] [--bundle NAME] [--state-dir DIR]
#     [--jobs N] [--install-deps]
#     [--gpg-sign KEY-ID --gpg-homedir DIR] [--repo-url URL]
#
# DIR receives repo/ (OSTree, with static deltas) and the bundle, named NAME
# (default app.chilpa.Chilpa.flatpak). With --gpg-sign the commits and the
# summary are signed, the public key is written to DIR/chilpa-flatpak.gpg and
# embedded in the bundle; with --repo-url an installed bundle updates from
# that repository. The state directory holds downloads and the build tree;
# it defaults to ~/.cache/chilpa-flatpak-builder and must lie outside the
# checkout, which the manifest copies whole. --install-deps installs the
# runtime, SDK, Rust extension and base app from Flathub first.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output=''
commit=''
branch='stable'
manifest="$root/packaging/flatpak/app.chilpa.Chilpa.yml"
app_id='app.chilpa.Chilpa'
bundle='app.chilpa.Chilpa.flatpak'
state="${XDG_CACHE_HOME:-$HOME/.cache}/chilpa-flatpak-builder"
jobs="${CARGO_BUILD_JOBS:-2}"
deps=()
key=''
gpg_home=''
repo_url=''

while [[ $# -gt 0 ]]; do
  case "$1" in
    --output) output="$2"; shift 2 ;;
    --commit) commit="$2"; shift 2 ;;
    --branch) branch="$2"; shift 2 ;;
    --manifest) manifest="$2"; shift 2 ;;
    --app-id) app_id="$2"; shift 2 ;;
    --bundle) bundle="$2"; shift 2 ;;
    --state-dir) state="$2"; shift 2 ;;
    --jobs) jobs="$2"; shift 2 ;;
    --install-deps) deps=(--user --install-deps-from=flathub); shift ;;
    --gpg-sign) key="$2"; shift 2 ;;
    --gpg-homedir) gpg_home="$2"; shift 2 ;;
    --repo-url) repo_url="$2"; shift 2 ;;
    *) printf 'unknown option: %s\n' "$1" >&2; exit 2 ;;
  esac
done
[[ -n "$output" ]] || { printf 'pass --output DIR\n' >&2; exit 2; }
[[ "$jobs" =~ ^[1-9][0-9]*$ ]] || { printf 'not a job count: %s\n' "$jobs" >&2; exit 2; }
[[ -z "$key" || -n "$gpg_home" ]] || { printf -- '--gpg-sign needs --gpg-homedir\n' >&2; exit 2; }
mkdir -p "$output" "$state"
output="$(cd "$output" && pwd)"
state="$(cd "$state" && pwd)"
case "$state/" in
  "$root"/*) printf 'the state directory lies inside the checkout: %s\n' "$state" >&2; exit 2 ;;
esac
commit="${commit:-$(git -C "$root" rev-parse HEAD)}"
epoch="${SOURCE_DATE_EPOCH:-$(git -C "$root" log -1 --format=%ct "$commit")}"
signing=()
[[ -z "$key" ]] || signing=(--gpg-sign="$key" --gpg-homedir="$gpg_home")

"$root/scripts/flatpak-sources.sh" "$commit"

# rofiles-fuse needs FUSE, which the builder's own sandbox does not give.
flatpak run org.flatpak.Builder --force-clean --disable-rofiles-fuse \
  --state-dir="$state" --repo="$output/repo" --default-branch="$branch" \
  --override-source-date-epoch="$epoch" --jobs="$jobs" "${deps[@]}" "${signing[@]}" \
  "$state/build" "$manifest"

# Deltas let a client download one file per update instead of every object.
flatpak run --command=flatpak org.flatpak.Builder build-update-repo --title=Chilpa \
  --default-branch="$branch" --generate-static-deltas --static-delta-jobs="$jobs" --prune \
  "${signing[@]}" "$output/repo"

bundling=(--runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo)
if [[ -n "$key" ]]; then
  gpg --homedir "$gpg_home" --batch --export "$key" > "$output/chilpa-flatpak.gpg"
  [[ -s "$output/chilpa-flatpak.gpg" ]] || { printf 'no public key exported for %s\n' "$key" >&2; exit 1; }
  bundling+=(--gpg-keys="$output/chilpa-flatpak.gpg")
fi
[[ -z "$repo_url" ]] || bundling+=(--repo-url="$repo_url")
flatpak build-bundle "${bundling[@]}" "$output/repo" "$output/$bundle" "$app_id" "$branch"
printf 'package-flatpak: %s, %s on branch %s, commit %s\n' "$output/$bundle" "$app_id" "$branch" "$commit"
