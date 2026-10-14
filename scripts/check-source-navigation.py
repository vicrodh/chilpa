#!/usr/bin/env python3
"""I check provider navigation, delayed shelves and covers in a disposable profile."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import struct
import zlib
import threading
import time

root = Path(__file__).resolve().parent / "fixtures"
requests = 0
class Pictures(BaseHTTPRequestHandler):
    def do_GET(self):
        global requests
        requests += 1
        time.sleep(0.15)
        index = sum(self.path.encode())
        pixel = bytes((index * 3 % 255, index * 7 % 255, index * 11 % 255))
        def chunk(kind, data):
            return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
        content = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 200, 200, 8, 2, 0, 0, 0))
        content += chunk(b"IDAT", zlib.compress((b"\x00" + pixel * 200) * 200)) + chunk(b"IEND", b"")
        self.send_response(200)
        self.send_header("Content-Type", "image/png")
        self.end_headers()
        self.wfile.write(content)
    def log_message(self, *_):
        pass
server = ThreadingHTTPServer(("127.0.0.1", 0), Pictures)
threading.Thread(target=server.serve_forever, daemon=True).start()
with tempfile.TemporaryDirectory(prefix="chilpa-source-smoke-") as directory:
    profile = Path(directory)
    for name in ("config", "cache", "runtime", "data/chilpa/plugins"):
        (profile / name).mkdir(parents=True)
    (profile / "runtime").chmod(0o700)
    approvals = {}
    for plugin in ("app.test.navigation", "app.test.second"):
        folder = profile / "data/chilpa/plugins" / plugin
        folder.mkdir()
        capabilities = [dict(type="register_source"), dict(type="register_panel"), dict(type="network", domains=["127.0.0.1"])]
        manifest = dict(id=plugin, name="Navigation fixture", version="1.0.0", api_version=1, kind="source", runtime="process", capabilities=capabilities)
        (folder / "manifest.json").write_text(json.dumps(manifest))
        (folder / "image-base.json").write_text(json.dumps(f"http://127.0.0.1:{server.server_port}"))
        shutil.copyfile(root / "navigation-source.py", folder / "plugin")
        (folder / "plugin").chmod(0o755)
        approvals[plugin] = dict(kind="source", runtime="process", capabilities=capabilities, auth=None)
    (profile / "data/chilpa/plugins/approvals.json").write_text(json.dumps(approvals))
    output = Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else profile / "captures"
    output.mkdir(parents=True, exist_ok=True)
    smoke = profile / "sources.qml"
    smoke.write_text((root / "source-navigation.qml").read_text().replace('property string captureFolder: ""', 'property string captureFolder: ' + json.dumps(str(output))))
    env = dict(os.environ, XDG_CONFIG_HOME=str(profile / "config"), XDG_DATA_HOME=str(profile / "data"), XDG_CACHE_HOME=str(profile / "cache"), XDG_RUNTIME_DIR=str(profile / "runtime"), QT_QPA_PLATFORM="offscreen", CHILPA_RENDERER="software", CHILPA_QML_SMOKE=str(smoke))
    result = subprocess.run(["timeout", "--signal=TERM", "--kill-after=2s", "40s", "dbus-run-session", "--", sys.argv[1]], env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (output / "source-navigation.log").write_text(result.stdout)
    errors = ("SMOKE FAIL", "ReferenceError:", "TypeError:", "binding loop", "QQmlApplicationEngine failed", "is not a type", "Type ")
    print("\n".join(line for line in result.stdout.splitlines() if "SMOKE" in line or "qrc:" in line or "plugin[" in line))
    print(f"SMOKE delayed image requests={requests}")
    if requests == 0:
        print("SMOKE FAIL no covers were requested")
        sys.exit(1)
    if result.returncode or any(error in result.stdout for error in errors) or "SMOKE sources completed" not in result.stdout:
        sys.exit(1)
