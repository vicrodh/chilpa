"""Versioned, optional beets adapter. Input contains metadata, never music paths.

The host runs this with an explicitly selected Python interpreter and -I.
Only the bundled defaults and the allowlisted metadata source are loaded.
No Library, importer, tag writer or personal plugin configuration is used.
"""
import contextlib
import copy
import json
import math
import os
import sys
import threading

PROTOCOL = 1
BEETS_VERSION = "2.14.1"
MAX_INPUT = 1024 * 1024
MAX_TRACKS = 1024
MAX_CANDIDATES = 5
MAX_GENRES = 5
MAX_IDS = 8
SEARCH_LIMIT = 10
# The layer of a hybrid SACD that repeats the stereo one as a CD.
SACD_IGNORED = ["Hybrid SACD (CD layer)"]
MAX_OUTPUT = 8 * 1024 * 1024


class AdapterError(Exception):
    pass


def text(value):
    value = str(value) if value is not None else ""
    if len(value) > 4096:
        raise AdapterError("output_limit")
    return value


def strings(value):
    if not value:
        return []
    if isinstance(value, (list, tuple)):
        if len(value) > 128:
            raise AdapterError("output_limit")
        return [text(item) for item in value]
    return [text(value)]


def integer(value):
    try:
        return max(0, int(value or 0))
    except (ValueError, TypeError, OverflowError):
        return 0


def credits(info):
    names = strings(getattr(info, "artists", None)) or strings(getattr(info, "artist", None))
    ids = strings(getattr(info, "artists_ids", None)) or strings(getattr(info, "artist_id", None))
    return [{"name": name, "id": ids[index] if index < len(ids) else "", "join": " & " if index + 1 < len(names) else ""}
            for index, name in enumerate(names)]


def validate(request):
    if not isinstance(request, dict) or request.get("protocol") != PROTOCOL:
        raise AdapterError("invalid_request")
    if request.get("operation") == "probe":
        return
    if request.get("operation") != "lookup" or request.get("source") != "musicbrainz":
        raise AdapterError("unsupported_source")
    tracks = request.get("tracks")
    if not isinstance(tracks, list) or not 1 <= len(tracks) <= MAX_TRACKS:
        raise AdapterError("track_limit")
    ids = [track.get("id") for track in tracks if isinstance(track, dict)]
    if len(ids) != len(tracks) or any(type(value) is not int for value in ids) or len(set(ids)) != len(ids):
        raise AdapterError("invalid_request")
    for row in [request, *tracks]:
        for field in ("artist", "album", "title", "catalog_number", "release_id", "recording_id", "media", "label"):
            if not isinstance(row.get(field, ""), str) or len(row.get(field, "")) > 4096:
                raise AdapterError("invalid_request")
    ids = request.get("release_ids", [])
    if not isinstance(ids, list) or len(ids) > MAX_IDS or any(not isinstance(value, str) or len(value) > 64 for value in ids):
        raise AdapterError("invalid_request")
    if type(request.get("year", 0)) is not int or type(request.get("strict", True)) is not bool:
        raise AdapterError("invalid_request")


def date(info):
    year = integer(getattr(info, "year", 0))
    month = integer(getattr(info, "month", 0))
    day = integer(getattr(info, "day", 0))
    if not year:
        return ""
    return f"{year:04d}" + (f"-{month:02d}" if month else "") + (f"-{day:02d}" if month and day else "")


def scored(match, source, items, album_distance, assign_items):
    """How the files fit the discs of the release they land on, not the
    whole release: a disc of a set, or one layer of a hybrid SACD, is not
    worse for the discs it does not hold. Files of one disc are tried on
    each disc alone, and keep the one they fit best."""
    def fit(tracks, pairs, unmatched):
        formats = {text(getattr(track, "media", "")) for track in tracks}
        info = copy.copy(match.info)
        info.tracks = tracks
        if len(formats) == 1:
            info.media = next(iter(formats))
        return float(album_distance(source.data, info, pairs, unmatched)), next(iter(formats)) if len(formats) == 1 else ""
    discs = {integer(item.disc) for item in items}
    if len(discs) == 1:
        local = discs.pop()
        best = None
        for medium in sorted({integer(getattr(track, "medium", 0)) for track in match.info.tracks}):
            originals = [track for track in match.info.tracks if integer(getattr(track, "medium", 0)) == medium]
            copies = []
            for track in originals:
                moved = copy.copy(track)
                if local:
                    moved.medium = local
                copies.append(moved)
            pairs, extra_items, _ = assign_items(items, copies)
            distance, media = fit(copies, pairs, len(extra_items))
            if best is None or distance < best[0]:
                back = {id(moved): track for moved, track in zip(copies, originals)}
                best = (distance, [medium], media, {item: back[id(track)] for item, track in pairs})
        if best is not None:
            return best
    media = sorted({integer(getattr(track, "medium", 0)) for track in match.mapping.values()})
    tracks = [track for track in match.info.tracks if integer(getattr(track, "medium", 0)) in media] or list(match.info.tracks)
    distance, formats = fit(tracks, list(match.mapping.items()), len(match.extra_items))
    return distance, media, formats, dict(match.mapping)


def candidate(match, item_ids, fitted):
    info = match.info
    if text(getattr(info, "data_source", "")).lower() != "musicbrainz":
        raise AdapterError("unsupported_source")
    if len(info.tracks) > MAX_TRACKS:
        return None
    distance, discs, media, mapping = fitted
    positions = {id(track): index for index, track in enumerate(info.tracks)}
    mappings = []
    for item, track in mapping.items():
        if id(item) not in item_ids or id(track) not in positions:
            raise AdapterError("invalid_mapping")
        mappings.append({"track": item_ids[id(item)], "index": positions[id(track)],
                         "composers": strings(getattr(track, "composers", None)),
                         "work": text(getattr(track, "work", ""))})
    whole = float(match.distance)
    if not math.isfinite(distance) or not math.isfinite(whole):
        raise AdapterError("invalid_score")
    tracks = []
    for track in info.tracks:
        tracks.append({"disc": min(65535, integer(getattr(track, "medium", 0))),
                       "number": min(65535, integer(getattr(track, "medium_index", 0) or getattr(track, "index", 0))),
                       "title": text(getattr(track, "title", "")),
                       "duration_ms": min(4294967295, integer((getattr(track, "length", 0) or 0) * 1000)),
                       "credits": credits(track), "recording_id": text(getattr(track, "track_id", "")),
                       "track_id": text(getattr(track, "release_track_id", ""))})
    release = {"provider": "musicbrainz", "id": text(info.album_id), "title": text(info.album),
               "credits": credits(info), "date": date(info),
               "genres": strings(list(getattr(info, "genres", None) or [])[:MAX_GENRES]),
               "label": text(getattr(info, "label", "")), "catalog_number": text(getattr(info, "catalognum", "")),
               "catalog_numbers": [], "country": text(getattr(info, "country", "")), "barcode": text(getattr(info, "barcode", "")),
               "discs": min(65535, integer(getattr(info, "mediums", 0))), "media_titles": [],
               "release_group_id": text(getattr(info, "releasegroup_id", "")),
               "url": text(getattr(info, "data_url", "")), "tracks": tracks}
    return {"release": release, "score": max(0, min(100, round((1 - distance) * 100))),
            "whole": max(0, min(100, round((1 - whole) * 100))), "discs": [min(65535, disc) for disc in discs][:64],
            "format": text(media), "mappings": sorted(mappings, key=lambda mapping: mapping["track"])}


def lookup(request):
    import beets

    # An environment the listener chose runs the beets it has: the host says
    # it was not tested with it.
    if request.get("strict", True) and beets.__version__ != BEETS_VERSION:
        raise AdapterError("incompatible_version")
    beets.config.read(user=False)
    beets.config["include"].set([])
    beets.config["plugins"].set(["musicbrainz"])
    # The source leaves genres out unless asked: without this a release never names any.
    beets.config["musicbrainz"]["genres"].set(True)
    beets.config["musicbrainz"]["search_limit"].set(SEARCH_LIMIT)
    # DSD files are an SACD: its editions lead, and the CD layer of a hybrid
    # is not taken for the files' own.
    if request.get("media") == "SACD":
        beets.config["match"]["preferred"]["media"].set(["(Hybrid )?SACD"])
        beets.config["match"]["ignored_media"].set(SACD_IGNORED)
    if request["operation"] == "probe":
        return {"protocol": PROTOCOL, "beets_version": beets.__version__, "candidates": [], "recommendation": "none"}

    from beets import plugins
    from beets.autotag import Source, tag_album
    from beets.autotag.distance import distance as album_distance
    from beets.autotag.match import assign_items
    from beets.library import Item

    plugins.load_plugins()
    items = []
    item_ids = {}
    for row in request["tracks"]:
        # These virtual paths cannot address the listener's music files.
        item = Item(path=f"chilpa://track/{row['id']}".encode(), title=row.get("title", ""),
                    artist=row.get("artist", ""), album=request.get("album", ""), albumartist=request.get("artist", ""),
                    track=integer(row.get("number")), disc=integer(row.get("disc")), length=integer(row.get("duration_ms")) / 1000,
                    catalognum=request.get("catalog_number", ""), mb_albumid=request.get("release_id", ""),
                    mb_trackid=row.get("recording_id", ""), year=integer(request.get("year")), label=request.get("label", ""))
        items.append(item)
        item_ids[id(item)] = row["id"]
    source = Source.from_items(items)
    proposal = tag_album(source, search_artist=request.get("artist"), search_name=request.get("album"))
    matches = list(proposal.candidates)
    # What the host's own search found, by catalog number first, is scored
    # too: beets' search can miss the edition the files are.
    known = {text(match.info.album_id) for match in matches}
    extra = [value for value in request.get("release_ids", []) if value and value not in known]
    if extra:
        matches.extend(match for match in tag_album(source, search_ids=extra).candidates if text(match.info.album_id) not in known)
    fitted = [(scored(match, source, items, album_distance, assign_items), match) for match in matches]
    fitted.sort(key=lambda pair: (pair[0][0], float(pair[1].distance)))
    candidates = []
    for fit, match in fitted:
        converted = candidate(match, item_ids, fit)
        if converted:
            candidates.append(converted)
        if len(candidates) == MAX_CANDIDATES:
            break
    return {"protocol": PROTOCOL, "beets_version": beets.__version__, "candidates": candidates,
            "recommendation": proposal.recommendation.name}


def main():
    try:
        payload = sys.stdin.buffer.readline(MAX_INPUT + 2)
        if not payload.endswith(b"\n"):
            raise AdapterError("invalid_request")
        payload = payload[:-1]
        if len(payload) > MAX_INPUT:
            raise AdapterError("input_limit")
        # The owner keeps the input pipe open for the job's lifetime. EOF
        # means the application exited, including abrupt process termination.
        def owner_closed():
            while sys.stdin.buffer.read(1):
                pass
            os._exit(0)
        threading.Thread(target=owner_closed, name="beets-owner", daemon=True).start()
        request = json.loads(payload)
        validate(request)
        # Dependencies may log; stdout carries precisely one protocol object.
        with contextlib.redirect_stdout(sys.stderr):
            result = lookup(request)
        encoded = json.dumps(result, ensure_ascii=True, allow_nan=False, separators=(",", ":")).encode()
        if len(encoded) > MAX_OUTPUT:
            raise AdapterError("output_limit")
    except ModuleNotFoundError:
        encoded = json.dumps({"protocol": PROTOCOL, "error": "unavailable"}).encode()
    except AdapterError as error:
        encoded = json.dumps({"protocol": PROTOCOL, "error": str(error)}).encode()
    except (ValueError, TypeError, KeyError):
        encoded = json.dumps({"protocol": PROTOCOL, "error": "invalid_request"}).encode()
    except Exception:
        encoded = json.dumps({"protocol": PROTOCOL, "error": "lookup_failed"}).encode()
    sys.stdout.buffer.write(encoded + b"\n")
    sys.stdout.buffer.flush()
    # The daemon may hold stdin's buffered-reader lock. No user database or
    # tag writer was opened; exit after flushing the single result frame.
    os._exit(0)


if __name__ == "__main__":
    main()
