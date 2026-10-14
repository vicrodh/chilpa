#!/usr/bin/env python3
"""I package one Linux binary as a tarball and a self-contained, updateable AppImage."""
import argparse
import hashlib
import gzip
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument("--binary", type=Path, required=True)
parser.add_argument("--version", required=True)
parser.add_argument("--repository", required=True)
parser.add_argument("--tools", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
parser.add_argument("--glibc-max", help="Fail when the binary requires a newer glibc, such as 2.39.")
args = parser.parse_args()
if not re.fullmatch(r"\d+\.\d+\.\d+", args.version):
    parser.error("I require a stable semantic version: X.Y.Z.")
if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", args.repository):
    parser.error("I require a repository in owner/name form.")
args.binary = args.binary.resolve(strict=True)
args.tools = args.tools.resolve(strict=True)
args.output = args.output.resolve()
args.output.mkdir(parents=True, exist_ok=True)
header = subprocess.check_output(["readelf", "-h", str(args.binary)], text=True)
if "Advanced Micro Devices X86-64" not in header:
    parser.error("This initial Linux package targets x86_64.")
if args.glibc_max:
    symbols = subprocess.check_output(["readelf", "--version-info", str(args.binary)], text=True)
    versions = [tuple(map(int, match.split("."))) for match in re.findall(r"GLIBC_(\d+\.\d+)", symbols)]
    if versions and max(versions) > tuple(map(int, args.glibc_max.split("."))):
        raise SystemExit(f"The binary requires glibc {'.'.join(map(str, max(versions)))}.")
locked = json.loads((root / "packaging/linux/tools.lock.json").read_text())
for tool in locked.values():
    path = args.tools / tool["asset"]
    if hashlib.file_digest(path.open("rb"), "sha256").hexdigest() != tool["sha256"]:
        raise SystemExit(f"Unverified packaging tool: {path.name}")

epoch = int(os.environ.get("SOURCE_DATE_EPOCH", subprocess.check_output(["git", "show", "-s", "--format=%ct", "HEAD"], cwd=root, text=True).strip()))
commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
manifest = {"version": args.version, "source_commit": commit, "platform": "linux-x86_64", "source_date_epoch": epoch,
            "plugin_protocol": json.loads((root / "vendor/chilpa-plugin-api/ORIGIN.json").read_text()),
            "binary_sha256": hashlib.file_digest(args.binary.open("rb"), "sha256").hexdigest()}
with tempfile.TemporaryDirectory(prefix="chilpa-package-", dir=args.output.parent) as directory:
    work = Path(directory)
    plain = work / f"chilpa-{args.version}"
    plain.mkdir()
    shutil.copy2(args.binary, plain / "chilpa-qt")
    shutil.copy2(root / "LICENSE", plain / "LICENSE")
    notices = plain / "licenses"
    notices.mkdir()
    shutil.copy2(root / "vendor/chilpa-plugin-api/LICENSE", notices / "plugin-protocol-MIT.txt")
    shutil.copy2(root / "packaging/linux/Inter-OFL.txt", notices / "Inter-OFL.txt")
    (plain / "MANIFEST.json").write_text(json.dumps(manifest, indent=2) + "\n")
    runtime = args.binary.parent / "compat-runtime"
    if runtime.is_dir():
        shutil.copytree(runtime, plain / "compat-runtime")
    archive = args.output / f"chilpa-{args.version}-linux-x86_64.tar.gz"
    def normalize(info):
        info.uid = info.gid = 0
        info.uname = info.gname = "root"
        info.mtime = epoch
        return info
    with archive.open("wb") as destination:
        with gzip.GzipFile(filename="", fileobj=destination, mode="wb", mtime=epoch) as compressed:
            with tarfile.open(fileobj=compressed, mode="w") as output:
                output.add(plain, arcname=plain.name, filter=normalize)
    appdir = work / "AppDir"
    bin_dir = appdir / "usr/bin"
    bin_dir.mkdir(parents=True)
    shutil.copy2(args.binary, bin_dir / "chilpa-qt")
    if runtime.is_dir():
        shutil.copytree(runtime, bin_dir / "compat-runtime")
    desktop = work / "app.chilpa.Chilpa.desktop"
    desktop.write_text((root / "packaging/linux/app.chilpa.Chilpa.desktop").read_text().replace("Exec=/usr/bin/chilpa-qt", "Exec=chilpa-qt"))
    icon = work / "app.chilpa.Chilpa.png"
    shutil.copy2(root / "assets/logo.png", icon)
    subprocess.run(["desktop-file-validate", str(desktop)], check=True)
    qml = work / "qml-sources"
    qml.mkdir()
    for path in sorted((root / "crates").glob("chilpa-qt*/qml")):
        shutil.copytree(path, qml / path.parent.name)
    qmake = os.environ.get("QMAKE") or shutil.which("qmake6") or shutil.which("qmake")
    if not qmake:
        raise SystemExit("Qt's qmake is required to package the runtime.")
    plugins = Path(subprocess.check_output([qmake, "-query", "QT_INSTALL_PLUGINS"], text=True).strip())
    platforms = [name for name in ("libqoffscreen.so", "libqwayland.so", "libqwayland-generic.so", "libqwayland-egl.so")
                 if (plugins / "platforms" / name).is_file()]
    environment = dict(os.environ, NO_STRIP="1", QMAKE=qmake, QML_SOURCES_PATHS=str(qml), ARCH="x86_64",
                       EXTRA_PLATFORM_PLUGINS=";".join(platforms))
    environment["PATH"] = str(args.tools) + os.pathsep + environment["PATH"]
    subprocess.run([str(args.tools / locked["linuxdeploy"]["asset"]), "--appimage-extract-and-run", "--appdir", str(appdir),
                    "-e", str(bin_dir / "chilpa-qt"), "-d", str(desktop), "-i", str(icon), "--plugin", "qt"],
                   cwd=work, env=environment, check=True)
    updater = bin_dir / "chilpa-appimage-updater.AppImage"
    shutil.copy2(args.tools / locked["updater"]["asset"], updater)
    shutil.copy2(root / "packaging/linux/chilpa-restart", bin_dir / "chilpa-restart")
    release_data = appdir / "usr/share/chilpa"
    release_data.mkdir(parents=True)
    (release_data / "release.json").write_text(json.dumps({"repository":args.repository,"version":args.version}) + "\n")
    licenses = appdir / "usr/share/licenses/chilpa"
    licenses.mkdir(parents=True)
    shutil.copy2(root / "LICENSE", licenses / "LICENSE")
    shutil.copy2(root / "vendor/chilpa-plugin-api/LICENSE", licenses / "plugin-protocol-MIT.txt")
    for name in ("Inter-OFL.txt", "Qt-LGPL-3.0.txt", "Qt-GPL-3.0.txt"):
        shutil.copy2(root / "packaging/linux" / name, licenses / name)
    (licenses / "Qt-source.txt").write_text("Qt runtime source and notices: https://code.qt.io/\nQt version: " + subprocess.check_output([qmake, "-query", "QT_VERSION"], text=True))
    (licenses / "AppImageUpdate.txt").write_text("I bundle AppImageUpdate unmodified under its MIT license.\nSource: https://github.com/AppImage/AppImageUpdate\n" + (root / "packaging/linux/AppImageUpdate-LICENSE.txt").read_text())
    qt_root = Path(subprocess.check_output([qmake, "-query", "QT_INSTALL_PREFIX"], text=True).strip())
    qt_licenses = qt_root / "Licenses"
    if qt_licenses.is_dir():
        shutil.copytree(qt_licenses, licenses / "Qt")
    for required in ("libqoffscreen.so",):
        if not list(appdir.rglob(required)):
            raise SystemExit(f"Missing Qt platform plugin: {required}")
    if not list(appdir.rglob("libqwayland*.so")):
        raise SystemExit("The Wayland platform plugin is missing.")
    if not (bin_dir / "qt.conf").exists() or not list(appdir.rglob("libeffectsplugin.so")):
        raise SystemExit("The deployed Qt Quick runtime is incomplete.")
    owner, repository = args.repository.split("/")
    information = f"gh-releases-zsync|{owner}|{repository}|latest|Chilpa-*-x86_64.AppImage.zsync"
    image = args.output / f"Chilpa-{args.version}-x86_64.AppImage"
    subprocess.run([str(args.tools / locked["appimagetool"]["asset"]), "--appimage-extract-and-run", "--no-appstream",
                    "--runtime-file", str(args.tools / locked["runtime"]["asset"]),
                    "--mksquashfs-opt", "-processors", "--mksquashfs-opt", "2",
                    "--updateinformation", information, str(appdir), str(image)], cwd=args.output, env=environment, check=True)
    if not Path(str(image) + ".zsync").is_file():
        raise SystemExit("No delta update metadata was generated.")
    image.chmod(0o755)
    embedded = subprocess.check_output([str(image), "--appimage-updateinformation"], text=True).strip()
    if embedded != information:
        raise SystemExit("The AppImage update information does not match its release channel.")
    (args.output / "MANIFEST.json").write_text(json.dumps(manifest, indent=2) + "\n")
assets = [archive, image, Path(str(image) + ".zsync"), args.output / "MANIFEST.json"]
(args.output / "SHA256SUMS").write_text("".join(f"{hashlib.file_digest(path.open('rb'), 'sha256').hexdigest()}  {path.name}\n" for path in assets))
print(f"Packaged {archive.name} and {image.name}")
