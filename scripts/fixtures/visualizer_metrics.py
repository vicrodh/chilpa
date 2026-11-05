"""Sample only the owned Qt process; reuse the Linux visualizer metric reader."""

from contextlib import contextmanager
import importlib.util
import json
from pathlib import Path
import threading


@contextmanager
def measure(pid_file, log_path, output):
    source = Path(__file__).resolve().parents[1] / "compare-compact-visualizer.py"
    spec = importlib.util.spec_from_file_location("visualizer_counters", source)
    reader = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(reader)
    stopped = threading.Event()
    points, errors = [], []

    def sample():
        phase, offset, identity = "warmup", 0, None
        try:
            with log_path.open() as log:
                while not stopped.wait(0.2):
                    log.seek(offset)
                    for line in log:
                        if "SMOKE BENCH phase " in line:
                            phase = line.rstrip().rsplit(" ", 1)[1]
                    offset = log.tell()
                    if not pid_file.exists():
                        continue
                    point = reader.sample(int(pid_file.read_text()))
                    if point is None:
                        continue
                    if identity is None:
                        identity = point["start_time"]
                    if identity != point["start_time"]:
                        raise RuntimeError("The measured Qt process changed identity")
                    point["phase"] = phase
                    points.append(point)
                    if len(points) >= 384:
                        raise RuntimeError("The visualizer benchmark exceeded its sample budget")
        except Exception as error:
            errors.append(str(error))

    worker = threading.Thread(target=sample, daemon=True)
    worker.start()
    try:
        yield
    finally:
        stopped.set()
        worker.join()
        report = {phase: reader.phase_summary(points, phase) for phase in ("active", "idle", "paused")}
        for summary in report.values():
            if summary and any(value < 0 for value in summary["gpu_engine_percent"].values()):
                summary["gpu_engine_percent"] = None
                summary["gpu_counter_reset"] = True
        (output / "metrics.json").write_text(json.dumps({"phases": report, "samples": points, "errors": errors}, indent=2) + "\n")
    if errors or any(report[phase] is None for phase in report):
        raise RuntimeError("Visualizer metrics were incomplete: " + "; ".join(errors))
