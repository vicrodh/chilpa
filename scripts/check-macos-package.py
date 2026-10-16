#!/usr/bin/env python3
"""I start the deployed macOS app without using a real profile or audio output."""
import argparse
import json
import os
from pathlib import Path
import plistlib
import subprocess
import tempfile


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("app", type=Path)
parser.add_argument("--report", type=Path, required=True)
args = parser.parse_args()
app = args.app.resolve(strict=True)
info = plistlib.loads((app / "Contents/Info.plist").read_bytes())
assert info["CFBundleIdentifier"] == "app.chilpa.Chilpa"
resources = app / "Contents/Resources"
assert all((resources / name).is_file() for name in ("AppIcon.icns", "AppIcon-dark.icns"))
if info.get("CFBundleIconName"):
    assert (resources / "Assets.car").is_file()
args.report.parent.mkdir(parents=True, exist_ok=True)
log_path = args.report.with_suffix(".log")
with tempfile.TemporaryDirectory(prefix="chilpa-macos-smoke-") as directory:
    profile = Path(directory)
    for name in ("config/chilpa", "data", "cache", "runtime"):
        (profile / name).mkdir(parents=True)
    (profile / "runtime").chmod(0o700)
    (profile / "config/chilpa/settings.conf").write_text(
        "output = file:capture\nlanguage = en\nappearance.renderer = software\n"
        "updates.automatic = false\nintegrations.offline = true\n")
    environment = dict(os.environ, XDG_CONFIG_HOME=str(profile / "config"), XDG_DATA_HOME=str(profile / "data"),
        XDG_CACHE_HOME=str(profile / "cache"), XDG_RUNTIME_DIR=str(profile / "runtime"), TMPDIR=str(profile / "runtime"),
        QT_QPA_PLATFORM="offscreen", QT_QUICK_BACKEND="software", CHILPA_RENDERER="software")
    # I remove developer Qt paths so only the deployed runtime can satisfy imports.
    for key in ("DYLD_LIBRARY_PATH", "DYLD_FRAMEWORK_PATH", "QT_PLUGIN_PATH", "QML_IMPORT_PATH", "QML2_IMPORT_PATH", "CHILPA_QML_SMOKE"):
        environment.pop(key, None)
    with log_path.open("w") as log:
        process = subprocess.Popen([str(app / "Contents/MacOS" / info["CFBundleExecutable"])], env=environment,
            cwd=profile, stdout=log, stderr=subprocess.STDOUT)
        stayed_alive = False
        try:
            process.wait(timeout=12)
        except subprocess.TimeoutExpired:
            stayed_alive = True
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
transcript = log_path.read_text()
errors = ("QQmlApplicationEngine failed to load component", "is not a type", "ReferenceError:", "TypeError:",
    "Failed to build graphics pipeline state", "Failed to link shader program", "Library not loaded:")
passed = stayed_alive and "main window constructed" in transcript and not any(error in transcript for error in errors)
report = dict(passed=passed, app=str(app), stayed_alive_seconds=12 if stayed_alive else 0, log=str(log_path),
    physical_audio_used=False, real_profile_modified=False, native_icon_catalog=(resources / "Assets.car").is_file())
args.report.write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, indent=2))
if not passed:
    print(transcript)
raise SystemExit(0 if passed else 1)
