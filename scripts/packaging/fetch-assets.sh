#!/usr/bin/env bash
# fetch-assets.sh <tag> <dir> <asset>...
# Downloads release assets from their published URLs, retrying while the
# release is still being populated, and verifies each one against the
# release's SHA256SUMS. Leaves SHA256SUMS in <dir>.
set -euo pipefail

tag="${1:?tag}"; dir="${2:?dir}"; shift 2
repo="${GH_REPO:-vicrodh/chilpa}"
attempts="${FETCH_ATTEMPTS:-60}"
pause="${FETCH_PAUSE:-30}"
base="https://github.com/${repo}/releases/download/${tag}"
mkdir -p "$dir"

get() {
  local name="$1" n
  for ((n = 1; n <= attempts; n++)); do
    if curl -fsSL --retry 3 -o "${dir}/${name}" "${base}/${name}"; then
      return 0
    fi
    echo "Attempt ${n}/${attempts} failed for ${name}" >&2
    sleep "$pause"
  done
  echo "::error::Release asset unavailable: ${name}"
  return 1
}

get SHA256SUMS
for name in "$@"; do
  get "$name"
  expected="$(awk -v f="$name" '$2 == f {print $1}' "${dir}/SHA256SUMS")"
  actual="$(sha256sum "${dir}/${name}" | cut -d' ' -f1)"
  if [[ -z "$expected" || "$expected" != "$actual" ]]; then
    echo "::error::${name}: published file ${actual} does not match SHA256SUMS (${expected:-missing})"
    exit 1
  fi
  echo "${name}: ${actual}"
done
