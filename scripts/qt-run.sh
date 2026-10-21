#!/usr/bin/env bash
# Build Chilpa's Qt frontend, publish a release artifact, then run the binary
# directly. Release is the default because this is also the owner's smoke-test
# entry point; DEBUG=1 is an explicit developer-only opt-in.
#
# Usage: ./scripts/qt-run.sh [application arguments]
#   NORUN=1    ./scripts/qt-run.sh  # build and publish, but do not launch
#   DEBUG=1    ./scripts/qt-run.sh  # unoptimized developer build; not published
#   NO_AUDIT=1 ./scripts/qt-run.sh  # skip tools/lint/fast
#   JOBS=4     ./scripts/qt-run.sh  # override Cargo's build-job count
#   FORCE=1    ./scripts/qt-run.sh  # ignore the box-wide build guard
#
# CHILPA_ARTIFACT_DIR overrides the release-artifact root. On this workstation
# it defaults to /mnt/build/artifacts/chilpa.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

say() { printf '[qt-run] %s\n' "$*"; }
warn() { printf '[qt-run] warning: %s\n' "$*" >&2; }
die() { printf '[qt-run] error: %s\n' "$*" >&2; exit 1; }

other_builds() {
  { pgrep -x rustc; pgrep -x cargo; } 2>/dev/null || true
}

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    die "neither sha256sum nor shasum is available"
  fi
}

# Homebrew does not normally put Qt's qmake on PATH.
if [[ "$(uname -s)" == Darwin ]]; then
  for qt_dir in /opt/homebrew/opt/qt /usr/local/opt/qt; do
    if [[ -x "${qt_dir}/bin/qmake" ]]; then
      export PATH="${qt_dir}/bin:${PATH}"
      export CMAKE_PREFIX_PATH="${qt_dir}${CMAKE_PREFIX_PATH:+:${CMAKE_PREFIX_PATH}}"
      break
    fi
  done
fi

if [[ "${DEBUG:-0}" == 1 ]]; then
  profile=debug
  profile_args=()
  warn "DEBUG=1 is unoptimized and is not a valid owner smoke artifact"
else
  profile=release
  profile_args=(--release)
fi

# A single Cargo/rustc build is allowed across the workstation. The guard runs
# before cargo metadata so a refused invocation does not start Cargo at all.
if [[ "${FORCE:-0}" != 1 ]]; then
  running_builds="$(other_builds)"
  if [[ -n "${running_builds}" ]]; then
    build_count="$(printf '%s\n' "${running_builds}" | wc -l | tr -d ' ')"
    die "another cargo/rustc is running (${build_count}); wait for it to finish"
  fi
fi

if [[ "${NO_AUDIT:-0}" != 1 ]]; then
  say "running the fast repository checks"
  tools/lint/fast
fi

if [[ -n "${JOBS:-}" ]]; then
  [[ "${JOBS}" =~ ^[1-9][0-9]*$ ]] || die "JOBS must be a positive integer"
  export CARGO_BUILD_JOBS="${JOBS}"
fi

target_dir="$(
  cargo metadata --format-version 1 --no-deps |
    python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])'
)"
binary="${target_dir}/${profile}/chilpa-qt"

say "building chilpa-qt profile=${profile}"
say "target=${target_dir}"
build_started="$(date +%s)"
cargo build -p chilpa-qt "${profile_args[@]}"
[[ -x "${binary}" ]] || die "Cargo succeeded but ${binary} is missing"
# The sign-in window for plugins whose browser sign-in ends on an address only
# their own application receives (crates/chilpa-web-sign-in). It is built only
# where the web engine is installed; without it the link opens the browser.
sign_in="${target_dir}/${profile}/chilpa-web-sign-in"
rm -f "${sign_in}"
if [[ "$(uname -s)" == Linux ]] && pkg-config --exists webkitgtk-6.0 gtk4 2>/dev/null; then
  say "building chilpa-web-sign-in"
  cargo build -p chilpa-web-sign-in --features webkit "${profile_args[@]}"
else
  warn "webkitgtk-6.0 or gtk4 not found: plugin sign-in pages open in the browser"
fi
build_seconds="$(( $(date +%s) - build_started ))"
say "build finished in ${build_seconds}s"
say "binary=${binary}"

if [[ "${profile}" == release ]]; then
  source_commit="$(git rev-parse HEAD)"
  short_commit="$(git rev-parse --short=9 HEAD)"
  source_state=clean
  if [[ -n "$(git status --porcelain --untracked-files=no)" ]]; then
    source_state=dirty
    warn "tracked files differ from ${source_commit}; artifact is marked dirty"
  fi

  if [[ -n "${CHILPA_ARTIFACT_DIR:-}" ]]; then
    artifact_root="${CHILPA_ARTIFACT_DIR}"
  elif [[ -d /mnt/build ]]; then
    artifact_root=/mnt/build/artifacts/chilpa
  else
    artifact_root="${repo_root}/target/smoke-artifacts/chilpa"
  fi

  artifact_dir="${artifact_root}/$(date +%Y%m%d)-${short_commit}"
  [[ "${source_state}" == clean ]] || artifact_dir="${artifact_dir}-dirty"
  artifact="${artifact_dir}/chilpa-qt"
  mkdir -p "${artifact_dir}"
  install -m 0755 "${binary}" "${artifact}"
  [[ -x "${sign_in}" ]] && install -m 0755 "${sign_in}" "${artifact_dir}/chilpa-web-sign-in"
  if [[ -d "${repo_root}/target/compat-runtime" ]]; then
    "${repo_root}/scripts/package-compat.sh" "${repo_root}/target/compat-runtime" "${artifact_dir}"
    "${repo_root}/scripts/package-compat.sh" "${repo_root}/target/compat-runtime" "$(dirname "${binary}")"
  fi
  artifact_sha256="$(sha256_file "${artifact}")"
  printf '%s  %s\n' "${artifact_sha256}" chilpa-qt > "${artifact_dir}/SHA256SUMS"
  if [[ -x "${artifact_dir}/chilpa-web-sign-in" ]]; then
    printf '%s  %s\n' "$(sha256_file "${artifact_dir}/chilpa-web-sign-in")" chilpa-web-sign-in >> "${artifact_dir}/SHA256SUMS"
  fi
  printf 'source_commit=%s\nsource_state=%s\nprofile=%s\nbuilt_at=%s\n' \
    "${source_commit}" "${source_state}" "${profile}" "$(date --iso-8601=seconds 2>/dev/null || date '+%Y-%m-%dT%H:%M:%S%z')" \
    > "${artifact_dir}/MANIFEST.txt"

  say "artifact=${artifact}"
  say "sha256=${artifact_sha256}"
  say "source_commit=${source_commit} (${source_state})"
fi

if [[ "${NORUN:-0}" == 1 ]]; then
  say "NORUN=1; not launching Chilpa"
  exit 0
fi

exec "${binary}" "$@"
