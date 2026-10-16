#!/usr/bin/env python3
"""I follow decoded music through a paced capture output into the actual views."""
import argparse
from array import array
import importlib.util
import json
import math
import os
from pathlib import Path
import subprocess
import tempfile
import wave

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary", type=Path)
parser.add_argument("output", type=Path)
parser.add_argument("--backend", choices=("opengl", "vulkan"), default="opengl")
parser.add_argument("--host-display", action="store_true")
parser.add_argument("--dsp-package", type=Path)
args = parser.parse_args()
scripts = Path(__file__).resolve().parent
output = args.output.resolve()
output.mkdir(parents=True, exist_ok=True)
spec = importlib.util.spec_from_file_location("scene_checks", scripts / "check-immersive-scenes.py")
scene_checks = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scene_checks)

with tempfile.TemporaryDirectory(prefix="chilpa-live-visualizer-") as directory:
    profile = Path(directory)
    for name in ("config/chilpa", "data/chilpa/plugins", "cache", "runtime"):
        (profile / name).mkdir(parents=True)
    (profile / "runtime").chmod(0o700)
    (profile / "config/chilpa/settings.conf").write_text("output = file:capture\nlanguage = es\n")
    if args.dsp_package:
        dsp_spec = importlib.util.spec_from_file_location("dsp_checks", scripts / "check-dsp.py")
        dsp_checks = importlib.util.module_from_spec(dsp_spec)
        dsp_spec.loader.exec_module(dsp_checks)
        manifest, files = dsp_checks.read_package(args.dsp_package.resolve(strict=True))
        plugins = profile / "data/chilpa/plugins"
        for name, data in files.items():
            destination = plugins / manifest["id"] / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)
        approval = {key: manifest.get(key) for key in ("kind", "capabilities", "auth")}
        approval["runtime"] = manifest.get("runtime", "wasm")
        (plugins / "approvals.json").write_text(json.dumps({manifest["id"]: approval}))
    music = profile / "fixture.wav"
    samples = array("h")
    rate = 48000
    for index in range(rate * 48):
        seconds = index / rate
        amplitude = 0.15 + 0.12 * math.sin(seconds * math.tau * 0.8)
        samples.append(round(32767 * amplitude * math.sin(index * math.tau * 440 / rate)))
        samples.append(round(32767 * amplitude * math.sin(index * math.tau * 659 / rate + 0.7)))
    if os.sys.byteorder != "little":
        samples.byteswap()
    with wave.open(str(music), "wb") as stream:
        stream.setparams((2, 2, rate, 0, "NONE", "not compressed"))
        stream.writeframes(samples.tobytes())
    buffer = dict(id=1, name="Now playing", renamed=False, epoch=1, paths=[str(music)], origins=[],
        removed=[False], placement=["Original"], groups=[], order=[0], cursor=0,
        seconds=0, shuffle=False, repeat=0, stop_after=None)
    (profile / "data/chilpa/listenlist.json").write_text(json.dumps(dict(version=1, buffers=[buffer], active=1, selected=1, next=2)))
    fixture = profile / "live.qml"
    fixture.write_text((scripts / "fixtures/live-visualizer.qml").read_text()
        .replace('"OUTPUT_DIRECTORY"', json.dumps(str(output)))
        .replace('"WITH_DSP"', "true" if args.dsp_package else "false"))
    environment = dict(os.environ, XDG_CONFIG_HOME=str(profile / "config"), XDG_DATA_HOME=str(profile / "data"),
        XDG_CACHE_HOME=str(profile / "cache"), XDG_RUNTIME_DIR=str(profile / "runtime"), TMPDIR=str(profile / "runtime"),
        QT_QPA_PLATFORM="xcb", QT_NO_XDG_DESKTOP_PORTAL="1", CHILPA_QML_SMOKE=str(fixture),
        CHILPA_CAPTURE_REALTIME="1", QSG_RHI_BACKEND=args.backend, CHILPA_RENDERER=args.backend)
    log_path = output / "live-visualizer.log"
    from contextlib import nullcontext
    display = nullcontext(os.environ["DISPLAY"]) if args.host_display else scene_checks.isolated_display(output)
    with display as address, log_path.open("w") as log:
        environment["DISPLAY"] = address
        result = subprocess.run(["timeout", "--kill-after=2s", "65s", "dbus-run-session", "--", str(args.binary.resolve())],
            env=environment, cwd=profile, stdout=log, stderr=subprocess.STDOUT, timeout=70)
    transcript = log_path.read_text()
    print("\n".join(line for line in transcript.splitlines() if "SMOKE" in line or "qrc:" in line))
    capture = profile / "runtime/chilpa-capture.raw"
    captured_bytes = capture.stat().st_size if capture.exists() else 0
    print(f"SMOKE live capture bytes={captured_bytes}")
    errors = ("SMOKE FAIL", "ReferenceError:", "TypeError:", "Binding loop", "Unable to assign", "is not a type",
              "Failed to build graphics pipeline state", "Failed to link shader program")
    passed = (result.returncode == 0 and captured_bytes > rate * 4
              and "SMOKE live visualizer completed" in transcript and not any(error in transcript for error in errors))
    (output / "report.json").write_text(json.dumps(dict(binary=str(args.binary.resolve()), backend=args.backend,
        captured_bytes=captured_bytes, exit_code=result.returncode, passed=passed, log=str(log_path)), indent=2) + "\n")
    raise SystemExit(0 if passed else 1)
