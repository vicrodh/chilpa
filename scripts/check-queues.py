#!/usr/bin/env python3
"""I check native queue focus and long additions in an isolated, paused profile."""
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile

project = Path(__file__).resolve().parents[1]
binary = Path(sys.argv[1]).resolve()
library = Path(sys.argv[2]).resolve()
with tempfile.TemporaryDirectory(prefix="chilpa-queue-smoke-") as directory:
    profile = Path(directory)
    for folder in ("config", "cache", "runtime", "data/chilpa"):
        (profile / folder).mkdir(parents=True)
    (profile / "runtime").chmod(0o700)
    source = sqlite3.connect(library.as_uri() + "?mode=ro", uri=True)
    target = sqlite3.connect(profile / "data/chilpa/library.db")
    source.backup(target)
    source.close()
    target.execute("INSERT OR IGNORE INTO sources(provider, instance) VALUES ('local', 'queue-smoke')")
    source_id = target.execute("SELECT id FROM sources WHERE provider='local' AND instance='queue-smoke'").fetchone()[0]
    album_id = target.execute("SELECT coalesce(max(id), 0) + 1 FROM albums").fetchone()[0]
    first_id = target.execute("SELECT coalesce(max(id), 0) + 1 FROM tracks").fetchone()[0]
    release_title = "Queue smoke fixture"
    release_key = f"release:{source_id}:{len(release_title)}:{release_title}{profile}"
    target.execute("INSERT INTO albums(id,title,album_artist,folder,version_key) VALUES (?, ?, 'Fixture artist', ?, ?)",
        (album_id, release_title, str(profile), release_key))
    target.executemany("""INSERT INTO tracks(id, source_id, external_id, title, artist, album_id,
        disc_number, track_number, duration_ms, added_at, album_title, release_part)
        VALUES (?, ?, ?, ?, 'Fixture artist', ?, 1, ?, 180000, 1, 'Queue smoke fixture', ?)""",
        ((first_id + index, source_id, str(profile / f"track-{index}.flac"),
          f"Queue smoke fixture {index}", album_id, index + 1, str(profile)) for index in range(20000)))
    target.commit()
    target.close()
    buffer = dict(id=1, name="Now playing", renamed=False, epoch=1,
        paths=[str(profile / f"track-{index}.flac") for index in range(80)], origins=[],
        removed=[False] * 80, placement=["Original"] * 80, groups=[], order=list(range(80)),
        cursor=40, seconds=0, shuffle=False, repeat=0, stop_after=None)
    (profile / "data/chilpa/listenlist.json").write_text(json.dumps(dict(version=1, buffers=[buffer], active=1, selected=1, next=2)))
    smoke = profile / "queues.qml"
    template = (project / "crates/chilpa-qt/tests/smoke/queues.qml").read_text()
    smoke.write_text(template.replace("property int fixtureAlbum: 0", f"property int fixtureAlbum: {album_id}"))
    environment = dict(os.environ, XDG_CONFIG_HOME=str(profile / "config"), XDG_DATA_HOME=str(profile / "data"),
        XDG_CACHE_HOME=str(profile / "cache"), XDG_RUNTIME_DIR=str(profile / "runtime"),
        QT_QPA_PLATFORM="offscreen", CHILPA_RENDERER="software", CHILPA_QML_SMOKE=str(smoke))
    result = subprocess.run(["timeout", "--signal=TERM", "--kill-after=2s", "25s", "dbus-run-session", "--", str(binary)],
        env=environment, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    log = result.stdout
    errors = ("SMOKE FAIL", "ReferenceError:", "TypeError:", "QQmlApplicationEngine failed", "is not a type", "binding loop")
    if result.returncode or any(error in log for error in errors) or "SMOKE queues completed" not in log:
        print(log)
        sys.exit(1)
    print("\n".join(line for line in log.splitlines() if "SMOKE" in line))
