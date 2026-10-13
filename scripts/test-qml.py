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
qt = Path("/usr/lib64/qt6/bin")
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
    flags = shlex.split(subprocess.check_output(["pkg-config", "--cflags", "--libs", "Qt6Core"], text=True))
    subprocess.run(["c++", "-shared", "-fPIC", str(loader), "-o", str(temporary / "loader.so"), *flags], check=True)
    imports = temporary / "imports"
    shutil.copytree(root / "crates/chilpa-qt/tests/qml/imports", imports)
    shutil.copytree(root / "crates/chilpa-qt-kit/tests/qml/imports/app/chilpa/kit", imports / "app/chilpa/kit")
    environment = dict(os.environ, QT_QPA_PLATFORM="offscreen", CHILPA_QML_RESOURCES=str(temporary / "resources.rcc"),
                       LD_PRELOAD=str(temporary / "loader.so"))
    result = subprocess.run([str(qt / "qmltestrunner"), "-import", str(imports), "-input", str(test)], env=environment)
    sys.exit(result.returncode)
