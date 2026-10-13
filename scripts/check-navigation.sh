#!/usr/bin/env bash
# I measure real QML navigation and unchanged-model refreshes in a disposable
# profile, optionally backed by a read-only snapshot of an existing library.
set -euo pipefail
binary="${1:?pass the chilpa-qt binary}"
library="${2:-}"
project="$(cd "$(dirname "$0")/.." && pwd)"
profile="$(mktemp -d)"
trap 'rm -rf "${profile}"' EXIT
mkdir -p "${profile}/data/chilpa" "${profile}/config" "${profile}/cache" "${profile}/runtime"
chmod 700 "${profile}/runtime"
if [[ -n "${library}" ]]; then
    python3 - "${library}" "${profile}/data/chilpa/library.db" <<'PY'
from pathlib import Path
import sqlite3
import sys
source = sqlite3.connect(Path(sys.argv[1]).resolve().as_uri() + "?mode=ro", uri=True)
target = sqlite3.connect(sys.argv[2])
source.backup(target)
target.close()
source.close()
PY
fi
set +e
timeout --signal=TERM --kill-after=2s 25s dbus-run-session -- env \
  XDG_CONFIG_HOME="${profile}/config" XDG_DATA_HOME="${profile}/data" \
  XDG_CACHE_HOME="${profile}/cache" XDG_RUNTIME_DIR="${profile}/runtime" \
  QT_QPA_PLATFORM=offscreen CHILPA_RENDERER=software \
  CHILPA_QML_SMOKE="${project}/crates/chilpa-qt/tests/smoke/navigation.qml" \
  QT_LOGGING_RULES="chilpa.nav.timing.info=true" \
  "${binary}" >"${profile}/navigation.log" 2>&1
result=$?
set -e
if [[ "${result}" -ne 0 ]] || rg -q 'SMOKE FAIL|ReferenceError:|TypeError:|binding loop|QQmlApplicationEngine failed|is not a type|Type .* unavailable' "${profile}/navigation.log"; then
    cat "${profile}/navigation.log"
    exit 1
fi
rg 'SMOKE|navigation:' "${profile}/navigation.log"
rg -q 'navigation completed samples=6 failures=0' "${profile}/navigation.log"
