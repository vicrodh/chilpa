#!/usr/bin/env python3
"""I compare catalog load/save memory in isolated Linux profiles without plugins.

Usage: scripts/check-catalog-memory.py --binary PATH --catalog PATH --output PATH
I read only the supplied catalog and optional SQLite snapshot. I never copy a
profile, settings, plugins or credentials. --malloc-profile adds glibc memusage
instrumentation; I compare instrumented runs only with other instrumented runs.
"""

import argparse
import gc
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import sqlite3
import struct
import subprocess
import sys
import tempfile
import time

MARKER = "CATALOG_MEMORY "
SAVES = ("memory-smoke-one", "memory-smoke-two")
VERIFY_PLUGIN = "memory-smoke-check"
VERIFY_KEY = VERIFY_PLUGIN + " section:catalog"
MEMUSAGE_ENTRY = struct.Struct("=QQII")


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def identity(pid):
    """I pair a PID with its kernel start time before sampling or signalling it."""
    try:
        fields = (Path("/proc") / str(pid) / "stat").read_text().rsplit(")", 1)[1].split()
        return int(fields[19])
    except (OSError, ValueError, IndexError):
        return None


def memory(pid, started):
    if identity(pid) != started:
        return None
    try:
        lines = (Path("/proc") / str(pid) / "smaps_rollup").read_text().splitlines()
        return {key: int(value.split()[0]) for line in lines if ":" in line
                for key, value in [line.split(":", 1)]
                if key in ("Rss", "Pss", "Anonymous", "Swap", "Private_Dirty", "Private_Clean")}
    except (OSError, ValueError):
        return None


def descendants(parent):
    """I inspect process parent IDs; some kernels omit task/children files."""
    relations = {}
    for entry in Path("/proc").iterdir():
        if not entry.name.isdigit():
            continue
        try:
            fields = (entry / "stat").read_text().rsplit(")", 1)[1].split()
            relations[int(entry.name)] = int(fields[1])
        except (OSError, ValueError, IndexError):
            pass
    found, pending = set(), {parent}
    while pending:
        children = {pid for pid, owner in relations.items() if owner in pending} - found
        found.update(children)
        pending = children
    return found


def stop_owned(owned):
    for sig in (signal.SIGTERM, signal.SIGKILL):
        for pid, started in owned.items():
            if identity(pid) == started:
                try:
                    os.kill(pid, sig)
                except ProcessLookupError:
                    pass
        if sig == signal.SIGTERM:
            deadline = time.monotonic() + 2
            while time.monotonic() < deadline and any(identity(p) == s for p, s in owned.items()):
                time.sleep(0.05)


def profile_sample(path):
    """I read the last complete glibc memusage entry, excluding its two headers.

    I use the format from glibc malloc/memusage.c: heap, stack, time_low,
    time_high. The heap is requested live bytes, not allocator arena capacity.
    A small buffer limits lag; this sample still precedes the latest allocation.
    """
    try:
        with path.open("rb") as stream:
            size = stream.seek(0, 2)
            records = size // MEMUSAGE_ENTRY.size
            if records <= 2:
                return None
            stream.seek((records - 1) * MEMUSAGE_ENTRY.size)
            heap, stack, _, _ = MEMUSAGE_ENTRY.unpack(stream.read(MEMUSAGE_ENTRY.size))
        return dict(live_requested_bytes=heap, sampled_stack_bytes=stack)
    except (OSError, struct.error):
        return None


def profile_peak(path):
    try:
        with path.open("rb") as stream:
            stream.seek(MEMUSAGE_ENTRY.size)
            heap, stack, _, _ = MEMUSAGE_ENTRY.unpack(stream.read(MEMUSAGE_ENTRY.size))
        return dict(peak_requested_bytes=heap, peak_sampled_stack_bytes=stack)
    except (OSError, struct.error):
        return None


def fixture(full_ui, expected, has_catalog):
    shell_import = 'import "qrc:/qt/qml/app/chilpa/qml" as Shell' if full_ui else ""
    root = "Shell.Main" if full_ui else "Item"
    return f'''import QtQuick
import app.chilpa
import app.chilpa.sources
{shell_import}
{root} {{
    property bool smokePlayback: Player.playing
    property int smokeStep: 0
    property int smokeAttempts: 0
    property bool smokeLoaded: false
    property var smokeExpected: {json.dumps(expected)}
    AlbumGridModel {{ id: memoryAlbums; kind: "source"; key: smokeExpected.key; columns: 2 }}
    TrackListModel {{ id: memoryTracks; kind: "source"; key: smokeExpected.key; limit: 2 }}
    function smokeMark(phase, ok) {{
        console.log("{MARKER}" + JSON.stringify({{phase: phase, ok: ok, playing: smokePlayback}}))
    }}
    function smokePresent() {{
        const page = JSON.parse(Sources.page(smokeExpected.key, 0))
        if (page.title !== smokeExpected.title || page.cards !== smokeExpected.cards || page.tracks !== smokeExpected.tracks)
            return false
        if (memoryAlbums.albums !== smokeExpected.cards || memoryTracks.count !== smokeExpected.tracks)
            return false
        for (let i = 0; i < smokeExpected.cards; ++i)
            if (memoryAlbums.albumId(i, 0) !== smokeExpected.card_ids[i] || memoryAlbums.title(i, 0) !== smokeExpected.card_titles[i])
                return false
        for (let i = 0; i < smokeExpected.tracks; ++i) {{
            const row = memoryTracks.index(i, 0)
            if (!memoryTracks.isTrack(i) || memoryTracks.data(row, 269) !== smokeExpected.track_ids[i] || memoryTracks.data(row, 258) !== smokeExpected.track_titles[i])
                return false
        }}
        return true
    }}
    Component.onCompleted: smokeMark("startup", !smokePlayback)
    Timer {{
        interval: 100; running: true; repeat: true
        onTriggered: {{
            const present = smokePresent()
            const emptyReady = !present && memoryAlbums.albums === 0 && memoryTracks.count === 0 && JSON.parse(Sources.page(smokeExpected.key, 0)).loading === false
            if ({str(has_catalog).lower()} ? present : emptyReady) {{
                smokeLoaded = true
                smokeMark("loaded", true)
                running = false
            }} else if (++smokeAttempts >= 60) {{
                smokeMark("loaded", false)
                Qt.exit(2)
            }} else {{
                memoryAlbums.contentRev++
                memoryTracks.filterRev++
            }}
        }}
    }}
    Timer {{
        interval: 4000; running: true; repeat: true
        onTriggered: {{
            if (smokeStep === 1 || smokeStep === 3) {{
                const ok = smokeLoaded && ({str(has_catalog).lower()} ? smokePresent() : true)
                smokeMark(smokeStep === 1 ? "save_one_requested" : "save_two_requested", ok)
                Sources.page(smokeStep === 1 ? "{SAVES[0]} root" : "{SAVES[1]} root", 0)
            }} else if (smokeStep === 5) {{
                smokeMark("completed", smokeLoaded && !smokePlayback)
                Qt.exit(0)
            }}
            smokeStep++
        }}
    }}
}}
'''


def retained_content(document, expected, has_catalog):
    """I check the persisted root and real entity records after every write."""
    if not has_catalog:
        return not document.get("cards") and not document.get("tracks")
    shelf = document.get("shelves", {}).get(VERIFY_KEY, {})
    if (shelf.get("title") != expected["title"] or shelf.get("cards") != expected["card_ids"]
            or shelf.get("tracks") != expected["track_ids"]):
        return False
    for name, ids, titles in (("cards", expected["card_ids"], expected["card_titles"]),
                              ("tracks", expected["track_ids"], expected["track_titles"])):
        for key, title in zip(ids, titles):
            item = document.get(name, {}).get(str(key), {})
            if item.get("id") != key or item.get("title") != title:
                return False
    return True


def database_backup(source, target):
    uri = source.resolve().as_uri() + "?mode=ro"
    with sqlite3.connect(uri, uri=True) as original:
        with sqlite3.connect(target) as snapshot:
            original.backup(snapshot)


def run_case(args, case, expected, profiler):
    with tempfile.TemporaryDirectory(prefix="chilpa-catalog-memory-") as directory:
        root = Path(directory)
        for name in ("config/chilpa", "data/chilpa", "cache/chilpa", "runtime"):
            (root / name).mkdir(parents=True)
        (root / "runtime").chmod(0o700)
        saved = root / "cache/chilpa/source-catalog.json"
        if case == "catalog":
            document = json.loads(args.catalog.read_text())
        else:
            document = {}
        # I protect four existing entities with one tiny collection root.
        # Historical page retention can change without invalidating verification.
        document.setdefault("drawn", {})[VERIFY_PLUGIN] = False
        if case == "catalog":
            document.setdefault("shelves", {})[VERIFY_KEY] = dict(
                title=expected["title"], cards=expected["card_ids"],
                tracks=expected["track_ids"], document={}, membership={}, read_at=0)
        saved.write_text(json.dumps(document, separators=(",", ":")))
        del document
        gc.collect()
        initial_disk_stamp = saved.stat().st_mtime_ns
        fixture_hash, fixture_bytes = digest(saved), saved.stat().st_size
        if args.database_snapshot:
            database_backup(args.database_snapshot, root / "data/chilpa/library.db")
        bus_config = root / "dbus.conf"
        bus_config.write_text('<busconfig><type>session</type><listen>unix:tmpdir=' +
                              str(root / "runtime") + '</listen><auth>EXTERNAL</auth>' +
                              '<policy context="default"><allow send_destination="*"/>' +
                              '<allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
        qml = root / "probe.qml"
        qml.write_text(fixture(args.full_ui, expected, case == "catalog"))
        env = dict(os.environ, XDG_CONFIG_HOME=str(root / "config"),
                   XDG_DATA_HOME=str(root / "data"), XDG_CACHE_HOME=str(root / "cache"),
                   XDG_RUNTIME_DIR=str(root / "runtime"), QT_QPA_PLATFORM="offscreen",
                   CHILPA_RENDERER="software", CHILPA_QML_SMOKE=str(qml))
        for key in ("CHILPA_GALLERY", "LD_PRELOAD", "MEMUSAGE_OUTPUT", "MEMUSAGE_PROG_NAME"):
            env.pop(key, None)
        profile = root / "malloc.dat"
        if profiler:
            env.update(LD_PRELOAD=str(profiler), MEMUSAGE_PROG_NAME=args.binary.name,
                       MEMUSAGE_OUTPUT=str(profile), MEMUSAGE_BUFFER_SIZE="32")
        samples, markers, owned = [], [], {}
        phase, seen, disk_stamp = "startup", 0, initial_disk_stamp
        invalid_writes = 0
        verified_saves = set()
        start = time.monotonic()
        timed_out = False
        with (root / "run.log").open("w+") as log:
            launcher = subprocess.Popen(["dbus-run-session", "--config-file=" + str(bus_config), "--", str(args.binary)],
                                        env=env, stdout=log, stderr=subprocess.STDOUT)
            owned[launcher.pid] = identity(launcher.pid)
            target = None
            try:
                while launcher.poll() is None:
                    elapsed = time.monotonic() - start
                    if elapsed > args.timeout:
                        timed_out = True
                        break
                    for pid in descendants(launcher.pid):
                        started = identity(pid)
                        if started is not None:
                            owned.setdefault(pid, started)
                        try:
                            if (Path("/proc") / str(pid) / "exe").resolve() == args.binary:
                                target = pid
                        except OSError:
                            pass
                    lines = (root / "run.log").read_text(errors="replace").splitlines()
                    for line in lines[seen:]:
                        if MARKER in line:
                            event = json.loads(line.split(MARKER, 1)[1])
                            event["seconds"] = round(elapsed, 3)
                            if event["phase"] == "loaded" and any(
                                    m["phase"] == "startup_compaction_complete" for m in markers):
                                event["phase"] = "loaded_after_startup_compaction"
                            markers.append(event)
                            phase = event["phase"]
                    seen = len(lines)
                    try:
                        stamp = saved.stat().st_mtime_ns
                        if stamp != disk_stamp:
                            disk_stamp = stamp
                            document = json.loads(saved.read_text())
                            retained = retained_content(document, expected, case == "catalog")
                            counts = {name: len(document.get(name, {}))
                                      for name in ("cards", "tracks", "shelves")}
                            first_requested = any(m["phase"] == "save_one_requested" for m in markers)
                            if not first_requested:
                                phase = "startup_compaction_complete"
                                valid = retained and not any(key in document.get("drawn", {}) for key in SAVES)
                                invalid_writes += not valid
                                markers.append(dict(phase=phase, ok=valid, seconds=round(elapsed, 3),
                                                    catalog_counts=counts))
                            for index, key in enumerate(SAVES, 1):
                                if key in document.get("drawn", {}) and index not in verified_saves:
                                    verified_saves.add(index)
                                    phase = f"save_{index}_complete"
                                    requested = "save_one_requested" if index == 1 else "save_two_requested"
                                    valid = retained and any(m["phase"] == requested for m in markers)
                                    invalid_writes += not valid
                                    markers.append(dict(phase=phase, ok=valid, seconds=round(elapsed, 3),
                                                        catalog_counts=counts))
                            del document
                    except (OSError, ValueError):
                        pass
                    if target is not None:
                        resident = memory(target, owned[target])
                        if resident:
                            sample = dict(seconds=round(elapsed, 3), phase=phase,
                                          memory_kib=resident)
                            allocation = profile_sample(profile) if profiler else None
                            if allocation:
                                sample["malloc"] = allocation
                            samples.append(sample)
                    time.sleep(args.interval)
                launcher.wait(timeout=5) if not timed_out else None
            finally:
                stop_owned(owned)
                launcher.wait(timeout=5)
            lines = (root / "run.log").read_text(errors="replace").splitlines()
        # I read the final marker after exit too, since it can follow the last poll.
        for line in lines[seen:]:
            if MARKER in line:
                event = json.loads(line.split(MARKER, 1)[1])
                event["seconds"] = round(time.monotonic() - start, 3)
                markers.append(event)
        failures = []
        if invalid_writes:
            failures.append("a catalog write lost verified content or preceded its save request")
        if timed_out:
            failures.append("probe timed out")
        if launcher.returncode != 0:
            failures.append(f"probe exit code {launcher.returncode}")
        for required in ("startup", "loaded", "completed"):
            acceptable = (required, "loaded_after_startup_compaction") if required == "loaded" else (required,)
            if not any(marker["phase"] in acceptable and marker["ok"] for marker in markers):
                failures.append(f"missing successful {required} marker")
        if any(not marker["ok"] or marker.get("playing", False) for marker in markers):
            failures.append("QML assertion failed or playback started")
        if verified_saves != {1, 2}:
            failures.append("both catalog writes were not verified on disk")
        if not samples:
            failures.append("no process memory samples")
        if profiler and not any("malloc" in sample for sample in samples):
            failures.append("no malloc profile samples")
        errors = ("ReferenceError:", "TypeError:", "QQmlApplicationEngine failed", "is not a type")
        if any(any(error in line for error in errors) for line in lines):
            failures.append("QML reported an error")
        summaries = {}
        for name in dict.fromkeys(sample["phase"] for sample in samples):
            selected = [sample for sample in samples if sample["phase"] == name]
            final = selected[-1]
            summaries[name] = dict(last=final, peak_rss_kib=max(s["memory_kib"]["Rss"] for s in selected))
        return dict(case=case, exit_code=launcher.returncode, failures=failures,
                    fixture_catalog_sha256=fixture_hash, fixture_catalog_bytes=fixture_bytes,
                    markers=markers, phases=summaries, samples=samples,
                    malloc_summary=profile_peak(profile) if profiler else None,
                    pre_save=next((sample for sample in reversed(samples)
                                   if sample["phase"] in ("loaded", "loaded_after_startup_compaction", "startup_compaction_complete")), None))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--catalog", type=Path, required=True)
    parser.add_argument("--database-snapshot", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--full-ui", action="store_true")
    parser.add_argument("--malloc-profile", action="store_true")
    parser.add_argument("--memusage-library", type=Path)
    parser.add_argument("--interval", type=float, default=0.25)
    parser.add_argument("--timeout", type=float, default=40)
    parser.add_argument("--max-catalog-rss-mib", type=float)
    parser.add_argument("--max-save-growth-mib", type=float)
    args = parser.parse_args()
    if sys.platform != "linux":
        parser.error("I require Linux process memory accounting")
    if args.interval <= 0 or args.timeout < 26:
        parser.error("I require a positive interval and a timeout of at least 26 seconds")
    args.binary = args.binary.resolve(strict=True)
    args.catalog = args.catalog.resolve(strict=True)
    if args.database_snapshot:
        args.database_snapshot = args.database_snapshot.resolve(strict=True)
    if args.output.resolve() in (args.binary, args.catalog, args.database_snapshot):
        parser.error("I require an output path separate from the inputs")
    if not shutil.which("dbus-run-session"):
        parser.error("I require dbus-run-session")
    profiler = None
    if args.malloc_profile:
        choices = ([args.memusage_library] if args.memusage_library else
                   [Path("/usr/lib64/libmemusage.so"), Path("/usr/lib/x86_64-linux-gnu/libmemusage.so")])
        profiler = next((p.resolve() for p in choices if p.is_file()), None)
        if profiler is None:
            parser.error("I could not find libmemusage; pass --memusage-library")
    catalog_hash = digest(args.catalog)
    document = json.loads(args.catalog.read_text())
    counts = {key: len(document.get(key, {})) for key in ("cards", "tracks", "shelves")}
    if any(key in document.get("drawn", {}) for key in SAVES):
        parser.error("I require unused memory smoke persistence markers")
    if VERIFY_PLUGIN in document.get("drawn", {}) or VERIFY_KEY in document.get("shelves", {}):
        parser.error("I require an unused memory verification namespace")
    # I retain only two real albums and two real tracks through a synthetic
    # collection shelf. I verify their IDs and titles through Qt's real models.
    cards = sorted((int(key), card) for key, card in document.get("cards", {}).items()
                   if card.get("kind") == "Album" and card.get("title"))[:2]
    tracks = sorted((int(key), track) for key, track in document.get("tracks", {}).items()
                    if track.get("title") and track.get("path"))[:2]
    if len(cards) != 2 or len(tracks) != 2:
        parser.error("I require at least two titled albums and two titled tracks")
    expected = dict(key=VERIFY_KEY, title="Memory smoke retained content", cards=2, tracks=2,
                    card_ids=[key for key, _ in cards], card_titles=[card["title"] for _, card in cards],
                    track_ids=[key for key, _ in tracks], track_titles=[track["title"] for _, track in tracks])
    del document, cards, tracks
    gc.collect()
    result = dict(protocol_version=2, script_sha256=digest(Path(__file__).resolve()),
                  binary=str(args.binary), binary_sha256=digest(args.binary),
                  catalog_sha256=catalog_hash, catalog_bytes=args.catalog.stat().st_size,
                  catalog_counts=counts, full_ui=args.full_ui,
                  sample_interval_seconds=args.interval, timeout_seconds=args.timeout,
                  database_snapshot=bool(args.database_snapshot), malloc_profile=bool(profiler),
                  fixture_transformation="I add a drawn=false synthetic section:catalog root referencing two existing albums and two existing tracks only in the loaded disposable fixture; the empty fixture has only its drawn=false flag.",
                  verification_counts=dict(cards=2, tracks=2),
                  measurement_note="I distinguish sampled load readiness, startup compaction and the steady interval before explicit saves. RSS includes mappings and allocator retention; memusage tracks requested live heap bytes with instrumentation overhead.")
    result["cases"] = [run_case(args, case, expected, profiler) for case in ("empty", "catalog")]
    failures = [failure for case in result["cases"] for failure in case["failures"]]
    if digest(args.catalog) != catalog_hash:
        failures.append("input catalog changed during the run")
    loaded = result["cases"][1]["pre_save"]
    saved = result["cases"][1]["phases"].get("save_2_complete", {}).get("last")
    if loaded and saved:
        verified_loaded = result["cases"][1]["phases"].get("loaded", {}).get("last")
        result["catalog_loaded_rss_mib"] = (verified_loaded["memory_kib"]["Rss"] / 1024
                                           if verified_loaded else None)
        result["catalog_pre_save_rss_mib"] = loaded["memory_kib"]["Rss"] / 1024
        result["catalog_pre_save_phase"] = loaded["phase"]
        result["catalog_save_growth_mib"] = (saved["memory_kib"]["Rss"] - loaded["memory_kib"]["Rss"]) / 1024
        for option, value in ((args.max_catalog_rss_mib, "catalog_pre_save_rss_mib"),
                              (args.max_save_growth_mib, "catalog_save_growth_mib")):
            if option is not None and result[value] > option:
                failures.append(f"{value} exceeds {option}")
    elif args.max_catalog_rss_mib is not None or args.max_save_growth_mib is not None:
        failures.append("missing phase for requested memory limit")
    result["failures"] = failures
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    for case in result["cases"]:
        print(json.dumps({k: v for k, v in case.items() if k not in ("samples", "markers", "phases", "pre_save")}))
        for phase, value in case["phases"].items():
            sample = value["last"]
            print(json.dumps(dict(case=case["case"], phase=phase, **sample["memory_kib"],
                                  **sample.get("malloc", {}))))
    return bool(failures)


if __name__ == "__main__":
    raise SystemExit(main())
