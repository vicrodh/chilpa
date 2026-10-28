#!/usr/bin/env python3
"""I deploy a self-contained macOS app, ZIP and disk image with both icon appearances."""
import argparse
import hashlib
import json
import platform
import plistlib
import re
from pathlib import Path
import shutil
import subprocess
import tempfile

from release_version import release_version
from release_platform import architecture as release_architecture


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def iconset(source, destination):
    destination.mkdir()
    for size in (16, 32, 128, 256, 512):
        for scale in (1, 2):
            name = f"icon_{size}x{size}" + ("@2x" if scale == 2 else "") + ".png"
            pixels = str(size * scale)
            subprocess.run(["sips", "-z", pixels, pixels, str(source), "--out", str(destination / name)],
                check=True, stdout=subprocess.DEVNULL)


def deployment_version(value):
    return tuple(int(part) for part in value.split("."))


def deployment_minimum(load_commands):
    versions = []
    for command in re.split(r"Load command \d+\n", load_commands):
        if re.search(r"^\s*cmd LC_BUILD_VERSION\s*$", command, re.MULTILINE):
            field = "minos"
        elif re.search(r"^\s*cmd LC_VERSION_MIN_MACOSX\s*$", command, re.MULTILINE):
            field = "version"
        else:
            continue
        match = re.search(r"^\s*" + field + r"\s+(\d+(?:\.\d+)*)\s*$", command, re.MULTILINE)
        if match:
            versions.append(match.group(1))
    return max(versions, key=deployment_version) if versions else None


def check_dependencies(app):
    checked = 0
    minimum = "0.0"
    frameworks = app / "Contents/Frameworks"
    for path in app.rglob("*"):
        if path.is_symlink() or not path.is_file():
            continue
        with path.open("rb") as stream:
            magic = stream.read(4)
        if magic not in (b"\xcf\xfa\xed\xfe", b"\xce\xfa\xed\xfe", b"\xca\xfe\xba\xbe", b"\xca\xfe\xba\xbf"):
            continue
        checked += 1
        target = deployment_minimum(subprocess.check_output(["otool", "-l", str(path)], text=True))
        if target:
            minimum = max((minimum, target), key=deployment_version)
        # I relocate indirect libraries whose own install names macdeployqt leaves behind.
        if path.is_relative_to(frameworks):
            identifiers = subprocess.check_output(["otool", "-D", str(path)], text=True).splitlines()
            if any(line.startswith("/") and not line.endswith(":") for line in identifiers):
                subprocess.run(["install_name_tool", "-id", "@rpath/" + str(path.relative_to(frameworks)), str(path)], check=True)
        dependencies = subprocess.check_output(["otool", "-L", str(path)], text=True).splitlines()
        for line in dependencies:
            # A universal binary heads each architecture's list with its own path.
            if not line.startswith("\t"):
                continue
            dependency = line.strip().split(" (", 1)[0]
            if dependency.startswith("/") and not dependency.startswith(("/System/Library/", "/usr/lib/")):
                raise SystemExit(f"I found an undeployed dependency in {path.name}: {dependency}")
    return checked, minimum


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--arch", choices=("x86_64", "aarch64"), help="Expected binary architecture.")
    parser.add_argument("--minimum-macos", default="14.0", help="Requested floor; deployed libraries can require a newer macOS.")
    parser.add_argument("--require-native-icon", action="store_true", help="Require Xcode's asset compiler for system-selected icon appearances.")
    args = parser.parse_args()
    if platform.system() != "Darwin":
        parser.error("I package macOS bundles on macOS.")
    try:
        release_version(args.version)
    except ValueError as error:
        parser.error(str(error))
    root = Path(__file__).resolve().parents[1]
    binary = args.binary.resolve(strict=True)
    architecture = subprocess.check_output(["lipo", "-archs", str(binary)], text=True).strip()
    if architecture not in ("arm64", "x86_64"):
        parser.error("I require a single arm64 or x86_64 macOS executable.")
    arch = release_architecture(architecture)
    if args.arch and args.arch != arch:
        parser.error("The executable architecture differs from the requested package.")
    if not shutil.which("macdeployqt"):
        parser.error("I require Qt's macdeployqt on PATH.")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    app = output / "Chilpa.app"
    if app.exists():
        parser.error("I require an output directory without an existing Chilpa.app.")
    native = subprocess.run(["xcrun", "--find", "actool"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0
    if args.require_native_icon and not native:
        parser.error("I require full Xcode to compile the native light/dark icon catalog.")
    contents = app / "Contents"
    executable = contents / "MacOS/chilpa-qt"
    resources = contents / "Resources"
    executable.parent.mkdir(parents=True)
    resources.mkdir()
    shutil.copy2(binary, executable)
    executable.chmod(0o755)
    numeric_version = args.version.split("-", 1)[0]
    info = dict(CFBundleIdentifier="app.chilpa.Chilpa", CFBundleName="Chilpa", CFBundleDisplayName="Chilpa",
        CFBundleExecutable="chilpa-qt", CFBundlePackageType="APPL", CFBundleShortVersionString=numeric_version,
        CFBundleVersion=numeric_version, ChilpaReleaseVersion=args.version, CFBundleIconFile="AppIcon.icns",
        LSMinimumSystemVersion=args.minimum_macos, LSApplicationCategoryType="public.app-category.music",
        NSHighResolutionCapable=True, NSPrincipalClass="NSApplication")
    with tempfile.TemporaryDirectory(prefix="chilpa-macos-") as directory:
        work = Path(directory)
        for appearance in ("light", "dark"):
            # The .icns is made from the padded master (tools/icons/macos-icon): art that fills the canvas draws bigger than every other icon.
            source = root / f"packaging/macos/icns-{appearance}.png"
            assert digest(root / f"crates/chilpa-qt-kit/assets/app-icon-{appearance}.png") == digest(root / f"packaging/macos/AppIcon.icon/Assets/{appearance}.png")
            icons = work / f"{appearance}.iconset"
            iconset(source, icons)
            name = "AppIcon.icns" if appearance == "light" else "AppIcon-dark.icns"
            subprocess.run(["iconutil", "--convert", "icns", "--output", str(resources / name), str(icons)], check=True)
        if native:
            partial = work / "icon.plist"
            subprocess.run(["xcrun", "actool", str(root / "packaging/macos/AppIcon.icon"), "--compile", str(resources),
                "--platform", "macosx", "--target-device", "mac", "--minimum-deployment-target", args.minimum_macos,
                "--app-icon", "AppIcon", "--output-partial-info-plist", str(partial)], check=True)
            if not (resources / "Assets.car").is_file():
                raise SystemExit("The icon compiler did not produce Assets.car.")
            info.update(plistlib.loads(partial.read_bytes()))
            info["CFBundleIconName"] = "AppIcon"
        else:
            print("I use the ICNS fallback; the running app follows the system's light/dark appearance.")
        (contents / "Info.plist").write_bytes(plistlib.dumps(info))
        qml = work / "qml"
        qml.mkdir()
        for source in sorted((root / "crates").glob("chilpa-qt*/qml")):
            shutil.copytree(source, qml / source.parent.name)
        plugins = Path(subprocess.check_output(["qmake", "-query", "QT_INSTALL_PLUGINS"], text=True).strip())
        offscreen = contents / "PlugIns/platforms/libqoffscreen.dylib"
        offscreen.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(plugins / "platforms/libqoffscreen.dylib", offscreen)
        subprocess.run(["macdeployqt", str(app), "-qmldir=" + str(qml), "-executable=" + str(offscreen),
            "-no-strip", "-no-codesign"], check=True)
        # I keep SQLite for QML LocalStorage; the other SQL drivers need client libraries I do not ship.
        for driver in (contents / "PlugIns/sqldrivers").glob("*.dylib"):
            if driver.name != "libqsqlite.dylib":
                driver.unlink()
    if not (contents / "PlugIns/platforms/libqcocoa.dylib").is_file():
        raise SystemExit("I could not deploy Qt's Cocoa platform plugin.")
    if not list((contents / "Resources/qml").rglob("*effectsplugin*")):
        raise SystemExit("I could not deploy Qt Quick Effects.")
    licenses = resources / "licenses"
    shutil.copytree(root / "licenses", licenses)
    shutil.copy2(root / "LICENSE", licenses / "LICENSE")
    shutil.copy2(root / "vendor/chilpa-plugin-api/LICENSE", licenses / "plugin-protocol-MIT.txt")
    for name in ("Inter-OFL.txt", "Qt-LGPL-3.0.txt", "Qt-GPL-3.0.txt"):
        shutil.copy2(root / "packaging/linux" / name, licenses / name)
    qt_version = subprocess.check_output(["qmake", "-query", "QT_VERSION"], text=True).strip()
    (licenses / "Qt-source.txt").write_text(f"Qt runtime source and notices: https://code.qt.io/\nQt version: {qt_version}\n")
    dependencies, runtime_minimum = check_dependencies(app)
    minimum = max((args.minimum_macos, runtime_minimum), key=deployment_version)
    info["LSMinimumSystemVersion"] = minimum
    (contents / "Info.plist").write_bytes(plistlib.dumps(info))
    if minimum != args.minimum_macos:
        print(f"The deployed runtime requires macOS {minimum}; I record that minimum in the bundle.")
    subprocess.run(["codesign", "--force", "--deep", "--sign", "-", str(app)], check=True)
    subprocess.run(["codesign", "--verify", "--deep", "--strict", str(app)], check=True)
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
    manifest = dict(version=args.version, source_commit=commit, platform=f"macos-{arch}", minimum_macos=minimum, requested_minimum_macos=args.minimum_macos,
        source_binary_sha256=digest(binary), packaged_binary_sha256=digest(executable), qt_version=qt_version,
        native_icon_catalog=native, icon_appearances=["light", "dark"], signing="ad-hoc", notarized=False,
        checked_macho_dependencies=dependencies)
    manifest_path = output / f"MANIFEST-macos-{arch}.json"
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
    archive = output / f"Chilpa-{args.version}-macos-{arch}.zip"
    subprocess.run(["ditto", "-c", "-k", "--sequesterRsrc", "--keepParent", str(app), str(archive)], check=True)
    image = output / f"Chilpa-{args.version}-macos-{arch}.dmg"
    with tempfile.TemporaryDirectory(prefix="chilpa-dmg-") as directory:
        payload = Path(directory)
        shutil.copytree(app, payload / app.name, symlinks=True)
        (payload / "Applications").symlink_to("/Applications")
        subprocess.run(["hdiutil", "create", "-volname", "Chilpa", "-srcfolder", str(payload), "-ov", "-format", "UDZO", str(image)], check=True)
    (output / f"SHA256SUMS-macos-{arch}").write_text("".join(f"{digest(path)}  {path.name}\n" for path in (archive, image, manifest_path)))
    print(json.dumps(manifest, indent=2))
    print(f"I packaged {app}, {archive.name} and {image.name}.")


if __name__ == "__main__":
    main()
