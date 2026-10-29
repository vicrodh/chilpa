#!/usr/bin/python3
"""A searchable destination that cannot resolve or identify an origin link."""
import json
import sys

with open("/plugin/item.json") as stream:
    item = json.load(stream)

for line in sys.stdin:
    request = json.loads(line)
    if "id" not in request:
        continue
    method = request.get("method")
    if method == "initialize":
        result = dict(protocol=1, capabilities=dict(search=True, pages=True))
    elif method == "search":
        result = dict(tracks=[item] if item["kind"] == "track" else [],
                      albums=[item] if item["kind"] == "album" else [])
    else:
        result = {}
    print(json.dumps(dict(jsonrpc="2.0", id=request["id"], result=result)), flush=True)
