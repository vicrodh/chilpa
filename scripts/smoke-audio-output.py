#!/usr/bin/env python3
"""I test the real output panel with synthetic silence, file capture and a private profile."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time
import wave

SCRIPTS = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("device_smoke", SCRIPTS / "smoke-device-sync.py")
helpers = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helpers)


def sample(pid, elapsed):
    row = helpers.sample(pid, elapsed)
    if row and sys.platform.startswith("linux"):
        fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
        row["cpu_seconds"] = (int(fields[11]) + int(fields[12])) / os.sysconf("SC_CLK_TCK")
        totals = dict(line.split(":", 1) for line in Path(f"/proc/{pid}/smaps_rollup").read_text().splitlines() if ":" in line)
        row["pss_kib"] = int(totals["Pss"].split()[0])
    return row


def run(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    profile = output / "profile"
    if args.seed:
        if not json.loads((args.seed / "measurements.json").read_text()).get("passed"):
            raise ValueError("--seed requires a completed audio output smoke")
        shutil.copytree(args.seed.resolve() / "profile", profile)
    for name in ("config/chilpa", "data/chilpa", "cache", "runtime", "renderer"):
        (profile / name).mkdir(parents=True, exist_ok=True)
    (profile / "runtime").chmod(0o700)
    (profile / "config/chilpa/settings.conf").write_text("output = file:capture\nlanguage = " + args.language + "\n")
    music = profile / "fixture.wav"
    with wave.open(str(music), "wb") as stream:
        stream.setparams((2, 2, 44100, 0, "NONE", "not compressed"))
        stream.writeframes(bytes(44100 * 4 * 30))
    buffer = dict(id=1, name="Now playing", renamed=False, epoch=1, paths=[str(music)], origins=[],
                  removed=[False], placement=["Original"], groups=[], order=[0], cursor=0,
                  seconds=0, shuffle=False, repeat=0, stop_after=None)
    (profile / "data/chilpa/listenlist.json").write_text(json.dumps(dict(version=1, buffers=[buffer], active=1, selected=1, next=2)))
    source = (SCRIPTS / "fixtures/audio-output.qml").read_text()
    source = source.replace("SMOKE_SCENARIO", args.scenario).replace("SMOKE_DURATION", str(args.duration * 1000))
    source = source.replace('"SMOKE_CAPTURE"', json.dumps(str(output / "capture.png")))
    fixture = output / "fixture.qml"
    fixture.write_text(source)
    environment = helpers.environment(args, profile, fixture)
    environment.update(TMPDIR=str(profile / "runtime"), CHILPA_CAPTURE_REALTIME="1", CHILPA_DEV="1")
    if args.timing:
        environment["QSG_RENDER_TIMING"] = "1"
    started = time.monotonic()
    samples, target = [], None
    with (output / "run.log").open("w") as log:
        process = subprocess.Popen(helpers.command(args.binary, output), env=environment,
                                   stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            while process.poll() is None and time.monotonic() - started < args.duration + 190:
                target = process.pid if sys.platform == "darwin" else target or helpers.linux_target(process.pid, args.binary)
                if target:
                    try:
                        row = sample(target, round(time.monotonic() - started, 2))
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
    passed = bool(samples) and "SMOKE audio real Main completed" in contents and not any(e in contents for e in errors)
    result = dict(passed=passed, scenario=args.scenario, platform=args.platform,
                  timing=args.timing,
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
    parser.add_argument("--seed", type=Path)
    parser.add_argument("--scenario", choices=("basic", "lifecycle", "idle"), default="basic")
    parser.add_argument("--platform", choices=("native", "offscreen"), default="native")
    parser.add_argument("--duration", type=int)
    parser.add_argument("--timing", action="store_true", help="keep Qt's render timing in the isolated log")
    parser.add_argument("--language", choices=("en", "es", "de", "fr", "pt", "nl", "ru", "ja"), default="es")
    options = parser.parse_args()
    options.binary = options.binary.resolve(strict=True)
    if options.duration is None:
        options.duration = 1800 if options.scenario == "lifecycle" else 30
    if not 1 <= options.duration <= 1800:
        parser.error("--duration must be between 1 and 1800 seconds")
    if not sys.platform.startswith("linux") and sys.platform != "darwin":
        parser.error("this smoke runner supports Linux and macOS")
    raise SystemExit(run(options))
