#!/usr/bin/env python3
"""I measure artist search, record navigation and history on a library snapshot."""
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile

fixture = Path(__file__).resolve().parent / "fixtures/artist-opening.qml"
with tempfile.TemporaryDirectory(prefix="chilpa-artist-opening-") as directory:
    profile = Path(directory)
    for name in ("config", "cache", "runtime", "data/chilpa"):
        (profile / name).mkdir(parents=True)
    (profile / "runtime").chmod(0o700)
    source = sqlite3.connect(Path(sys.argv[2]).resolve().as_uri() + "?mode=ro", uri=True)
    target = sqlite3.connect(profile / "data/chilpa/library.db")
    source.backup(target)
    source.close()
    artists = target.execute("""SELECT a.album_artist, count(DISTINCT a.id) n, min(a.id)
        FROM albums a JOIN tracks t ON t.album_id = a.id JOIN sources s ON s.id = t.source_id
        WHERE s.provider = 'local' AND s.enabled = 1 AND t.present = 1
          AND a.album_artist <> '' AND a.album_artist NOT LIKE 'Various%'
        GROUP BY a.album_artist HAVING n >= 5 ORDER BY n DESC LIMIT 2""").fetchall()
    target.close()
    if len(artists) != 2:
        sys.exit("The sample needs two artists with at least five local records each.")
    smoke = profile / "artists.qml"
    smoke.write_text(fixture.read_text().replace("property var artists: []", "property var artists: " + json.dumps(artists)))
    environment = dict(os.environ, XDG_CONFIG_HOME=str(profile / "config"), XDG_DATA_HOME=str(profile / "data"),
        XDG_CACHE_HOME=str(profile / "cache"), XDG_RUNTIME_DIR=str(profile / "runtime"),
        QT_QPA_PLATFORM="offscreen", CHILPA_RENDERER="software", CHILPA_QML_SMOKE=str(smoke),
        QT_LOGGING_RULES="chilpa.nav.timing=true")
    result = subprocess.run(["timeout", "--kill-after=2s", "35s", "dbus-run-session", "--", sys.argv[1]],
        env=environment, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    output = Path(sys.argv[3]).resolve() if len(sys.argv) > 3 else profile / "artist-opening.log"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(result.stdout)
    print("\n".join(line for line in result.stdout.splitlines() if "SMOKE" in line or "qrc:" in line or "navigation:" in line))
    errors = ("SMOKE FAIL", "ReferenceError:", "TypeError:", "Binding loop", "QQmlApplicationEngine failed", "is not a type")
    if result.returncode or any(error in result.stdout for error in errors) or "SMOKE artist opening completed" not in result.stdout:
        sys.exit(1)
