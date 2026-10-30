#!/usr/bin/env bash
# stamp-aur.sh <PKGBUILD> <version> <sha256-x86_64> <sha256-aarch64>
# Stamps pkgver, pkgrel=1 and both source checksums, then checks that each
# value landed exactly once.
set -euo pipefail

file="${1:?PKGBUILD}"; version="${2:?version}"; x86="${3:?x86_64 sha256}"; arm="${4:?aarch64 sha256}"
for sum in "$x86" "$arm"; do
  [[ "$sum" =~ ^[0-9a-f]{64}$ ]] || { echo "bad sha256: $sum" >&2; exit 1; }
done
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "bad version: $version" >&2; exit 1; }

sed -i \
  -e "s/^pkgver=.*/pkgver=${version}/" \
  -e "s/^pkgrel=.*/pkgrel=1/" \
  -e "s/^sha256sums_x86_64=.*/sha256sums_x86_64=('${x86}')/" \
  -e "s/^sha256sums_aarch64=.*/sha256sums_aarch64=('${arm}')/" \
  "$file"

for expected in "pkgver=${version}" "pkgrel=1" "sha256sums_x86_64=('${x86}')" "sha256sums_aarch64=('${arm}')"; do
  [[ "$(grep -cxF "$expected" "$file")" == 1 ]] || { echo "stamp failed: ${expected}" >&2; exit 1; }
done
