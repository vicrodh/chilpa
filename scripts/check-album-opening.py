#!/usr/bin/env python3
"""I measure complete local record pages on a read-only library snapshot."""
import json
import os
import re
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile

fixture = Path(__file__).resolve().parent / "fixtures/album-opening.qml"
with tempfile.TemporaryDirectory(prefix="chilpa-album-opening-") as directory:
    profile = Path(directory)
    for name in ("config", "cache", "runtime", "data/chilpa"):
        (profile / name).mkdir(parents=True)
    (profile / "runtime").chmod(0o700)
    source = sqlite3.connect(Path(sys.argv[2]).resolve().as_uri() + "?mode=ro", uri=True)
    target = sqlite3.connect(profile / "data/chilpa/library.db")
    source.backup(target)
    source.close()
    query = """SELECT a.id, count(*) n FROM albums a JOIN tracks t ON t.album_id = a.id
        JOIN sources s ON s.id = t.source_id WHERE s.provider = 'local' AND s.enabled = 1 AND t.present = 1
        GROUP BY a.id"""
    small = target.execute(query + " HAVING n BETWEEN 5 AND 12 ORDER BY n, a.id LIMIT 1").fetchone()
    large = target.execute(query + " ORDER BY n DESC, a.id LIMIT 3").fetchall()
    if not small or not large or large[0][1] < 100:
        sys.exit("The navigation sample needs a small local record and one with at least 100 tracks.")
    albums = [small[0], *[row[0] for row in large], small[0], large[0][0]]
    titles = [target.execute("SELECT title FROM albums WHERE id = ?", [album]).fetchone()[0] for album in albums]
    target.close()
    smoke = profile / "albums.qml"
    smoke.write_text(fixture.read_text().replace("property var albums: []", "property var albums: " + json.dumps(albums)).replace("property var titles: []", "property var titles: " + json.dumps(titles)))
    environment = dict(os.environ, XDG_CONFIG_HOME=str(profile / "config"), XDG_DATA_HOME=str(profile / "data"), XDG_CACHE_HOME=str(profile / "cache"), XDG_RUNTIME_DIR=str(profile / "runtime"), QT_QPA_PLATFORM="offscreen", CHILPA_RENDERER="software", CHILPA_ALBUM_TIMINGS="1", CHILPA_QML_SMOKE=str(smoke))
    result = subprocess.run(["timeout", "--signal=TERM", "--kill-after=2s", "30s", "dbus-run-session", "--", sys.argv[1]], env=environment, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    output = Path(sys.argv[3]).resolve() if len(sys.argv) > 3 else profile / "album-opening.log"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(result.stdout)
    print("\n".join(line for line in result.stdout.splitlines() if "SMOKE" in line or "qrc:" in line or "album-worker" in line))
    errors = ("SMOKE FAIL", "ReferenceError:", "TypeError:", "binding loop", "QQmlApplicationEngine failed", "is not a type", "Type ")
    preference = re.search(r"SMOKE album preference persisted album=(\d+) version=(\d+)", result.stdout)
    if preference:
        copied = sqlite3.connect(profile / "data/chilpa/library.db")
        stored = copied.execute("SELECT count(*) FROM album_preferences WHERE album_id = ?", [int(preference[2])]).fetchone()[0]
        copied.close()
        if not stored:
            sys.exit("The version preference did not persist after its page was destroyed.")
    if result.returncode or any(error in result.stdout for error in errors) or "album opening completed samples=6 failures=0" not in result.stdout:
        sys.exit(1)
