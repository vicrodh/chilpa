#!/usr/bin/env python3
"""I load the packaged process plugin through the host in a disposable profile."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import zipfile


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary", type=Path)
parser.add_argument("package", type=Path)
parser.add_argument("output", type=Path)
args = parser.parse_args()
output = args.output.resolve()
output.mkdir(parents=True, exist_ok=True)
fixture = Path(__file__).resolve().parent / "fixtures/process-plugin.qml"
with tempfile.TemporaryDirectory(prefix="chilpa-process-package-") as directory:
    profile = Path(directory)
    for name in ("config", "cache", "runtime", "data/chilpa/plugins"):
        (profile / name).mkdir(parents=True)
    (profile / "runtime").chmod(0o700)
    with zipfile.ZipFile(args.package) as package:
        manifest = json.loads(package.read("manifest.json"))
        identity = manifest["id"]
        if manifest.get("runtime") != "process" or not re.fullmatch(r"[A-Za-z0-9._-]+", identity):
            parser.error("a packaged process plugin with a valid identity is required")
        folder = profile / "data/chilpa/plugins" / identity
        folder.mkdir()
        for member in package.infolist():
            if member.is_dir() or Path(member.filename).name != member.filename:
                parser.error("the native package must contain only top-level files")
            (folder / member.filename).write_bytes(package.read(member))
        (folder / "plugin").chmod(0o755)
    approval = {key: manifest.get(key) for key in ("kind", "runtime", "capabilities", "auth")}
    (folder.parent / "approvals.json").write_text(json.dumps({identity: approval}))
    smoke = profile / "process-plugin.qml"
    smoke.write_text(fixture.read_text().replace('"PLUGIN_ID"', json.dumps(identity))
        .replace('"PLUGIN_VERSION"', json.dumps(manifest["version"])))
    environment = dict(os.environ, XDG_CONFIG_HOME=str(profile / "config"),
        XDG_CACHE_HOME=str(profile / "cache"), XDG_DATA_HOME=str(profile / "data"),
        XDG_RUNTIME_DIR=str(profile / "runtime"), QT_QPA_PLATFORM="offscreen",
        CHILPA_RENDERER="software", CHILPA_QML_SMOKE=str(smoke))
    result = subprocess.run(["timeout", "--kill-after=2s", "35s", "dbus-run-session", "--",
        str(args.binary.resolve())], env=environment, text=True, stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT)
    (output / "process-plugin.log").write_text(result.stdout)
    print("\n".join(line for line in result.stdout.splitlines() if "SMOKE" in line))
    errors = ("SMOKE FAIL", "ReferenceError:", "TypeError:", "QQmlApplicationEngine failed", "is not a type")
    if (result.returncode or any(error in result.stdout for error in errors)
            or "SMOKE packaged process plugin completed" not in result.stdout):
        print(result.stdout)
        raise SystemExit(1)
