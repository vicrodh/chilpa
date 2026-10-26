#!/usr/bin/env python3
"""I check native notification teardown without mounting or writing a device."""
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile

root = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix="chilpa-storage-events-") as directory:
    temporary = Path(directory)
    environment = {key: value for key, value in os.environ.items() if not key.startswith("CHILPA_")}
    for key, folder in [("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data"), ("XDG_CACHE_HOME", "cache"), ("XDG_RUNTIME_DIR", "runtime"), ("TMPDIR", "tmp")]:
        path = temporary / folder
        path.mkdir(mode=0o700)
        environment[key] = str(path)
    environment["QT_QPA_PLATFORM"] = "offscreen"
    environment["QT_NO_XDG_DESKTOP_PORTAL"] = "1"
    qt_bin = os.environ.get("QT_BIN_DIR")
    executable = shutil.which("qtpaths6") or shutil.which("qtpaths")
    qt_bin = Path(qt_bin) if qt_bin else Path(executable).resolve().parent if executable else Path("/usr/lib64/qt6/bin")
    qt = subprocess.check_output([str(qt_bin / "qtpaths"), "--query", "QT_INSTALL_LIBEXECS"], text=True).strip()
    sources = root / "crates/chilpa-qt-devices"
    moc = temporary / "moc_storage_events.cpp"
    subprocess.run([str(Path(qt) / "moc"), str(sources / "cxx/storage_events.h"), "-o", str(moc)], check=True, env=environment)
    flags = shlex.split(subprocess.check_output(["pkg-config", "--cflags", "--libs", "Qt6Quick", "Qt6Qml", "Qt6Test"], text=True))
    if sys.platform == "darwin":
        flags += ["-framework", "DiskArbitration", "-framework", "IOKit"]
    binary = temporary / "storage-events"
    subprocess.run(["c++", "-std=c++17", "-fPIC", str(sources / "cxx/storage_events.cpp"), str(sources / "tests/storage_events.cpp"), str(moc), "-o", str(binary), *flags], check=True, env=environment)
    command = [str(binary)] if sys.platform == "darwin" else ["dbus-run-session", "--", str(binary)]
    subprocess.run(command, check=True, env=environment)
