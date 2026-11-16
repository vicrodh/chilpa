#!/usr/bin/env python3
"""Deploy and verify the audio library omitted by linuxdeploy's normal exclusions."""
import argparse
from pathlib import Path
import re
import subprocess
import tempfile

SONAME = "libpipewire-0.3.so.0"


def pipewire_runtime(binary):
    libraries = subprocess.check_output(["ldd", str(binary)], text=True)
    for line in libraries.splitlines():
        match = re.match(r"\s*libpipewire-0\.3\.so\.0\s+=>\s+(.+?)\s+\(0x[0-9a-f]+\)", line)
        if match:
            path = Path(match.group(1))
            path.resolve(strict=True)
            return path
    raise SystemExit("The executable's required PipeWire client library could not be resolved.")


def check_appdir_audio(appdir):
    appdir = Path(appdir).resolve()
    library = appdir / "usr/lib" / SONAME
    if not library.is_file() or not library.resolve().is_relative_to(appdir):
        raise SystemExit("The AppImage is missing its bundled PipeWire client library.")
    with library.open("rb") as stream:
        if stream.read(4) != b"\x7fELF":
            raise SystemExit("The bundled PipeWire client library is not an ELF library.")


def check_appimage_audio(image):
    with tempfile.TemporaryDirectory(prefix="chilpa-appimage-runtime-") as directory:
        subprocess.run([str(Path(image).resolve()), "--appimage-extract"], cwd=directory,
                       check=True, stdout=subprocess.DEVNULL)
        check_appdir_audio(Path(directory) / "squashfs-root")
    print("The extracted AppImage contains its required PipeWire client library.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("image", type=Path)
    check_appimage_audio(parser.parse_args().image)
