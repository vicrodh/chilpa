#!/usr/bin/env python3
"""I exercise native queue artwork and production lyric layout on a GPU."""
import argparse
from array import array
from contextlib import nullcontext
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import wave

from PIL import Image, ImageChops, ImageStat


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def prepare(profile, library):
    for name in ("config/chilpa", "data/chilpa", "cache", "runtime"):
        (profile / name).mkdir(parents=True)
    (profile / "runtime").chmod(0o700)
    (profile / "config/chilpa/settings.conf").write_text(
        "output = file:capture\nlanguage = es\nui.lyrics.lite = false\nui.lyrics.uppercase = false\n")
    paths, pictures = [], []
    rate = 48000
    for index, color in enumerate(((240, 64, 32), (32, 208, 64), (32, 96, 240))):
        folder = profile / f"release-{index}"
        folder.mkdir()
        picture = folder / "cover.png"
        Image.new("RGB", (384, 384), color).save(picture)
        pictures.append(picture)
        music = folder / "fixture.wav"
        samples = array("h", (round(1800 * math.sin(math.tau * (330 + index * 110) * frame / rate))
                              for frame in range(rate * 30) for _ in range(2)))
        if os.sys.byteorder != "little":
            samples.byteswap()
        with wave.open(str(music), "wb") as stream:
            stream.setparams((2, 2, rate, 0, "NONE", "not compressed"))
            stream.writeframes(samples.tobytes())
        paths.append(str(music))
    # SQLite's backup API reads the source through a read-only connection.
    with sqlite3.connect(library.as_uri() + "?mode=ro", uri=True) as source:
        with sqlite3.connect(profile / "data/chilpa/library.db") as target:
            source.backup(target)
            target.execute("INSERT INTO sources(provider,instance) VALUES ('local','focus-smoke')")
            source_id = target.execute("SELECT last_insert_rowid()").fetchone()[0]
            album = target.execute("SELECT coalesce(max(id),0)+1 FROM albums").fetchone()[0]
            track = target.execute("SELECT coalesce(max(id),0)+1 FROM tracks").fetchone()[0]
            for index, path in enumerate(paths):
                title = f"Focus fixture {index + 1}"
                folder = str(Path(path).parent)
                target.execute("""INSERT INTO albums(id,title,album_artist,folder,version_key,artwork_path)
                    VALUES (?,?,'Fixture artist',?,?,?)""",
                    (album + index, title, folder, f"focus:{source_id}:{index}", str(pictures[index])))
                target.execute("""INSERT INTO tracks(id,source_id,external_id,title,artist,album_id,
                    disc_number,track_number,duration_ms,added_at,album_title,release_part,release_artwork)
                    VALUES (?,?,?,?,'Fixture artist',?,1,1,30000,1,?,?,?)""",
                    (track + index, source_id, path, title, album + index, title, folder, str(pictures[index])))
            target.commit()
    buffer = dict(id=1, name="Focus fixture", renamed=False, epoch=1, paths=paths, origins=[],
                  removed=[False] * 3, placement=["Original"] * 3, groups=[], order=[0, 1, 2],
                  cursor=0, seconds=0, shuffle=False, repeat=0, stop_after=None)
    (profile / "data/chilpa/listenlist.json").write_text(json.dumps(
        dict(version=1, buffers=[buffer], active=1, selected=1, next=2)))
    return paths, [track + index for index in range(3)]


def verify_images(output, samples):
    measurements = {}
    for direction in ("next", "back"):
        frames = [row for row in samples if row["phase"] == direction]
        if len(frames) < 3:
            raise ValueError(f"{direction}: fewer than three captures during actual motion")
        for frame in frames:
            picture = Image.open(output / frame["file"]).convert("RGB")
            x, y = round(frame["x"]), round(frame["y"])
            probe = picture.crop((x - 3, y - 3, x + 4, y + 4))
            color = ImageStat.Stat(probe).mean
            channel = frame["channel"]
            if color[channel] < 60 or color[channel] - max(color[i] for i in range(3) if i != channel) < 35:
                raise ValueError(f"{direction}: missing artwork at {frame['file']}: {color}")
        first = Image.open(output / frames[0]["file"]).convert("RGB")
        last = Image.open(output / frames[-1]["file"]).convert("RGB")
        motion = sum(ImageStat.Stat(ImageChops.difference(first, last)).mean)
        if motion < 0.1:
            raise ValueError(f"{direction}: captures did not move")
        measurements[direction] = dict(frames=len(frames), motion=motion)
    first = Image.open(output / "lyrics-word-a.png").convert("RGB")
    second = Image.open(output / "lyrics-word-b.png").convert("RGB")
    progress = sum(ImageStat.Stat(ImageChops.difference(first, second)).mean)
    if progress < 0.01 or max(ImageStat.Stat(second).stddev) < 1:
        raise ValueError("word timing did not change visible lyric pixels")
    for name in ("lyrics-line.png", "lyrics-follow.png"):
        if max(ImageStat.Stat(Image.open(output / name).convert("RGB")).stddev) < 1:
            raise ValueError(f"{name}: empty lyric capture")
    measurements["lyrics"] = dict(word_progress=progress, captures=4)
    return measurements


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("library", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--backend", choices=("opengl", "vulkan"), default="opengl")
    parser.add_argument("--host-display", action="store_true")
    args = parser.parse_args()
    if args.host_display and not os.environ.get("DISPLAY"):
        parser.error("--host-display requires DISPLAY")
    binary, library = args.binary.resolve(strict=True), args.library.resolve(strict=True)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    scripts = Path(__file__).resolve().parent
    spec = importlib.util.spec_from_file_location("scene_checks", scripts / "check-immersive-scenes.py")
    scene_checks = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(scene_checks)
    report = dict(binary=str(binary), sha256=digest(binary), library=str(library), backend=args.backend,
                  passed=False, lyrics_document_injected=True, lyrics_clock_injected=True,
                  scope="Native queue/history, decoded PCM capture and GPU artwork; production lyric layout with explicit fixture timing")
    log_path = output / "focus-panels.log"
    try:
        with tempfile.TemporaryDirectory(prefix="chilpa-focus-panels-") as temporary:
            profile = Path(temporary)
            paths, identities = prepare(profile, library)
            fixture = profile / "focus-panels.qml"
            contents = (scripts / "fixtures/focus-panels.qml").read_text()
            for key, value in (("OUTPUT_DIRECTORY", str(output)), ("EXPECTED_BACKEND", args.backend),
                               ("TRACK_PATHS", paths), ("TRACK_IDS", identities)):
                contents = contents.replace(json.dumps(key), json.dumps(value))
            fixture.write_text(contents)
            environment = dict(os.environ, XDG_CONFIG_HOME=str(profile / "config"), XDG_DATA_HOME=str(profile / "data"),
                XDG_CACHE_HOME=str(profile / "cache"), XDG_RUNTIME_DIR=str(profile / "runtime"),
                TMPDIR=str(profile / "runtime"), QT_QPA_PLATFORM="xcb", QT_NO_XDG_DESKTOP_PORTAL="1",
                CHILPA_QML_SMOKE=str(fixture), CHILPA_CAPTURE_REALTIME="1", QSG_RHI_BACKEND=args.backend,
                CHILPA_RENDERER=args.backend, QT_LOGGING_RULES="qt.qml.connections=false")
            display = nullcontext(os.environ["DISPLAY"]) if args.host_display else scene_checks.isolated_display(output)
            with display as address, log_path.open("w") as log:
                environment["DISPLAY"] = address
                result = subprocess.run(["timeout", "--kill-after=2s", "55s", "dbus-run-session", "--", str(binary)],
                    env=environment, cwd=profile, stdout=log, stderr=subprocess.STDOUT, timeout=60)
            transcript = log_path.read_text()
            print("\n".join(line for line in transcript.splitlines() if "SMOKE" in line or "qrc:" in line))
            capture = profile / "runtime/chilpa-capture.raw"
            report.update(exit_code=result.returncode, captured_bytes=capture.stat().st_size if capture.exists() else 0,
                          log=str(log_path), fixture_sha256=digest(fixture))
            errors = ("SMOKE FAIL", "ReferenceError:", "TypeError:", "Binding loop", "Unable to assign", "is not a type",
                      "QQmlApplicationEngine failed", "Failed to build graphics pipeline state", "Failed to link shader program",
                      "pipeline creation failed", "resource creation failed", "not found in the qrc")
            if result.returncode or any(error in transcript for error in errors) or "SMOKE focus panels completed" not in transcript:
                raise ValueError("native fixture failed; see focus-panels.log")
            if report["captured_bytes"] < 48000 * 4:
                raise ValueError("less than one second of actual decoded stereo PCM was captured")
            samples = [json.loads(line.split("SMOKE focus sample ", 1)[1]) for line in transcript.splitlines()
                       if "SMOKE focus sample " in line]
            evidence = [json.loads(line.split("SMOKE focus evidence ", 1)[1]) for line in transcript.splitlines()
                        if "SMOKE focus evidence " in line]
            if len(evidence) != 1:
                raise ValueError("missing final native identity/layout evidence")
            report.update(measurements=verify_images(output, samples), samples=samples, evidence=evidence[0], passed=True)
    except (ValueError, OSError, subprocess.TimeoutExpired) as error:
        report["error"] = str(error)
        print("SMOKE FAIL focus panels: " + str(error))
    finally:
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    raise SystemExit(0 if report["passed"] else 1)


if __name__ == "__main__":
    main()
