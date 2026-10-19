"""I keep architecture names and the published asset contract in one place."""
import platform


def architecture(value=None):
    value = value or platform.machine()
    aliases = {"x86_64": "x86_64", "amd64": "x86_64", "arm64": "aarch64", "aarch64": "aarch64"}
    if value not in aliases:
        raise ValueError(f"Unsupported release architecture: {value}")
    return aliases[value]


def linux_assets(version, arch):
    arch = architecture(arch)
    deb = "amd64" if arch == "x86_64" else "arm64"
    return [f"chilpa-{version}-linux-{arch}.tar.gz", f"chilpa_{version}_{deb}.deb",
            f"chilpa-{version}-1.{arch}.rpm", f"Chilpa-{version}-{arch}.AppImage",
            f"Chilpa-{version}-{arch}.AppImage.zsync", f"MANIFEST-linux-{arch}.json"]


def macos_assets(version, arch):
    arch = architecture(arch)
    return [f"Chilpa-{version}-macos-{arch}.zip", f"Chilpa-{version}-macos-{arch}.dmg",
            f"MANIFEST-macos-{arch}.json"]
