#!/usr/bin/env bash
# build-pkgs-repo.sh <packages-dir> <out-dir>
#
# Builds the signed APT and DNF indices served at pkgs.chilpa.app.
#
#   <packages-dir>  flat directory with every retained chilpa_*.deb and chilpa-*.rpm
#   <out-dir>       static site root for the Worker (index files only, no packages)
#
# Environment:
#   GNUPGHOME         keyring holding the packages signing secret key
#   GPG_PASSPHRASE    optional; the key may have none
#   BASE_URL          public origin, default https://pkgs.chilpa.app
#
# Needs apt-ftparchive (apt-utils), createrepo_c, gnupg.
#
# The packages themselves never enter <out-dir>: APT Filename entries point
# under apt/pool/ and RPM locations under dnf/<arch>/Packages/, and the Worker
# answers those paths with a 302 to the GitHub release asset.
set -euo pipefail

pkgs="$(realpath "${1:?packages dir}")"
mkdir -p "${2:?out dir}"
out="$(realpath "$2")"
base_url="${BASE_URL:-https://pkgs.chilpa.app}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

fingerprint="$(gpg --list-secret-keys --with-colons | awk -F: '/^fpr/ {print $10; exit}')"
[[ -n "$fingerprint" ]] || { echo "no signing key in GNUPGHOME" >&2; exit 1; }

gpg_sign() { # gpg_sign <mode...> -- <output> <input>
  local args=() ; while [[ "$1" != -- ]]; do args+=("$1"); shift; done; shift
  local pass=()
  if [[ -n "${GPG_PASSPHRASE:-}" ]]; then
    pass=(--pinentry-mode loopback --passphrase-fd 3)
  fi
  gpg --batch --yes --local-user "$fingerprint" "${pass[@]}" "${args[@]}" --output "$1" "$2" \
    3< <(printf '%s' "${GPG_PASSPHRASE:-}")
}

shopt -s nullglob
debs=("$pkgs"/chilpa_*_{amd64,arm64}.deb)
rpms=("$pkgs"/chilpa-*.{x86_64,aarch64}.rpm)
(( ${#debs[@]} > 0 && ${#rpms[@]} > 0 )) || { echo "no packages found in $pkgs" >&2; exit 1; }

# ---- APT -------------------------------------------------------------------
apt_root="$work/apt"
pool="$apt_root/pool/main/c/chilpa"
mkdir -p "$pool"
cp "${debs[@]}" "$pool/"
dists="$apt_root/dists/stable"
for arch in amd64 arm64; do
  mkdir -p "$dists/main/binary-$arch"
  (cd "$apt_root" && apt-ftparchive --arch "$arch" packages pool > "dists/stable/main/binary-$arch/Packages")
  gzip -9nk "$dists/main/binary-$arch/Packages"
  xz -9k "$dists/main/binary-$arch/Packages"
done
(
  cd "$dists"
  apt-ftparchive \
    -o APT::FTPArchive::Release::Origin=Chilpa \
    -o APT::FTPArchive::Release::Label=Chilpa \
    -o APT::FTPArchive::Release::Suite=stable \
    -o APT::FTPArchive::Release::Codename=stable \
    -o APT::FTPArchive::Release::Architectures="amd64 arm64" \
    -o APT::FTPArchive::Release::Components=main \
    -o APT::FTPArchive::Release::Description="Chilpa package repository" \
    release . > "$work/Release"
)
mv "$work/Release" "$dists/Release"
gpg_sign --armor --detach-sign -- "$dists/Release.gpg" "$dists/Release"
gpg_sign --clearsign -- "$dists/InRelease" "$dists/Release"
# Packages are redirected by the Worker; only the indices ship.
rm -rf "$apt_root/pool"
mkdir -p "$out/apt"
cp -a "$apt_root/dists" "$out/apt/"

# ---- DNF -------------------------------------------------------------------
for arch in x86_64 aarch64; do
  repo="$work/dnf/$arch"
  mkdir -p "$repo/Packages"
  files=("$pkgs"/chilpa-*."$arch".rpm)
  (( ${#files[@]} > 0 )) || { echo "no $arch rpm" >&2; exit 1; }
  cp "${files[@]}" "$repo/Packages/"
  createrepo_c --quiet --general-compress-type gz "$repo"
  gpg_sign --armor --detach-sign -- "$repo/repodata/repomd.xml.asc" "$repo/repodata/repomd.xml"
  mkdir -p "$out/dnf/$arch"
  cp -a "$repo/repodata" "$out/dnf/$arch/"
done

# ---- Key and user-facing files ----------------------------------------------
gpg --export "$fingerprint" > "$out/chilpa.gpg"
gpg --export --armor "$fingerprint" > "$out/chilpa.asc"
cat > "$out/chilpa.repo" <<REPO
[chilpa]
name=Chilpa
baseurl=${base_url}/dnf/\$basearch
enabled=1
gpgcheck=0
repo_gpgcheck=1
gpgkey=${base_url}/chilpa.asc
REPO
echo "built repositories for ${#debs[@]} deb and ${#rpms[@]} rpm files into $out (key $fingerprint)"
