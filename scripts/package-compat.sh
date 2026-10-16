#!/usr/bin/env bash
# Package application-owned runtime assets next to the executable.
set -euo pipefail
runtime_source="$1"
adapter_source="$(cd "$(dirname "${BASH_SOURCE[0]}")/../compat/perl" && pwd)"
runtime_destination="$2/compat-runtime"
[[ -f "$adapter_source/worker.pl" && -d "$runtime_source/libraries/lib/perl5" ]] || {
  printf '%s\n' 'Compatibility runtime assets are missing.' >&2
  exit 1
}
mkdir -p "$runtime_destination"
# Dependencies can retain read-only modes from their original packages.
mkdir -p "$runtime_destination/adapter"
cp -af "$adapter_source/." "$runtime_destination/adapter/"
cp -af "$runtime_source/libraries" "$runtime_destination/"
