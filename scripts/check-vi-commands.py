#!/usr/bin/env python3
"""I check native Vi commands in an isolated profile without starting audio."""
import argparse
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile

arguments = argparse.ArgumentParser(description=__doc__)
arguments.add_argument("binary", type=Path)
arguments.add_argument("output", type=Path, nargs="?")
arguments.add_argument("--library", type=Path, required=True)
options = arguments.parse_args()
project = Path(__file__).resolve().parents[1]
binary = options.binary.resolve()
output = options.output.resolve() if options.output else None
if output:
    output.mkdir(parents=True, exist_ok=True)
library = options.library.resolve()
with tempfile.TemporaryDirectory(prefix="chilpa-vi-smoke-") as directory:
    profile = Path(directory)
    for folder in ("config", "cache", "runtime", "data/chilpa"):
        (profile / folder).mkdir(parents=True)
    (profile / "runtime").chmod(0o700)
    source = sqlite3.connect(library.as_uri() + "?mode=ro", uri=True)
    target = sqlite3.connect(profile / "data/chilpa/library.db")
    source.backup(target)
    source.close()
    target.execute("INSERT OR IGNORE INTO sources(provider, instance) VALUES ('local', 'vi-smoke')")
    source_id = target.execute("SELECT id FROM sources WHERE provider='local' AND instance='vi-smoke'").fetchone()[0]
    album_id = target.execute("SELECT coalesce(max(id),0)+1 FROM albums").fetchone()[0]
    first_id = target.execute("SELECT coalesce(max(id),0)+1 FROM tracks").fetchone()[0]
    title = "Vi smoke fixture"
    target.execute("INSERT INTO albums(id,title,album_artist,folder,version_key) VALUES (?,?,'Fixture artist',?,?)",
                   (album_id, title, str(profile), f"release:{source_id}:{len(title)}:{title}{profile}"))
    paths = [str(profile / f"track-{index}.flac") for index in range(100)]
    for path in paths:
        Path(path).touch()
    target.executemany("""INSERT INTO tracks(id,source_id,external_id,title,artist,album_id,
        disc_number,track_number,duration_ms,added_at,album_title,release_part)
        VALUES (?,?,?,?,'Fixture artist',?,1,?,180000,1,'Vi smoke fixture',?)""",
        ((first_id+i, source_id, path, f"Vi fixture track {i}", album_id, i+1, str(profile)) for i,path in enumerate(paths)))
    target.commit()
    target.close()
    buffer = dict(id=1, name="Vi fixture queue", renamed=True, epoch=1, paths=paths[:20], origins=[],
        removed=[False]*20, placement=["Original"]*20, groups=[], order=list(range(20)),
        cursor=0, seconds=0, shuffle=False, repeat=0, stop_after=None)
    (profile / "data/chilpa/listenlist.json").write_text(json.dumps(dict(version=1,buffers=[buffer],active=1,selected=1,next=2)))
    smoke = profile / "vi-commands.qml"
    template = (project / "scripts/fixtures/vi-commands.qml").read_text()
    smoke.write_text(template.replace("property int fixtureAlbum: 0", f"property int fixtureAlbum: {album_id}"))
    environment = dict(os.environ, XDG_CONFIG_HOME=str(profile/"config"), XDG_DATA_HOME=str(profile/"data"),
        XDG_CACHE_HOME=str(profile/"cache"), XDG_RUNTIME_DIR=str(profile/"runtime"),
        QT_QPA_PLATFORM="offscreen", CHILPA_RENDERER="software", CHILPA_QML_SMOKE=str(smoke))
    try:
        result = subprocess.run(["dbus-run-session", "--", str(binary)], env=environment, timeout=30,
            text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        log, code = result.stdout, result.returncode
    except subprocess.TimeoutExpired as error:
        log = (error.stdout or b"").decode() if isinstance(error.stdout, bytes) else error.stdout or ""
        log += "\nSMOKE FAIL Vi commands timed out\n"
        code = 1
    if output:
        (output / "vi-commands.log").write_text(log)
    errors = ("SMOKE FAIL", "ReferenceError:", "TypeError:", "QQmlApplicationEngine failed", "is not a type", "binding loop")
    if code == 0:
        with sqlite3.connect((profile / "data/chilpa/library.db").as_uri() + "?mode=ro", uri=True) as saved:
            named = saved.execute("SELECT id FROM playlists WHERE name='Vi smoke saved queue'").fetchone()
            count = saved.execute("SELECT count(*) FROM playlist_tracks WHERE playlist_id=?", (named[0],)).fetchone()[0] if named else 0
        if count != 20:
            log += f"\nSMOKE FAIL named queue saved {count} tracks instead of 20\n"
    failed = code or any(error in log for error in errors) or "SMOKE vi commands completed" not in log
    if failed:
        print(log)
        sys.exit(1)
    print("\n".join(line for line in log.splitlines() if "SMOKE" in line))
