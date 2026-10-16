#!/usr/bin/env python3
"""I exercise the packaged WASM DSP and native editor in a disposable profile."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess
import tempfile
import time
import zipfile


def digest(data):
    return hashlib.sha256(data).hexdigest()


def read_package(path):
    """I accept the distribution ZIP or its unpacked plugin folder."""
    if path.is_dir():
        files = {}
        for entry in path.rglob("*"):
            if entry.is_symlink():
                raise ValueError("the DSP folder must not contain symbolic links")
            if entry.is_file():
                files[entry.relative_to(path).as_posix()] = entry.read_bytes()
    else:
        files = {}
        with zipfile.ZipFile(path) as package:
            manifests = [entry for entry in package.namelist()
                         if PurePosixPath(entry).name == "manifest.json"]
            if len(manifests) != 1:
                raise ValueError("the DSP ZIP must contain exactly one manifest")
            root = PurePosixPath(manifests[0]).parent
            for entry in package.infolist():
                name = PurePosixPath(entry.filename)
                if (name.is_absolute() or ".." in name.parts or "\\" in entry.filename
                        or stat.S_ISLNK(entry.external_attr >> 16)):
                    raise ValueError("the DSP ZIP contains an unsafe path")
                if entry.is_dir():
                    continue
                if not name.is_relative_to(root):
                    raise ValueError("the DSP ZIP contains files outside its plugin folder")
                relative = name.relative_to(root).as_posix()
                if relative in files:
                    raise ValueError("the DSP ZIP contains duplicate files")
                files[relative] = package.read(entry)
    manifest = json.loads(files["manifest.json"])
    identity = manifest["id"]
    if (manifest.get("kind") != "dsp" or manifest.get("runtime", "wasm") != "wasm"
            or not re.fullmatch(r"[A-Za-z0-9._-]+", identity)):
        raise ValueError("a WASM DSP package with a valid identity is required")
    if not files["plugin.wasm"].startswith(b"\x00asm\x01\x00\x00\x00"):
        raise ValueError("plugin.wasm must be a WebAssembly version 1 module")
    return manifest, files


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("package", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    package = args.package.resolve(strict=True)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    try:
        manifest, files = read_package(package)
    except (ValueError, KeyError, OSError, zipfile.BadZipFile) as error:
        parser.error(str(error))
    with binary.open("rb") as stream:
        binary_hash = hashlib.file_digest(stream, "sha256").hexdigest()
    identity = manifest["id"]
    fixture = (Path(__file__).resolve().parent / "fixtures/dsp.qml").read_text()
    report = {
        "started_at": datetime.now(timezone.utc).isoformat(),
        "binary": str(binary), "sha256": binary_hash, "package": str(package),
        "plugin": identity, "version": manifest["version"],
        "files": {name: digest(data) for name, data in sorted(files.items())},
        "runs": [], "passed": False,
    }
    if package.is_file():
        report["package_sha256"] = digest(package.read_bytes())
    with tempfile.TemporaryDirectory(prefix="chilpa-dsp-smoke-") as directory:
        profile = Path(directory)
        for name in ("config/chilpa", "cache", "runtime", "data/chilpa/plugins"):
            (profile / name).mkdir(parents=True)
        (profile / "runtime").chmod(0o700)
        # I select a file output before startup and never request playback.
        (profile / "config/chilpa/settings.conf").write_text(
            "output = file:capture\nlanguage = es\nappearance.renderer = software\n")
        folder = profile / "data/chilpa/plugins" / identity
        folder.mkdir()
        for name, data in files.items():
            destination = folder / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)
        approval = {key: manifest.get(key) for key in ("kind", "capabilities", "auth")}
        approval["runtime"] = manifest.get("runtime", "wasm")
        (folder.parent / "approvals.json").write_text(json.dumps({identity: approval}))
        environment = dict(os.environ, XDG_CONFIG_HOME=str(profile / "config"),
            XDG_CACHE_HOME=str(profile / "cache"), XDG_DATA_HOME=str(profile / "data"),
            XDG_RUNTIME_DIR=str(profile / "runtime"), QT_QPA_PLATFORM="offscreen",
            CHILPA_RENDERER="software")
        for key in ("CHILPA_GALLERY", "CHILPA_QML_RESOURCES", "QML_IMPORT_PATH", "QML2_IMPORT_PATH"):
            environment.pop(key, None)
        for mode in ("exercise", "restore"):
            smoke = profile / f"dsp-{mode}.qml"
            smoke.write_text(fixture.replace('"PLUGIN_ID"', json.dumps(identity))
                .replace('"SMOKE_MODE"', json.dumps(mode)))
            environment["CHILPA_QML_SMOKE"] = str(smoke)
            started = time.monotonic()
            log = output / f"dsp-{mode}.log"
            # Session services may keep inherited descriptors after Qt exits.
            with log.open("w") as stream:
                try:
                    result = subprocess.run(["timeout", "--kill-after=2s", "45s", "dbus-run-session", "--",
                        str(binary)], cwd=profile, env=environment, text=True, stdout=stream,
                        stderr=subprocess.STDOUT, timeout=50)
                    code = result.returncode
                except subprocess.TimeoutExpired:
                    code = 124
            transcript = log.read_text()
            errors = ("SMOKE FAIL", "ReferenceError:", "TypeError:", "QQmlApplicationEngine failed",
                      "is not a type", "is not installed", "SyntaxError:")
            passed = (code == 0 and f"SMOKE DSP {mode} completed" in transcript
                      and not any(error in transcript for error in errors))
            snapshots = [line.split("SMOKE DSP snapshot ", 1)[1] for line in transcript.splitlines()
                         if "SMOKE DSP snapshot " in line]
            state = json.loads(snapshots[-1]) if snapshots else None
            report["runs"].append({"mode": mode, "exit_code": code,
                "seconds": round(time.monotonic() - started, 3), "passed": passed,
                "log": str(log), "snapshot": state})
            print("\n".join(line for line in transcript.splitlines()
                            if "SMOKE" in line and "snapshot" not in line), flush=True)
            (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
            if not passed:
                print(transcript)
                return 1
        report["passed"] = True
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print("SMOKE DSP completed: native module, packaged catalog, editor actions, bypass, restart persistence")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
