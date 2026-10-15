#!/usr/bin/env python3
"""I render every immersive scene and scope on an isolated Xvfb display."""
import argparse
from contextlib import contextmanager, nullcontext
import json
import os
from pathlib import Path
import select
import shutil
import subprocess
import tempfile

from PIL import Image, ImageChops, ImageStat


@contextmanager
def isolated_display(output):
    """I own this display process and terminate only that process on exit."""
    executable = shutil.which("Xvfb")
    if not executable:
        raise SystemExit("Xvfb is required for the isolated GPU smoke")
    reader, writer = os.pipe()
    with (output / "xvfb.log").open("w") as log:
        server = subprocess.Popen([executable, "-displayfd", str(writer), "-screen", "0", "1280x800x24", "-nolisten", "tcp"],
            pass_fds=(writer,), stdout=log, stderr=log)
        os.close(writer)
        try:
            if not select.select([reader], [], [], 10)[0]:
                raise SystemExit("Xvfb did not report a display within 10 seconds")
            display = os.read(reader, 64).decode().strip()
            if not display.isdigit():
                raise SystemExit("Xvfb failed to open an isolated display; see xvfb.log")
            yield ":" + display
        finally:
            os.close(reader)
            server.terminate()
            try:
                server.wait(timeout=5)
            except subprocess.TimeoutExpired:
                server.kill()
                server.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--backend", choices=("opengl", "vulkan"), default="opengl")
    parser.add_argument("--host-display", action="store_true", help="Use the existing X display with the isolated test profile.")
    arguments = parser.parse_args()
    if arguments.host_display and not os.environ.get("DISPLAY"):
        parser.error("--host-display requires DISPLAY")
    binary = str(arguments.binary.resolve())
    output = arguments.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    fixture = Path(__file__).resolve().parent / "fixtures/immersive-scenes.qml"
    with tempfile.TemporaryDirectory(prefix="chilpa-immersive-scenes-") as directory:
        profile = Path(directory)
        for name in ("config", "data", "cache", "runtime"):
            (profile / name).mkdir()
        (profile / "runtime").chmod(0o700)
        qml = profile / "immersive-scenes.qml"
        qml.write_text(fixture.read_text().replace('"OUTPUT_DIRECTORY"', json.dumps(str(output)))
            .replace('"EXPECTED_BACKEND"', json.dumps(arguments.backend)))
        environment = dict(os.environ, XDG_CONFIG_HOME=str(profile / "config"), XDG_DATA_HOME=str(profile / "data"),
            XDG_CACHE_HOME=str(profile / "cache"), XDG_RUNTIME_DIR=str(profile / "runtime"),
            QT_QPA_PLATFORM="xcb", QSG_RHI_BACKEND=arguments.backend, CHILPA_RENDERER=arguments.backend,
            CHILPA_QML_SMOKE=str(qml), QT_LOGGING_RULES="qt.qml.connections=false")
        command = ["timeout", "--kill-after=2s", "70s", "dbus-run-session", "--", binary]
        log_path = output / "immersive-scenes.log"
        # A service activated by the session bus may retain inherited stdout
        # after the application exits. A regular file lets wait reap the
        # command without waiting for EOF from that unrelated descendant.
        display_context = nullcontext(os.environ["DISPLAY"]) if arguments.host_display else isolated_display(output)
        with display_context as display, log_path.open("w") as log:
            environment["DISPLAY"] = display
            result = subprocess.run(command, env=environment, text=True, stdout=log, stderr=subprocess.STDOUT)
        transcript = log_path.read_text()
        print("\n".join(line for line in transcript.splitlines() if "SMOKE" in line or "qrc:" in line))
        errors = ("SMOKE FAIL", "ReferenceError:", "TypeError:", "Binding loop", "QQmlApplicationEngine failed", "is not a type",
                  "failed to deserialize", "not found in the qrc", "pipeline creation failed", "resource creation failed", "Unable to assign")
        if result.returncode or any(error in transcript for error in errors) or "SMOKE immersive scenes completed" not in transcript:
            raise SystemExit(1)
    measurements = {}
    for name in ("plasma", "tunnel", "aurora", "ribbon", "linebed", "ambient", "tunnel-flow", "goniometer", "oscilloscope"):
        first = Image.open(output / f"{name}-a.png").convert("RGB")
        second = Image.open(output / f"{name}-b.png").convert("RGB")
        spread = max(ImageStat.Stat(second).stddev)
        difference = sum(ImageStat.Stat(ImageChops.difference(first, second)).mean)
        measurements[name] = {"spread": round(spread, 4), "motion": round(difference, 4), "size": second.size}
        if spread < 0.5 or difference < 0.01:
            raise SystemExit(f"SMOKE FAIL {name}: flat or static render: {measurements[name]}")
    for name in ("ribbon", "linebed"):
        paused = Image.open(output / f"{name}-paused.png").convert("RGB")
        restored = Image.open(output / f"{name}-restored.png").convert("RGB")
        difference = sum(ImageStat.Stat(ImageChops.difference(paused, restored)).mean)
        measurements[name]["paused_restore"] = round(difference, 4)
        if difference > 0.01:
            raise SystemExit(f"SMOKE FAIL {name}: paused history changed on restore: {difference}")
    (output / "measurements.json").write_text(json.dumps(measurements, indent=2) + "\n")
    print("SMOKE immersive images verified: 22 captures, nine moving renders, paused history restored")


if __name__ == "__main__":
    main()
