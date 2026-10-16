#!/usr/bin/env python3
"""I verify same-route spoken-show clicks with an isolated process source and GPU window."""
import argparse
from contextlib import nullcontext
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--backend", choices=("opengl", "vulkan"), default="opengl")
    parser.add_argument("--host-display", action="store_true")
    arguments = parser.parse_args()
    if arguments.host_display and not os.environ.get("DISPLAY"):
        parser.error("--host-display requires DISPLAY")
    output = arguments.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    scripts = Path(__file__).resolve().parent
    specification = importlib.util.spec_from_file_location("immersive_smoke", scripts / "check-immersive-scenes.py")
    display_tools = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(display_tools)
    with tempfile.TemporaryDirectory(prefix="chilpa-media-ack-") as directory:
        profile = Path(directory)
        for name in ("config", "cache", "runtime", "data/chilpa/plugins/app.test.spoken"):
            (profile / name).mkdir(parents=True)
        (profile / "runtime").chmod(0o700)
        data_home = Path(os.environ.get("XDG_DATA_HOME") or Path.home() / ".local/share")
        if (data_home / "fonts").is_dir():
            (profile / "data/fonts").symlink_to((data_home / "fonts").resolve(), target_is_directory=True)
        folder = profile / "data/chilpa/plugins/app.test.spoken"
        capabilities = [dict(type="register_source"), dict(type="register_panel")]
        manifest = dict(id="app.test.spoken", name="Spoken navigation fixture", version="1.0.0",
                        api_version=1, kind="source", runtime="process", capabilities=capabilities)
        (folder / "manifest.json").write_text(json.dumps(manifest))
        shutil.copyfile(scripts / "fixtures/navigation-media-source.py", folder / "plugin")
        (folder / "plugin").chmod(0o755)
        (folder.parent / "approvals.json").write_text(json.dumps({manifest["id"]:
            dict(kind="source", runtime="process", capabilities=capabilities, auth=None)}))
        qml = profile / "media-navigation-ack.qml"
        qml.write_text((scripts / "fixtures/media-navigation-ack.qml").read_text())
        environment = dict(os.environ, XDG_CONFIG_HOME=str(profile / "config"),
            XDG_DATA_HOME=str(profile / "data"), XDG_CACHE_HOME=str(profile / "cache"),
            XDG_RUNTIME_DIR=str(profile / "runtime"), QT_QPA_PLATFORM="xcb",
            QT_NO_XDG_DESKTOP_PORTAL="1", QSG_RHI_BACKEND=arguments.backend,
            CHILPA_RENDERER=arguments.backend, CHILPA_QML_SMOKE=str(qml))
        context = nullcontext(os.environ["DISPLAY"]) if arguments.host_display else display_tools.isolated_display(output)
        log_path = output / "media-navigation-ack.log"
        with context as display, log_path.open("w") as log:
            environment["DISPLAY"] = display
            result = subprocess.run(["timeout", "--kill-after=2s", "50s", "dbus-run-session", "--",
                str(arguments.binary.resolve())], env=environment, stdout=log, stderr=subprocess.STDOUT, text=True)
        transcript = log_path.read_text()
        print("\n".join(line for line in transcript.splitlines() if "SMOKE" in line or "FAIL!" in line))
        samples = [json.loads(line.split("SMOKE media acknowledgement ", 1)[1])
                   for line in transcript.splitlines() if "SMOKE media acknowledgement " in line]
        (output / "measurements.json").write_text(json.dumps(dict(backend=arguments.backend,
            samples=samples, exit_code=result.returncode), indent=2) + "\n")
        errors = ("SMOKE FAIL", "FAIL!", "ReferenceError:", "TypeError:", "Binding loop",
                  "QQmlApplicationEngine failed", "is not a type", "Unable to assign", "SyntaxError")
        if result.returncode or any(error in transcript for error in errors) or len(samples) != 2 or "SMOKE media navigation acknowledgement completed" not in transcript:
            raise SystemExit(1)


if __name__ == "__main__":
    main()
