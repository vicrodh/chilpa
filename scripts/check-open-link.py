#!/usr/bin/env python3
"""Exercise public link identification and source choice with isolated destinations."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary", type=Path)
parser.add_argument("output", type=Path)
parser.add_argument("--url", required=True)
parser.add_argument("--title", required=True)
parser.add_argument("--artist", required=True)
parser.add_argument("--album", default="")
parser.add_argument("--kind", choices=["album", "track"], default="track")
args = parser.parse_args()
binary = args.binary.resolve(strict=True)
output = args.output.resolve()
output.mkdir(parents=True, exist_ok=True)
fixtures = Path(__file__).resolve().parent / "fixtures"
for count in (1, 2):
    with tempfile.TemporaryDirectory(prefix="chilpa-open-link-") as temporary:
        profile = Path(temporary)
        for name in ("config", "cache", "runtime", "data/chilpa/plugins"):
            (profile / name).mkdir(parents=True)
        (profile / "runtime").chmod(0o700)
        plugins = profile / "data/chilpa/plugins"
        approvals = {}
        for index in range(count):
            plugin = f"app.test.destination{index}"
            folder = plugins / plugin
            folder.mkdir()
            capabilities = [dict(type="register_source"), dict(type="register_panel")]
            manifest = dict(id=plugin, name=f"Destination {index + 1}", version="1.0.0", api_version=1,
                            kind="source", runtime="process", capabilities=capabilities)
            (folder / "manifest.json").write_text(json.dumps(manifest))
            item = dict(kind=args.kind, ref=f"{args.kind}:1", title=args.title, artist=args.artist,
                        album=args.album, album_ref="album:1")
            (folder / "item.json").write_text(json.dumps(item))
            shutil.copyfile(fixtures / "link-source.py", folder / "plugin")
            (folder / "plugin").chmod(0o755)
            approvals[plugin] = dict(kind="source", runtime="process", capabilities=capabilities, auth=None)
        (plugins / "approvals.json").write_text(json.dumps(approvals))
        smoke = profile / "link.qml"
        qml = (fixtures / "open-link.qml").read_text().replace('"INPUT_URL"', json.dumps(args.url))
        qml = qml.replace("property int sourceCount: 1", f"property int sourceCount: {count}")
        smoke.write_text(qml.replace('"CAPTURE_PATH"', json.dumps(str(output / "destinations.png"))))
        env = dict(os.environ, XDG_CONFIG_HOME=str(profile / "config"), XDG_DATA_HOME=str(profile / "data"),
                   XDG_CACHE_HOME=str(profile / "cache"), XDG_RUNTIME_DIR=str(profile / "runtime"),
                   QT_QPA_PLATFORM="offscreen", CHILPA_RENDERER="software", CHILPA_QML_SMOKE=str(smoke))
        result = subprocess.run(["timeout", "--signal=TERM", "--kill-after=2s", "40s", "dbus-run-session", "--", str(binary)],
                                env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (output / f"sources-{count}.log").write_text(result.stdout)
        print("\n".join(line for line in result.stdout.splitlines() if "SMOKE" in line))
        if result.returncode or "SMOKE foreign music link completed" not in result.stdout or "SMOKE FAIL" in result.stdout:
            raise SystemExit(f"Foreign link failed with {count} destination(s); see sources-{count}.log")
with binary.open("rb") as stream:
    checksum = hashlib.file_digest(stream, "sha256").hexdigest()
print(json.dumps(dict(binary=str(binary), sha256=checksum, source_counts=[1, 2])))
