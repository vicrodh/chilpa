#!/usr/bin/env python3
"""Check canonical names and raw role credits through a native source and real rows."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import textwrap

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary", type=Path)
parser.add_argument("output", type=Path)
parser.add_argument("--artist", default="Lead Artist")
args = parser.parse_args()
binary = args.binary.resolve(strict=True)
output = args.output.resolve()
output.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory(prefix="chilpa-credit-names-") as directory:
    profile = Path(directory)
    for name in ("config", "cache", "runtime", "data/chilpa/plugins/app.test.credit-names"):
        (profile / name).mkdir(parents=True)
    (profile / "runtime").chmod(0o700)
    folder = profile / "data/chilpa/plugins/app.test.credit-names"
    capabilities = [dict(type="register_source"), dict(type="register_panel")]
    manifest = dict(id="app.test.credit-names", name="Credit fixture", version="1.0.0", api_version=1,
                    kind="source", runtime="process", capabilities=capabilities)
    (folder / "manifest.json").write_text(json.dumps(manifest))
    (folder.parent / "approvals.json").write_text(json.dumps({manifest["id"]:
        dict(kind="source", runtime="process", capabilities=capabilities, auth=None)}))
    tracks = []
    for index, artist in enumerate((args.artist, "ABBA", "k.d. lang"), 1):
        roles = artist.upper() + ", MainArtist, Vocalist, Producer, Composer"
        roles += " - Technical Person, RecordingEngineer, VideoScriptEditor, DigitalEditingEngineer"
        roles += " - Backing Person, BackgroundVocalist, Producer - Mastering Person, MasteringEngineer"
        if index == 1:
            roles += " - Guest Artist, FeaturedArtist"
        tracks.append(dict(kind="track", ref=f"track:{index}", title=f"Fixture track {index}", artist=artist,
                           artist_ref=f"artist:{index}", album="Fixture record", album_ref="album:1",
                           duration_ms=180000, performers=roles))
    (folder / "tracks.json").write_text(json.dumps(tracks))
    (folder / "plugin").write_text(textwrap.dedent('''\
        #!/usr/bin/python3
        import json,sys
        with open("/plugin/tracks.json") as stream:
            tracks=json.load(stream)
        for line in sys.stdin:
            request=json.loads(line)
            if "id" not in request:continue
            if request.get("method")=="initialize":result=dict(protocol=1,capabilities=dict(pages=True))
            elif request.get("method")=="page.get":
                result=dict(title="Credit fixture",shelves=[dict(id="tracks",kind="track",items=tracks)])
            else:result={}
            print(json.dumps(dict(jsonrpc="2.0",id=request["id"],result=result)),flush=True)
        '''))
    (folder / "plugin").chmod(0o755)
    template = (Path(__file__).resolve().parent / "fixtures/track-credit-names.qml").read_text()
    smoke = profile / "credits.qml"
    smoke.write_text(template.replace('"EXPECTED_ARTIST"', json.dumps(args.artist))
                    .replace('"CAPTURE_PATH"', json.dumps(str(output / "credits.png"))))
    bus = profile / "dbus.conf"
    bus.write_text('<busconfig><type>session</type><listen>unix:tmpdir=' + str(profile / "runtime") +
                   '</listen><auth>EXTERNAL</auth><policy context="default">' +
                   '<allow send_destination="*"/><allow receive_sender="*"/>' +
                   '<allow own="*"/></policy></busconfig>')
    environment = dict(os.environ, XDG_CONFIG_HOME=str(profile / "config"), XDG_DATA_HOME=str(profile / "data"),
                       XDG_CACHE_HOME=str(profile / "cache"), XDG_RUNTIME_DIR=str(profile / "runtime"),
                       QT_QPA_PLATFORM="offscreen", CHILPA_RENDERER="software", CHILPA_QML_SMOKE=str(smoke))
    result = subprocess.run(["timeout", "--signal=TERM", "--kill-after=2s", "20s", "dbus-run-session", "--config-file=" + str(bus), "--", str(binary)],
                            env=environment, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (output / "credits.log").write_text(result.stdout)
    print("\n".join(line for line in result.stdout.splitlines() if "SMOKE" in line))
    if result.returncode or "SMOKE canonical credit names completed" not in result.stdout:
        raise SystemExit("Canonical credit names failed; see credits.log")
with binary.open("rb") as stream:
    checksum = hashlib.file_digest(stream, "sha256").hexdigest()
print(json.dumps(dict(binary=str(binary), sha256=checksum, passed=True)))
