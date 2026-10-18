#!/usr/bin/env python3
"""Run QML tests with the application's real resource paths and test bridges."""
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET

root = Path(__file__).resolve().parents[1]
test = Path(sys.argv[1]).resolve()
qtpaths = os.environ.get("QT_BIN_DIR")
if qtpaths:
    qt = Path(qtpaths)
else:
    executable = shutil.which("qtpaths6") or shutil.which("qtpaths")
    qt = Path(executable).resolve().parent if executable else Path("/usr/lib64/qt6/bin")
libexec = Path(subprocess.check_output([str(qt / "qtpaths"), "--query", "QT_INSTALL_LIBEXECS"], text=True).strip())
with tempfile.TemporaryDirectory(prefix="chilpa-qml-test-") as directory:
    temporary = Path(directory)
    resources = ET.Element("RCC")
    for crate in sorted((root / "crates").glob("chilpa-qt*")):
        module = crate.name.removeprefix("chilpa-qt").removeprefix("-").replace("-", "/")
        prefix = "/qt/qml/app/chilpa" + ("/" + module if module else "")
        collection = ET.SubElement(resources, "qresource", prefix=prefix)
        for folder in (crate / "qml", crate / "assets"):
            if not folder.is_dir():
                continue
            for path in sorted(folder.rglob("*")):
                if path.is_file():
                    ET.SubElement(collection, "file", alias=path.relative_to(crate).as_posix()).text = str(path)
    ET.ElementTree(resources).write(temporary / "resources.qrc", encoding="utf-8")
    subprocess.run([str(libexec / "rcc"), "--binary", str(temporary / "resources.qrc"), "-o", str(temporary / "resources.rcc")], check=True)
    loader = root / "crates/chilpa-qt-kit/tests/resource_loader.cpp"
    fence = root / "crates/chilpa-qt-kit/cxx/frame_fence.cpp"
    fence_header = fence.with_suffix(".h")
    fence_moc = temporary / "moc_frame_fence.cpp"
    subprocess.run([str(libexec / "moc"), str(fence_header), "-o", str(fence_moc)], check=True)
    flags = shlex.split(subprocess.check_output(["pkg-config", "--cflags", "--libs", "Qt6Quick", "Qt6Qml"], text=True))
    subprocess.run(["c++", "-std=c++17", "-shared", "-fPIC", str(loader), str(fence), str(fence_moc),
                    "-o", str(temporary / "loader.so"), *flags], check=True)
    imports = temporary / "imports"
    shutil.copytree(root / "crates/chilpa-qt/tests/qml/imports", imports)
    for crate in sorted((root / "crates").glob("chilpa-qt*")):
        modules = crate / "tests/qml/imports/app/chilpa"
        if modules.is_dir():
            for module in sorted(modules.iterdir()):
                if module.is_dir():
                    shutil.copytree(module, imports / "app/chilpa" / module.name, dirs_exist_ok=True)
    environment = dict(os.environ, QT_QPA_PLATFORM="offscreen", CHILPA_QML_RESOURCES=str(temporary / "resources.rcc"),
                       LD_PRELOAD=str(temporary / "loader.so"))
    result = subprocess.run([str(qt / "qmltestrunner"), "-import", str(imports), "-input", str(test)], env=environment)
    sys.exit(result.returncode)
