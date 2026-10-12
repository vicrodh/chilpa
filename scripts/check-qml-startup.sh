#!/usr/bin/env bash
# Load the real application QML in an isolated profile. A tray icon can keep
# the process alive after QQmlApplicationEngine rejects Main.qml, so the log
# must be checked as well as the process lifetime.
set -euo pipefail

binary="${1:?pass the chilpa-qt binary to check}"
[[ -x "${binary}" ]] || { printf 'not executable: %s\n' "${binary}" >&2; exit 2; }

profile="$(mktemp -d)"
trap 'rm -rf "${profile}"' EXIT
mkdir -p "${profile}/config" "${profile}/data" "${profile}/cache" "${profile}/runtime"
chmod 700 "${profile}/runtime"

set +e
timeout --signal=TERM --kill-after=2s 12s dbus-run-session -- env \
  XDG_CONFIG_HOME="${profile}/config" \
  XDG_DATA_HOME="${profile}/data" \
  XDG_CACHE_HOME="${profile}/cache" \
  XDG_RUNTIME_DIR="${profile}/runtime" \
  QT_QPA_PLATFORM=offscreen \
  QT_QUICK_BACKEND=software \
  "${binary}" >"${profile}/startup.log" 2>&1
result=$?
set -e

if rg -n 'QQmlApplicationEngine failed to load component|Type .* unavailable|is not a type' "${profile}/startup.log"; then
  printf 'QML startup failed; full log follows:\n' >&2
  cat "${profile}/startup.log" >&2
  exit 1
fi
if [[ "${result}" -ne 124 ]]; then
  printf 'application ended before startup smoke completed (exit %s):\n' "${result}" >&2
  cat "${profile}/startup.log" >&2
  exit 1
fi
printf 'Main.qml loaded without a QML type error and the application stayed alive for 12 seconds\n'
