#!/usr/bin/env python3
"""I measure a native search click and its destination acknowledgement on an isolated library."""
import argparse
from contextlib import nullcontext
import importlib.util
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("database", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--backend", choices=("opengl", "vulkan"), default="opengl")
    parser.add_argument("--host-display", action="store_true")
    parser.add_argument("--timeout", type=int, default=45)
    arguments = parser.parse_args()
    if arguments.host_display and not os.environ.get("DISPLAY"):
        parser.error("--host-display requires DISPLAY")
    if not 30 <= arguments.timeout <= 60:
        parser.error("--timeout must be between 30 and 60 seconds")
    output = arguments.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    scripts = Path(__file__).resolve().parent
    specification = importlib.util.spec_from_file_location("immersive_smoke", scripts / "check-immersive-scenes.py")
    display_tools = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(display_tools)
    with tempfile.TemporaryDirectory(prefix="chilpa-navigation-ack-") as directory:
        profile = Path(directory)
        for name in ("config", "cache", "runtime", "data/chilpa"):
            (profile / name).mkdir(parents=True)
        (profile / "runtime").chmod(0o700)
        data_home = Path(os.environ.get("XDG_DATA_HOME") or Path.home() / ".local/share")
        if (data_home / "fonts").is_dir():
            (profile / "data/fonts").symlink_to((data_home / "fonts").resolve(), target_is_directory=True)
        with sqlite3.connect(arguments.database.resolve().as_uri() + "?mode=ro", uri=True) as source:
            with sqlite3.connect(profile / "data/chilpa/library.db") as target:
                source.backup(target)
                album = target.execute("""SELECT a.id, a.title, count(*) n FROM albums a
                    JOIN tracks t ON t.album_id = a.id JOIN sources s ON s.id = t.source_id
                    WHERE s.provider = 'local' AND s.enabled = 1 AND t.present = 1
                    GROUP BY a.id ORDER BY n DESC, a.id LIMIT 1""").fetchone()
        if not album or album[2] < 100:
            raise SystemExit("I need a local album with at least 100 tracks for this regression.")
        qml = profile / "navigation-ack.qml"
        contents = (scripts / "fixtures/navigation-ack.qml").read_text()
        for key, value in {"FIXTURE_ALBUM": album[0], "FIXTURE_TITLE": album[1],
                           "FIXTURE_TRACKS": album[2], "OUTPUT_DIRECTORY": str(output)}.items():
            contents = contents.replace(json.dumps(key), json.dumps(value))
        qml.write_text(contents)
        environment = dict(os.environ, XDG_CONFIG_HOME=str(profile / "config"),
            XDG_DATA_HOME=str(profile / "data"), XDG_CACHE_HOME=str(profile / "cache"),
            XDG_RUNTIME_DIR=str(profile / "runtime"), QT_QPA_PLATFORM="xcb",
            QT_NO_XDG_DESKTOP_PORTAL="1", QSG_RHI_BACKEND=arguments.backend,
            CHILPA_RENDERER=arguments.backend, CHILPA_QML_SMOKE=str(qml))
        command = ["timeout", "--signal=TERM", "--kill-after=2s", str(arguments.timeout) + "s",
                   "dbus-run-session", "--", str(arguments.binary.resolve())]
        context = nullcontext(os.environ["DISPLAY"]) if arguments.host_display else display_tools.isolated_display(output)
        log_path = output / "navigation-ack.log"
        with context as display, log_path.open("w") as log:
            environment["DISPLAY"] = display
            result = subprocess.run(command, env=environment, stdout=log, stderr=subprocess.STDOUT, text=True)
        transcript = log_path.read_text()
        print("\n".join(line for line in transcript.splitlines() if "SMOKE" in line or "FAIL!" in line))
        samples = [json.loads(line.split("SMOKE navigation measurements ", 1)[1])
                   for line in transcript.splitlines() if "SMOKE navigation measurements " in line]
        (output / "measurements.json").write_text(json.dumps(dict(backend=arguments.backend,
            album_id=album[0], album_title=album[1], expected_tracks=album[2], samples=samples,
            exit_code=result.returncode), indent=2) + "\n")
        errors = ("SMOKE FAIL", "FAIL!", "ReferenceError:", "TypeError:", "Binding loop",
                  "QQmlApplicationEngine failed", "is not a type", "Unable to assign", "SyntaxError")
        if result.returncode or any(error in transcript for error in errors) or not samples or "SMOKE navigation acknowledgement completed" not in transcript:
            raise SystemExit(1)


if __name__ == "__main__":
    main()
