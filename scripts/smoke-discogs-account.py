#!/usr/bin/env python3
"""I exercise account settings and the quota dialog in real Main with a private profile."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

SCRIPTS = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("audio_smoke", SCRIPTS / "smoke-audio-output.py")
helpers = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helpers)


def run(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    profile = output / "profile"
    for name in ("config/chilpa", "data/chilpa", "cache", "runtime", "renderer"):
        (profile / name).mkdir(parents=True, exist_ok=True)
    (profile / "runtime").chmod(0o700)
    (profile / "config/chilpa/settings.conf").write_text("output = file:capture\nlanguage = " + args.language + "\n")
    source = (SCRIPTS / "fixtures/discogs-account.qml").read_text()
    for key, value in {"SMOKE_DURATION": str(args.duration * 1000),
                       "SMOKE_CAPTURE": str(output / "capture-settings.png"),
                       "SMOKE_QUOTA_CAPTURE": str(output / "capture-quota.png")}.items():
        source = source.replace(key, json.dumps(value)[1:-1])
    fixture = output / "fixture.qml"
    fixture.write_text(source)
    environment = helpers.helpers.environment(args, profile, fixture)
    environment.update(TMPDIR=str(profile / "runtime"), CHILPA_DEV="1")
    if args.timing:
        environment.update(QSG_RENDER_TIMING="1", QT_MESSAGE_PATTERN="[%{time process}] %{message}")
    started = time.monotonic()
    samples, target = [], None
    with (output / "run.log").open("w") as log:
        process = subprocess.Popen(helpers.helpers.command(args.binary, output), env=environment,
                                   stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            while process.poll() is None and time.monotonic() - started < args.duration + 90:
                target = process.pid if sys.platform == "darwin" else target or helpers.helpers.linux_target(process.pid, args.binary)
                if target:
                    try:
                        row = helpers.sample(target, round(time.monotonic() - started, 2))
                        if row:
                            samples.append(row)
                    except (OSError, ValueError, KeyError):
                        pass
                time.sleep(1)
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait(timeout=5)
    contents = (output / "run.log").read_text(errors="replace")
    errors = ("SMOKE FAIL", "ReferenceError:", "TypeError:", "Binding loop", "Unable to assign", "is not a type")
    passed = bool(samples) and "SMOKE Discogs real Main completed" in contents and not any(e in contents for e in errors)
    result = dict(passed=passed, platform=args.platform, language=args.language, timing=args.timing,
                  binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),
                  fixture_sha256=hashlib.sha256(fixture.read_bytes()).hexdigest(),
                  seconds=round(time.monotonic() - started, 2), exit_code=process.returncode, samples=samples,
                  peak_rss_mib=max([row["rss_kib"] / 1024 for row in samples], default=None))
    (output / "measurements.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({key: value for key, value in result.items() if key != "samples"}), flush=True)
    return 0 if passed else 1


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--duration", type=int, default=30)
    parser.add_argument("--timing", action="store_true")
    parser.add_argument("--platform", choices=("native", "offscreen"), default="native")
    parser.add_argument("--language", choices=("en", "es", "de", "fr", "pt", "nl", "ru", "ja"), default="es")
    options = parser.parse_args()
    options.binary = options.binary.resolve(strict=True)
    if not 1 <= options.duration <= 1800:
        parser.error("--duration must be between 1 and 1800 seconds")
    if not sys.platform.startswith("linux") and sys.platform != "darwin":
        parser.error("this smoke runner supports Linux and macOS")
    raise SystemExit(run(options))
