"""I install the deployed Qt runtime under /opt/chilpa in DEB and RPM packages."""
import json
from pathlib import Path
import subprocess


def native_recipe(appdir, root, version, arch, glibc):
    return {
        "name": "chilpa", "arch": "amd64" if arch == "x86_64" else "arm64", "platform": "linux",
        "version": version, "release": "1", "section": "sound", "priority": "optional",
        "maintainer": "Victor Rodriguez <vicrodh@users.noreply.github.com>",
        "description": "Native music player with local libraries and extensible sources.",
        "homepage": "https://github.com/vicrodh/chilpa", "license": "MIT",
        "contents": [
            {"src": str(appdir / "usr"), "dst": "/opt/chilpa", "type": "tree"},
            {"src": "/opt/chilpa/bin/chilpa-qt", "dst": "/usr/bin/chilpa-qt", "type": "symlink"},
            {"src": str(root / "packaging/linux/app.chilpa.Chilpa.desktop"), "dst": "/usr/share/applications/app.chilpa.Chilpa.desktop"},
            {"src": str(root / "assets/logo.png"), "dst": "/usr/share/icons/hicolor/512x512/apps/app.chilpa.Chilpa.png"},
            {"src": str(root / "LICENSE"), "dst": "/usr/share/licenses/chilpa/LICENSE"},
        ],
        "overrides": {
            "deb": {"depends": [f"libc6 (>= {glibc})", "libstdc++6", "libgl1", "libegl1", "libfontconfig1",
                                "libfreetype6", "libdbus-1-3", "libxkbcommon0", "libxkbcommon-x11-0", "libxcb1",
                                "libxcb-cursor0", "libxcb-icccm4", "libxcb-image0", "libxcb-keysyms1",
                                "libxcb-render-util0", "libxcb-shape0", "libxcb-xkb1", "libx11-6", "zlib1g",
                                "libasound2 | libasound2t64", "libssl3 | libssl3t64",
                                "libwayland-client0", "libwayland-egl1", "libwayland-cursor0"], "recommends": ["bubblewrap"]},
            "rpm": {"depends": [f"glibc >= {glibc}", "libstdc++.so.6()(64bit)", "libGL.so.1()(64bit)",
                                "libEGL.so.1()(64bit)", "libfontconfig.so.1()(64bit)", "libfreetype.so.6()(64bit)",
                                "libdbus-1.so.3()(64bit)", "libxkbcommon.so.0()(64bit)", "libxcb.so.1()(64bit)",
                                "libX11.so.6()(64bit)", "libz.so.1()(64bit)", "libasound.so.2()(64bit)",
                                "libssl.so.3()(64bit)", "libxkbcommon-x11.so.0()(64bit)", "libxcb-cursor.so.0()(64bit)",
                                "libwayland-client.so.0()(64bit)", "libwayland-egl.so.1()(64bit)",
                                "libwayland-cursor.so.0()(64bit)"], "recommends": ["bubblewrap"]},
        },
        "deb": {"compression": "xz"}, "rpm": {"compression": "xz", "group": "Applications/Multimedia"},
    }


def package_native(appdir, root, nfpm, output, version, arch, glibc):
    # I reuse the same deployed libraries and qt.conf as the AppImage.
    config = appdir.parent / "nfpm.json"
    config.write_text(json.dumps(native_recipe(appdir, root, version, arch, glibc), indent=2) + "\n")
    deb_arch = "amd64" if arch == "x86_64" else "arm64"
    packages = [output / f"chilpa_{version}_{deb_arch}.deb", output / f"chilpa-{version}-1.{arch}.rpm"]
    for kind, path in zip(("deb", "rpm"), packages):
        subprocess.run([str(nfpm), "package", "--config", str(config), "--packager", kind, "--target", str(path)], check=True)
    return packages
