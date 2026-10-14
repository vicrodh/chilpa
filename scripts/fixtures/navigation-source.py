#!/usr/bin/python3
import json
import sys
import threading
import time

output = threading.Lock()
with open("/plugin/image-base.json") as configuration:
    image_base = json.load(configuration)

def items(kind, size=24):
    singular = {"albums": "album", "tracks": "track", "artists": "artist", "labels": "label", "playlists": "playlist"}.get(kind, kind)
    return [dict(kind=singular, ref=f"{singular}:{i}", title=f"Fixture {singular} {i}", artist="Fixture artist", artist_ref="artist:0", album="Fixture record", album_ref=f"album:{i}", duration_ms=180000, image=f"{image_base}/{singular}-{i}.png") for i in range(size)]

def answer(request):
    method = request.get("method")
    params = request.get("params") or {}
    if method == "initialize":
        result = dict(protocol=1, capabilities=dict(pages=True, home=True, library=["albums", "tracks", "artists", "labels", "playlists"]))
    elif method == "page.get":
        time.sleep(1.2)
        result = dict(title="Fixture home", shelves=[dict(id=f"shelf-{i}", title=f"Module {i}", kind="album", items=items("album")) for i in range(6)])
    elif method == "library.list":
        kind = params.get("kind", "albums")
        time.sleep(dict(albums=0.8, tracks=0.3, artists=0.5, labels=0.4, playlists=0.6).get(kind, 0.5))
        result = dict(items=items(kind), total=24)
    else:
        result = {}
    with output:
        print(json.dumps(dict(jsonrpc="2.0", id=request["id"], result=result)), flush=True)

for line in sys.stdin:
    request = json.loads(line)
    if "id" in request:
        threading.Thread(target=answer, args=(request,)).start()
