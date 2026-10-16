#!/usr/bin/env python3
"""I time Library's real facade in a disposable, offline profile."""
import argparse
import os
from pathlib import Path
import re
import sqlite3
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary", type=Path)
parser.add_argument("library", type=Path)
parser.add_argument("--budget-ms", type=int, default=5000)
args = parser.parse_args()
project = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix="chilpa-filtering-") as directory:
    profile = Path(directory)
    for name in ("data/chilpa", "config", "cache", "runtime"):
        (profile / name).mkdir(parents=True, exist_ok=True)
    (profile / "runtime").chmod(0o700)
    with sqlite3.connect(args.library.resolve().as_uri() + "?mode=ro", uri=True) as source:
        with sqlite3.connect(profile / "data/chilpa/library.db") as target:
            source.backup(target)
    environment = dict(os.environ, XDG_CONFIG_HOME=str(profile / "config"),
                       XDG_DATA_HOME=str(profile / "data"), XDG_CACHE_HOME=str(profile / "cache"),
                       XDG_RUNTIME_DIR=str(profile / "runtime"), QT_QPA_PLATFORM="offscreen",
                       CHILPA_RENDERER="software",
                       CHILPA_QML_SMOKE=str(project / "scripts/fixtures/library-filtering.qml"))
    result = subprocess.run(["dbus-run-session", "--", str(args.binary.resolve())],
                            env=environment, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            text=True, timeout=90)
    for line in result.stdout.splitlines():
        if "SMOKE" in line or "Error:" in line:
            print(line)
    samples = re.findall(r"SMOKE filtering (\S+) elapsed=(\d+)ms", result.stdout)
    errors = ("SMOKE FAIL", "ReferenceError:", "TypeError:", "QQmlApplicationEngine failed")
    if (result.returncode or len(samples) != 8 or "SMOKE burst latest-filter" not in result.stdout
            or any(error in result.stdout for error in errors)):
        print(result.stdout)
        raise SystemExit(1)
    if any(int(elapsed) > args.budget_ms for _, elapsed in samples):
        raise SystemExit("filtering exceeded the requested budget")
