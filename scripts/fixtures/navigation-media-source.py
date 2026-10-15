#!/usr/bin/python3
"""I expose two opaque spoken shows without any playback operation."""
import json
import sys

shows = [("show first /?#", "First opaque show"), ("show second /?#", "Second opaque show")]
for line in sys.stdin:
    request = json.loads(line)
    if "id" not in request:
        continue
    method = request.get("method", "")
    params = request.get("params") or {}
    if method == "initialize":
        result = dict(protocol=1, name="Spoken navigation fixture", capabilities=dict(search=True))
    elif method == "search":
        result = dict(albums=[dict(kind="album", ref="handle:" + identity, title=title,
                                  artist="Fixture publisher") for identity, title in shows])
    elif "browse" in method:
        identity = params.get("at", "")
        if identity:
            title = dict(shows).get(identity, "Unknown show")
            entries = [dict(id=identity + "/episode", title=title + " episode", kind="item",
                            playable=False, duration_ms=60000)]
        else:
            entries = [dict(id=identity, title=title, kind="folder") for identity, title in shows]
        result = dict(medium="podcasts", attribution="Disposable spoken directory", entries=entries)
    elif method in ("resolve", "chilpa_resolve", "track.resolve"):
        print(json.dumps(dict(jsonrpc="2.0", id=request["id"], error=dict(code=-32601, message="I do not expose playback"))), flush=True)
        continue
    else:
        result = {}
    print(json.dumps(dict(jsonrpc="2.0", id=request["id"], result=result)), flush=True)
