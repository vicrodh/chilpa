#!/usr/bin/env python3
"""I exercise real Main and the device facade with a disposable profile and silent WAVs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import struct
import subprocess
import sys
import tempfile
import time
import uuid
import wave

ROOT = Path(__file__).resolve().parent
COMPLETED = {
    "basic": "SMOKE device real Main completed",
    "restore": "SMOKE device restart query and approved removal completed",
    "cancel": "SMOKE device cancel recovery and cleanup completed",
    "lifecycle": "SMOKE lifecycle real Main completed",
    "browser": "SMOKE browser real Main completed",
}
FIXTURES = {"basic": "device-sync.qml", "restore": "device-sync-restore.qml",
            "cancel": "device-sync-cancel.qml", "lifecycle": "device-sync-lifecycle.qml", "browser": "device-sync-browser.qml"}


def wav(path, size=176400):
    with wave.open(str(path), "wb") as output:
        output.setparams((2, 2, 44100, 0, "NONE", "not compressed"))
        while size:
            count = min(size, 256 * 1024)
            output.writeframesraw(bytes(count))
            size -= count
    fields = []
    for key, value in ((b"INAM", path.stem), (b"IART", "Sync Fixture"), (b"IPRD", "DAP Native Smoke")):
        text = value.encode("utf-8") + b"\0"
        fields.append(key + struct.pack("<I", len(text)) + text + bytes(len(text) % 2))
    info = b"INFO" + b"".join(fields)
    with path.open("r+b") as stream:
        stream.seek(0, 2)
        stream.write(b"LIST" + struct.pack("<I", len(info)) + info)
        length = stream.tell()
        stream.seek(4)
        stream.write(struct.pack("<I", length - 8))


def prepare(args):
    output = args.output.resolve() if args.output else Path(tempfile.mkdtemp(prefix="chilpa-device-smoke-"))
    if args.output:
        output.mkdir(parents=True, exist_ok=False)
    profile = output / "profile"
    profile.mkdir()
    seed = None
    if args.seed:
        seed = json.loads((args.seed / "smoke.json").read_text())
        result = json.loads((args.seed / "measurements.json").read_text())
        if seed.get("version") != 1 or seed.get("scenario") != "basic" or not result.get("passed"):
            raise ValueError("--seed must be the output directory of a completed basic device smoke")
        shutil.copytree(args.seed / "profile", profile, dirs_exist_ok=True)
    for name in ("config", "data", "cache", "renderer"):
        (profile / name).mkdir(exist_ok=True)
    media = output / "media"
    media.mkdir()
    for name in ("DAP First.wav", "DAP Second.wav"):
        wav(media / name)
    large = output / "large"
    if args.scenario == "cancel":
        large.mkdir()
        wav(large / "DAP Large.wav", 128 * 1024 * 1024)
    storage = args.mtp or (seed["storage"] if seed else "mounted")
    if args.scenario == "cancel" and storage == "mounted":
        raise ValueError("the cancellation smoke needs --mtp or an MTP seed")
    target = seed["target"] if seed else "ChilpaSmoke-" + uuid.uuid4().hex[:12]
    if not seed and storage == "mounted":
        target = str(output / "card")
        Path(target).mkdir()
    values = {"SMOKE_STORAGE": storage, "SMOKE_FOLDER": target, "SMOKE_PROFILE": "DAP smoke",
              "SMOKE_MEDIA_ROOT": str(media), "SMOKE_LARGE_ROOT": str(large),
              "SMOKE_CAPTURE_PREFIX": str(output / "capture-"), "SMOKE_CAPTURE": str(output / "capture.png")}
    source = (ROOT / "fixtures" / FIXTURES[args.scenario]).read_text()
    # I substitute inside QML string literals, including Windows-safe backslash escaping.
    for key in sorted(values, key=len, reverse=True):
        source = source.replace(key, json.dumps(values[key], ensure_ascii=False)[1:-1])
    fixture = output / "fixture.qml"
    fixture.write_text(source)
    manifest = dict(version=1, scenario=args.scenario, storage=storage, target=target,
                    profile=str(profile), binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),
                    fixture_sha256=hashlib.sha256(fixture.read_bytes()).hexdigest())
    (output / "smoke.json").write_text(json.dumps(manifest, indent=2) + "\n")
    return output, profile, fixture, manifest


def environment(args, profile, fixture):
    env = {key: value for key, value in os.environ.items()
           if not key.startswith(("CHILPA_", "QT_", "QSG_"))
           and key not in ("QML_IMPORT_PATH", "QML2_IMPORT_PATH", "LD_PRELOAD")}
    env.update(XDG_CONFIG_HOME=str(profile / "config"), XDG_DATA_HOME=str(profile / "data"),
               XDG_CACHE_HOME=str(profile / "cache"), QT_NO_XDG_DESKTOP_PORTAL="1", QSG_INFO="1",
               QSG_RHI_SHADER_CACHE_DIR=str(profile / "renderer"), CHILPA_QML_SMOKE=str(fixture))
    if args.platform == "offscreen":
        env.update(QT_QPA_PLATFORM="offscreen", QT_QUICK_BACKEND="software", CHILPA_RENDERER="software")
    else:
        env["QT_QPA_PLATFORM"] = "cocoa" if sys.platform == "darwin" else "wayland" if env.get("WAYLAND_DISPLAY") else "xcb"
    return env


def command(binary, output):
    if sys.platform == "darwin":
        return [str(binary)]
    bus = output / "dbus.conf"
    bus.write_text('<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth>'
                   '<policy context="default"><allow send_destination="*"/><allow receive_sender="*"/>'
                   '<allow own="*"/></policy></busconfig>')
    return ["dbus-run-session", "--config-file=" + str(bus), "--", str(binary)]


def linux_target(parent, binary):
    for path in Path("/proc").glob("[0-9]*/exe"):
        try:
            fields = path.parent.joinpath("stat").read_text().rsplit(")", 1)[1].split()
            if int(fields[1]) == parent and path.resolve() == binary:
                return int(path.parent.name)
        except (OSError, ValueError, IndexError):
            pass
    return None


def sample(pid, elapsed):
    if sys.platform == "darwin":
        result = subprocess.run(["/bin/ps", "-p", str(pid), "-o", "rss=,time="], capture_output=True, text=True)
        words = result.stdout.split()
        return dict(seconds=elapsed, rss_kib=int(words[0]), cpu_time=words[1]) if len(words) == 2 else None
    status = dict(line.split(":", 1) for line in Path(f"/proc/{pid}/status").read_text().splitlines() if ":" in line)
    return dict(seconds=elapsed, rss_kib=int(status["VmRSS"].split()[0]), threads=int(status["Threads"]),
                anonymous_kib=int(status["RssAnon"].split()[0]), file_kib=int(status["RssFile"].split()[0]))


def run(args):
    output, profile, fixture, manifest = prepare(args)
    print("Device smoke output: " + str(output), flush=True)
    started, samples, target = time.monotonic(), [], None
    deadline = 1900 if args.scenario in ("lifecycle", "browser") else 360
    with (output / "run.log").open("w") as log:
        process = subprocess.Popen(command(args.binary, output), env=environment(args, profile, fixture),
                                   stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            while process.poll() is None and time.monotonic() - started < deadline:
                target = process.pid if sys.platform == "darwin" else target or linux_target(process.pid, args.binary)
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
    errors = ("SMOKE FAIL", "ReferenceError:", "TypeError:", "Binding loop", "QQmlApplicationEngine failed", "is not a type", "SyntaxError", "Unable to assign")
    passed = bool(samples) and COMPLETED[args.scenario] in contents and not any(error in contents for error in errors)
    result = dict(**manifest, passed=passed, exit_code=process.returncode, platform=args.platform,
                  seconds=round(time.monotonic() - started, 2), samples=samples,
                  peak_rss_mib=max([row["rss_kib"] / 1024 for row in samples], default=None))
    (output / "measurements.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({key: value for key, value in result.items() if key != "samples"}))
    return 0 if passed else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--output", type=Path, help="new directory; I retain fixtures, profile and evidence")
    parser.add_argument("--seed", type=Path, help="output directory of an earlier basic smoke")
    parser.add_argument("--mtp", help="explicit LOCATION/STORAGE; I write only a new dedicated smoke folder")
    parser.add_argument("--scenario", choices=FIXTURES, default="basic")
    parser.add_argument("--platform", choices=("native", "offscreen"), default="native")
    args = parser.parse_args()
    args.binary = args.binary.resolve(strict=True)
    if not sys.platform.startswith("linux") and sys.platform != "darwin":
        parser.error("this native smoke runner currently supports Linux and macOS")
    if args.scenario != "basic" and not args.seed:
        parser.error("this scenario requires --seed from a basic smoke")
    return run(args)


if __name__ == "__main__":
    raise SystemExit(main())
