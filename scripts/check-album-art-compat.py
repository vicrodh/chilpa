#!/usr/bin/env python3
"""Exercise both artwork call signatures against the real native QML type."""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary", type=Path)
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
binary = args.binary.resolve(strict=True)
fixture = root / "crates/chilpa-qt/tests/smoke/album-art-compat.qml"
with tempfile.TemporaryDirectory(prefix="chilpa-art-compat-") as directory:
    profile = Path(directory)
    for name in ("config", "data", "cache", "runtime"):
        (profile / name).mkdir()
    (profile / "runtime").chmod(0o700)
    environment = {key: value for key, value in os.environ.items()
                   if not key.startswith("CHILPA_") and key not in
                   ("QML_IMPORT_PATH", "QML2_IMPORT_PATH", "QT_PLUGIN_PATH")}
    environment.update(XDG_CONFIG_HOME=str(profile / "config"), XDG_DATA_HOME=str(profile / "data"),
                       XDG_CACHE_HOME=str(profile / "cache"), XDG_RUNTIME_DIR=str(profile / "runtime"),
                       QT_QPA_PLATFORM="offscreen", QT_QUICK_BACKEND="software", CHILPA_RENDERER="software",
                       CHILPA_QML_SMOKE=str(fixture))
    result = subprocess.run(["dbus-run-session", "--", str(binary)], env=environment,
                            text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=25)
    marker = "SMOKE album-art-compat failures=0"
    if result.returncode or marker not in result.stdout or "SMOKE FAIL" in result.stdout:
        print(result.stdout)
        raise SystemExit(1)
    print(marker)
