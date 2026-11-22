#!/usr/bin/env bash
# Writes the files packaging/flatpak/app.chilpa.Chilpa.yml reads from
# packaging/flatpak/generated/:
#   cargo-sources.json  the crates of Cargo.lock as flatpak-builder sources,
#                       from flatpak-cargo-generator (org.flatpak.Builder
#                       ships it), so cargo builds offline from them;
#   build-env           CHILPA_BUILD_COMMIT, the commit the About window
#                       names, since the build tree has no .git.
#
#   scripts/flatpak-sources.sh [COMMIT]
#
# COMMIT defaults to HEAD of this checkout and must be 40 hex digits.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="$root/packaging/flatpak/generated"
commit="${1:-$(git -C "$root" rev-parse HEAD)}"
[[ "$commit" =~ ^[0-9a-f]{40}$ ]] || { printf 'not a full commit: %s\n' "$commit" >&2; exit 2; }

if command -v flatpak-cargo-generator >/dev/null 2>&1; then
  generator=(flatpak-cargo-generator)
else
  generator=(flatpak run --command=flatpak-cargo-generator org.flatpak.Builder)
fi

mkdir -p "$out"
"${generator[@]}" "$root/Cargo.lock" -o "$out/cargo-sources.json.partial"
mv "$out/cargo-sources.json.partial" "$out/cargo-sources.json"
printf 'CHILPA_BUILD_COMMIT=%s\n' "$commit" > "$out/build-env"
# The key that signs requests to Chilpa's own services, when this build has one
# (crates/chilpa-api/src/service_signature.rs). generated/ is ignored by git
# and the build bundles no sources, so the key leaves only inside the binary.
if [[ -n "${CHILPA_SERVICE_KEY:-}" ]]; then
  [[ "$CHILPA_SERVICE_KEY" =~ ^[0-9a-fA-F]{64}$ ]] || { printf 'CHILPA_SERVICE_KEY is not 64 hex characters\n' >&2; exit 2; }
  chmod 600 "$out/build-env"
  printf 'CHILPA_SERVICE_KEY=%s\n' "$CHILPA_SERVICE_KEY" >> "$out/build-env"
fi
printf 'flatpak-sources: %s crates, commit %s\n' \
  "$(grep -c '"type": "archive"' "$out/cargo-sources.json")" "$commit"
