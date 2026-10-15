#!/usr/bin/env python3
"""I check one built application across browsing, queues, commands and scenes."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary", type=Path)
parser.add_argument("library", type=Path)
parser.add_argument("output", type=Path)
parser.add_argument("--stage", action="append", help="Run only the named stages after a targeted fix.")
parser.add_argument("--filter-budget-ms", type=int, default=700)
parser.add_argument("--process-plugin", type=Path)
parser.add_argument("--plugin-smoke-script", type=Path)
parser.add_argument("--plugin-package", type=Path)
parser.add_argument("--dsp-package", type=Path)
parser.add_argument("--additional-host-smoke", action="append", nargs=2, metavar=("SCRIPT", "PACKAGE"))
parser.add_argument("--vulkan-host-display", action="store_true")
args = parser.parse_args()
project = Path(__file__).resolve().parents[1]
binary = args.binary.resolve(strict=True)
library = args.library.resolve(strict=True)
output = args.output.resolve()
output.mkdir(parents=True, exist_ok=True)
if bool(args.process_plugin) != bool(args.plugin_smoke_script):
    parser.error("--process-plugin and --plugin-smoke-script must be supplied together")

python = sys.executable
stages = {
    "filters": [python, "scripts/check-library-filtering.py", str(binary), str(library),
                "--budget-ms", str(args.filter_budget_ms)],
    "navigation": ["bash", "scripts/check-navigation.sh", str(binary), str(library)],
    "albums": [python, "scripts/check-album-opening.py", str(binary), str(library),
               str(output / "album-details.log")],
    "queues": [python, "scripts/check-queues.py", str(binary), str(library)],
    "sources": [python, "scripts/check-source-navigation.py", str(binary), str(output / "sources")],
    "commands": [python, "scripts/check-vi-commands.py", str(binary), str(output / "commands"),
                 "--library", str(library)],
    "covers": [python, "scripts/check-immersive-scenes.py", str(binary), str(output / "covers"), "--covers-only"],
    "covers-vulkan": [python, "scripts/check-immersive-scenes.py", str(binary),
                      str(output / "covers-vulkan"), "--covers-only", "--backend", "vulkan"],
    "scenes": [python, "scripts/check-immersive-scenes.py", str(binary), str(output / "scenes")],
    "scenes-vulkan": [python, "scripts/check-immersive-scenes.py", str(binary),
                      str(output / "scenes-vulkan"), "--backend", "vulkan"],
    "music-views": [python, "scripts/check-live-visualizer.py", str(binary), str(output / "music-views")],
    "music-views-vulkan": [python, "scripts/check-live-visualizer.py", str(binary),
                           str(output / "music-views-vulkan"), "--backend", "vulkan"],
    "focus-panels": [python, "scripts/check-focus-panels.py", str(binary), str(library), str(output / "focus-panels")],
    "focus-panels-vulkan": [python, "scripts/check-focus-panels.py", str(binary), str(library),
                            str(output / "focus-panels-vulkan"), "--backend", "vulkan"],
}
if args.vulkan_host_display:
    stages["covers-vulkan"].append("--host-display")
    stages["scenes-vulkan"].append("--host-display")
    stages["music-views"].append("--host-display")
    stages["music-views-vulkan"].append("--host-display")
    stages["focus-panels"].append("--host-display")
    stages["focus-panels-vulkan"].append("--host-display")
if args.plugin_package:
    stages["process-host"] = [python, "scripts/check-process-plugin.py", str(binary),
                              str(args.plugin_package.resolve(strict=True)), str(output / "process-host")]
if args.process_plugin:
    stages["process-plugin"] = [python, str(args.plugin_smoke_script.resolve(strict=True)),
                                str(args.process_plugin.resolve(strict=True))]
if args.dsp_package:
    stages["dsp"] = [python, "scripts/check-dsp.py", str(binary),
                     str(args.dsp_package.resolve(strict=True)), str(output / "dsp")]
    for name in ("music-views", "music-views-vulkan"):
        stages[name].extend(["--dsp-package", str(args.dsp_package.resolve(strict=True))])
for index, (script, package) in enumerate(args.additional_host_smoke or []):
    name = f"additional-host-{index + 1}"
    stages[name] = [python, str(Path(script).resolve(strict=True)), str(binary),
                    str(Path(package).resolve(strict=True)), str(output / name)]
selected = args.stage or list(stages)
unknown = set(selected) - stages.keys()
if unknown:
    parser.error("unknown stages: " + ", ".join(sorted(unknown)))

with binary.open("rb") as stream:
    digest = hashlib.file_digest(stream, "sha256").hexdigest()
report = {
    "started_at": datetime.now(timezone.utc).isoformat(),
    "binary": str(binary),
    "sha256": digest,
    "library": str(library),
    "stages": [],
}
report_path = output / "report.json"
if args.stage and report_path.exists():
    try:
        previous_report = json.loads(report_path.read_text())
    except (OSError, ValueError) as error:
        parser.error(f"cannot resume the existing report: {error}")
    if not isinstance(previous_report, dict) or any(
            previous_report.get(key) != report[key] for key in ("binary", "sha256", "library")):
        parser.error("cannot resume: binary, sha256 and library must match the existing report")
    previous_stages = previous_report.get("stages")
    if (not isinstance(previous_stages, list)
            or any(not isinstance(stage, dict) or not isinstance(stage.get("name"), str)
                   or not isinstance(stage.get("exit_code"), int) for stage in previous_stages)
            or len({stage["name"] for stage in previous_stages}) != len(previous_stages)):
        parser.error("cannot resume: the existing report has invalid stage records")
    report = previous_report
for name in selected:
    command = stages[name]
    print(f"SMOKE integrated starting {name}", flush=True)
    started = time.monotonic()
    try:
        result = subprocess.run(command, cwd=project, text=True, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT, timeout=300)
        code, log = result.returncode, result.stdout
    except subprocess.TimeoutExpired as error:
        code = 124
        log = error.stdout or ""
        if isinstance(log, bytes):
            log = log.decode(errors="replace")
        log += "\nSMOKE FAIL integrated stage timed out\n"
    elapsed = round(time.monotonic() - started, 3)
    log_path = output / f"{name}.log"
    record = {"name": name, "exit_code": code, "seconds": elapsed, "log": str(log_path)}
    previous_index = next((index for index, stage in enumerate(report["stages"])
                           if stage["name"] == name), None)
    if previous_index is not None:
        previous = dict(report["stages"][previous_index])
        attempts = previous.pop("attempts", [])
        if log_path.is_file():
            archived_log = output / f"{name}.attempt-{len(attempts) + 1}.log"
            archived_log.write_bytes(log_path.read_bytes())
            previous["log"] = str(archived_log)
        record["attempts"] = attempts + [previous]
        report["stages"][previous_index] = record
    else:
        report["stages"].append(record)
    log_path.write_text(log)
    for line in log.splitlines():
        if "SMOKE" in line:
            print(line, flush=True)
    print(f"SMOKE integrated {name} exit={code} elapsed={elapsed}s", flush=True)
    report["passed"] = all(stage["exit_code"] == 0 for stage in report["stages"])
    report_path.write_text(json.dumps(report, indent=2) + "\n")

print(f"SMOKE integrated completed stages={len(report['stages'])} passed={report['passed']}")
print(output / "report.json")
raise SystemExit(0 if report["passed"] else 1)
