#!/usr/bin/env python3
"""I extract and start the actual native package payloads in private profiles."""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--dist", type=Path, required=True)
parser.add_argument("--version", required=True)
parser.add_argument("--arch", choices=("x86_64", "aarch64"), required=True)
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
deb_arch = "amd64" if args.arch == "x86_64" else "arm64"
deb = args.dist.resolve() / f"chilpa_{args.version}_{deb_arch}.deb"
rpm = args.dist.resolve() / f"chilpa-{args.version}-1.{args.arch}.rpm"
assert subprocess.check_output(["dpkg-deb", "--field", str(deb), "Architecture"], text=True).strip() == deb_arch
assert subprocess.check_output(["rpm", "-qp", "--queryformat", "%{ARCH}", str(rpm)], text=True).strip() == args.arch
for kind, package in (("deb", deb), ("rpm", rpm)):
    with tempfile.TemporaryDirectory(prefix=f"chilpa-{kind}-smoke-") as directory:
        payload = Path(directory)
        if kind == "deb":
            subprocess.run(["dpkg-deb", "--extract", str(package), str(payload)], check=True)
        else:
            with tempfile.TemporaryFile() as archive:
                subprocess.run(["rpm2cpio", str(package)], stdout=archive, check=True)
                archive.seek(0)
                subprocess.run(["cpio", "--extract", "--make-directories", "--no-absolute-filenames"], cwd=payload, stdin=archive, check=True)
        binary = payload / "opt/chilpa/bin/chilpa-qt"
        assert os.readlink(payload / "usr/bin/chilpa-qt") == "/opt/chilpa/bin/chilpa-qt"
        assert (binary.parent / "qt.conf").is_file()
        assert (payload / "usr/share/applications/app.chilpa.Chilpa.desktop").is_file()
        assert (payload / "usr/share/icons/hicolor/512x512/apps/app.chilpa.Chilpa.png").is_file()
        environment = dict(os.environ)
        for key in ("LD_LIBRARY_PATH", "QT_PLUGIN_PATH", "QML_IMPORT_PATH", "QML2_IMPORT_PATH"):
            environment.pop(key, None)
        subprocess.run([str(root / "scripts/check-qml-startup.sh"), str(binary)], env=environment, check=True)
