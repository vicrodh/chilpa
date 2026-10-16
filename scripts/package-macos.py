#!/usr/bin/env python3
"""I deploy a self-contained macOS app, ZIP and disk image with both icon appearances."""
import argparse
import hashlib
import json
import platform
import plistlib
from pathlib import Path
import shutil
import subprocess
import tempfile

from release_version import release_version


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


def check_dependencies(app):
    checked = 0
    frameworks = app / "Contents/Frameworks"
    for path in app.rglob("*"):
        if path.is_symlink() or not path.is_file():
            continue
        with path.open("rb") as stream:
            magic = stream.read(4)
        if magic not in (b"\xcf\xfa\xed\xfe", b"\xce\xfa\xed\xfe", b"\xca\xfe\xba\xbe", b"\xca\xfe\xba\xbf"):
            continue
        checked += 1
        # I relocate indirect libraries whose own install names macdeployqt leaves behind.
        if path.is_relative_to(frameworks):
            identifiers = subprocess.check_output(["otool", "-D", str(path)], text=True).splitlines()
            if any(line.startswith("/") and not line.endswith(":") for line in identifiers):
                subprocess.run(["install_name_tool", "-id", "@rpath/" + str(path.relative_to(frameworks)), str(path)], check=True)
        dependencies = subprocess.check_output(["otool", "-L", str(path)], text=True).splitlines()[1:]
        for line in dependencies:
            dependency = line.strip().split(" (", 1)[0]
            if dependency.startswith("/") and not dependency.startswith(("/System/Library/", "/usr/lib/")):
                raise SystemExit(f"I found an undeployed dependency in {path.name}: {dependency}")
    return checked


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--minimum-macos", default="14.0")
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
    if architecture != "arm64":
        parser.error("This macOS beta package targets Apple Silicon (arm64).")
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
            source = root / f"crates/chilpa-qt-kit/assets/app-icon-{appearance}.png"
            assert digest(source) == digest(root / f"packaging/macos/AppIcon.icon/Assets/{appearance}.png")
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
    dependencies = check_dependencies(app)
    subprocess.run(["codesign", "--force", "--deep", "--sign", "-", str(app)], check=True)
    subprocess.run(["codesign", "--verify", "--deep", "--strict", str(app)], check=True)
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
    manifest = dict(version=args.version, source_commit=commit, platform="macos-aarch64", minimum_macos=args.minimum_macos,
        source_binary_sha256=digest(binary), packaged_binary_sha256=digest(executable), qt_version=qt_version,
        native_icon_catalog=native, icon_appearances=["light", "dark"], signing="ad-hoc", notarized=False,
        checked_macho_dependencies=dependencies)
    manifest_path = output / "MANIFEST-macos-aarch64.json"
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
    archive = output / f"Chilpa-{args.version}-macos-aarch64.zip"
    subprocess.run(["ditto", "-c", "-k", "--sequesterRsrc", "--keepParent", str(app), str(archive)], check=True)
    image = output / f"Chilpa-{args.version}-macos-aarch64.dmg"
    with tempfile.TemporaryDirectory(prefix="chilpa-dmg-") as directory:
        payload = Path(directory)
        shutil.copytree(app, payload / app.name, symlinks=True)
        (payload / "Applications").symlink_to("/Applications")
        subprocess.run(["hdiutil", "create", "-volname", "Chilpa", "-srcfolder", str(payload), "-ov", "-format", "UDZO", str(image)], check=True)
    (output / "SHA256SUMS-macos-aarch64").write_text("".join(f"{digest(path)}  {path.name}\n" for path in (archive, image, manifest_path)))
    print(json.dumps(manifest, indent=2))
    print(f"I packaged {app}, {archive.name} and {image.name}.")


if __name__ == "__main__":
    main()
