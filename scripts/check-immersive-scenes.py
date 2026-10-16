#!/usr/bin/env python3
"""I check cover pixels or immersive scenes with an actual GPU scene graph."""
import argparse
import base64
from contextlib import contextmanager, nullcontext
import json
import os
from pathlib import Path
import select
import shutil
import subprocess
import tempfile

from PIL import Image, ImageChops, ImageDraw, ImageStat


def cover_sources(output):
    picture = Image.new("RGB", (320, 320))
    drawing = ImageDraw.Draw(picture)
    colors = ((240, 64, 32), (32, 192, 64), (32, 96, 224), (224, 208, 48))
    for (x, y), color in zip(((0, 0), (160, 0), (0, 160), (160, 160)), colors):
        drawing.rectangle((x, y, x + 159, y + 159), fill=color)
    path = output / "fixture.png"
    picture.save(path)
    return path.as_uri(), "data:image/png;base64," + base64.b64encode(path.read_bytes()).decode()


def verify_covers(output):
    rendered = Image.open(output / "covers.png").convert("RGB")
    colors = ((240, 64, 32), (32, 192, 64), (32, 96, 224), (224, 208, 48))
    for name, left in (("local", 24), ("URL", 232), ("contained", 440)):
        for (x, y), expected in zip(((40, 40), (120, 40), (40, 120), (120, 120)), colors):
            actual = rendered.getpixel((left + x, 24 + y))
            if max(abs(a - b) for a, b in zip(actual, expected)) > 6:
                raise SystemExit(f"SMOKE FAIL covers {name}: expected {expected}, got {actual}")
        corner = rendered.getpixel((left, 24))
        if max(abs(a - b) for a, b in zip(corner, (16, 24, 32))) > 6:
            raise SystemExit(f"SMOKE FAIL covers {name}: rounded corner was not masked: {corner}")
    print("SMOKE cover pixels verified: three loaded images, twelve color probes, rounded corners")


def verify_reactive(output, transcript):
    captures = [json.loads(line.split("SMOKE reactive capture ", 1)[1]) for line in transcript.splitlines()
                if "SMOKE reactive capture " in line]
    if len(captures) != 8:
        raise SystemExit("SMOKE FAIL reactive artwork: missing pulse captures")
    colors = ((240, 64, 32), (32, 192, 64), (32, 96, 224), (224, 208, 48))
    for capture in captures:
        rendered = Image.open(output / capture["file"]).convert("RGB")
        for (dx, dy), expected in zip(((-40, -40), (40, -40), (-40, 40), (40, 40)), colors):
            actual = rendered.getpixel((round(capture["x"]) + dx, round(capture["y"]) + dy))
            if max(abs(a - b) for a, b in zip(actual, expected)) > 8:
                raise SystemExit(f"SMOKE FAIL reactive artwork: missing cover pixels in {capture['file']}: {actual}")
    (output / "measurements.json").write_text(json.dumps(
        dict(captures=captures, injected_energy=True, stable_decode=True), indent=2) + "\n")
    print("SMOKE reactive artwork verified: stable native texture, eight pulses, 32 color probes")


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
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--covers-only", action="store_true", help="Check loaded cover pixels instead of immersive scenes.")
    mode.add_argument("--reactive-only", action="store_true", help="Check native artwork retention while energy crosses decode boundaries.")
    arguments = parser.parse_args()
    if arguments.host_display and not os.environ.get("DISPLAY"):
        parser.error("--host-display requires DISPLAY")
    binary = str(arguments.binary.resolve())
    output = arguments.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    fixture_name = ("reactive-artwork.qml" if arguments.reactive_only else
                    "cover-rendering.qml" if arguments.covers_only else "immersive-scenes.qml")
    fixture = Path(__file__).resolve().parent / "fixtures" / fixture_name
    with tempfile.TemporaryDirectory(prefix="chilpa-immersive-scenes-") as directory:
        profile = Path(directory)
        for name in ("config", "data", "cache", "runtime"):
            (profile / name).mkdir()
        (profile / "runtime").chmod(0o700)
        qml = profile / "immersive-scenes.qml"
        contents = fixture.read_text().replace('"OUTPUT_DIRECTORY"', json.dumps(str(output)))
        contents = contents.replace('"EXPECTED_BACKEND"', json.dumps(arguments.backend))
        if arguments.covers_only or arguments.reactive_only:
            local, remote = cover_sources(output)
            contents = contents.replace('"LOCAL_SOURCE"', json.dumps(local)).replace('"REMOTE_SOURCE"', json.dumps(remote))
        qml.write_text(contents)
        environment = dict(os.environ, XDG_CONFIG_HOME=str(profile / "config"), XDG_DATA_HOME=str(profile / "data"),
            XDG_CACHE_HOME=str(profile / "cache"), XDG_RUNTIME_DIR=str(profile / "runtime"),
            QT_QPA_PLATFORM="xcb", QT_NO_XDG_DESKTOP_PORTAL="1",
            QSG_RHI_BACKEND=arguments.backend, CHILPA_RENDERER=arguments.backend,
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
                  "failed to deserialize", "not found in the qrc", "pipeline creation failed", "resource creation failed", "Unable to assign",
                  "Failed to build graphics pipeline state", "Failed to link shader program")
        completed = ("SMOKE reactive artwork completed" if arguments.reactive_only else
                     "SMOKE covers completed" if arguments.covers_only else "SMOKE immersive scenes completed")
        if result.returncode or any(error in transcript for error in errors) or completed not in transcript:
            raise SystemExit(1)
    if arguments.covers_only:
        verify_covers(output)
        return
    if arguments.reactive_only:
        verify_reactive(output, transcript)
        return
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
