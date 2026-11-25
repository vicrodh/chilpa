#!/usr/bin/env python3
"""Deploy the audio client libraries so that the system's own are used wherever it has them.

The system's ALSA plugins for PipeWire and PulseAudio load into Chilpa's
process and bind to whichever `libpipewire` or `libpulse` is already there.
A copy older than the plugins breaks them: on Ubuntu 26.04 the PipeWire
plugin would not load beside a bundled PipeWire 1.0.5 (`undefined symbol:
pw_log_topic_register`), and the system's default output gave no sound. So
native packages ship none and depend on the distribution's, and the AppImage
keeps its copies apart and adds them only on a system that has none.
"""
import argparse
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

SONAME = "libpipewire-0.3.so.0"
# Each group's own libraries, and the one a system that has the group shows.
GROUPS = {
    "pipewire": (("libpipewire-0.3.so.0",), "libpipewire-0.3.so.0"),
    "pulse": (("libpulse.so.0", "libpulse-simple.so.0"), "libpulse.so.0"),
}
FALLBACK = Path("usr/lib/host-fallback")
# How `ldconfig -p` names this architecture's libraries.
LDCONFIG_ABI = {"x86_64": "libc6,x86-64", "aarch64": "libc6,AArch64"}
WRAPPED = "AppRun.linuxdeploy"


def pipewire_runtime(binary):
    libraries = subprocess.check_output(["ldd", str(binary)], text=True)
    for line in libraries.splitlines():
        match = re.match(r"\s*libpipewire-0\.3\.so\.0\s+=>\s+(.+?)\s+\(0x[0-9a-f]+\)", line)
        if match:
            path = Path(match.group(1))
            path.resolve(strict=True)
            return path
    raise SystemExit("The executable's required PipeWire client library could not be resolved.")


def is_elf(path):
    try:
        with path.open("rb") as stream:
            return stream.read(4) == b"\x7fELF"
    except OSError:
        return False


def elf_needed(path):
    """The libraries an ELF file names as needed."""
    dynamic = subprocess.check_output(["readelf", "-d", str(path)], text=True, stderr=subprocess.DEVNULL)
    return re.findall(r"\(NEEDED\)\s+Shared library: \[([^\]]+)\]", dynamic)


def host_audio_groups(usr, needed=elf_needed):
    """The libraries under `usr/lib` that each audio client group alone needs.

    A library also reached from anything else in the tree stays where it is:
    the D-Bus library the PulseAudio client uses is Qt's too.
    """
    usr = Path(usr)
    lib = usr / "lib"
    present = {path.name for path in lib.iterdir() if path.is_file() and not path.is_symlink()}
    graph = {name: [n for n in needed(lib / name) if n in present] for name in present}

    def reached(roots, around=frozenset()):
        seen, todo = set(), [r for r in roots if r in present and r not in around]
        while todo:
            name = todo.pop()
            if name not in seen:
                seen.add(name)
                todo.extend(n for n in graph.get(name, []) if n not in around)
        return seen

    clients = frozenset(name for roots, _ in GROUPS.values() for name in roots)
    audio = reached(clients)
    # What the rest of the tree needs, without passing through the clients:
    # that the executable needs `libpulse` is why the system's must be found.
    others = set()
    for path in usr.rglob("*"):
        if not path.is_file() or path.is_symlink() or not is_elf(path):
            continue
        if path.parent == lib and path.name in audio:
            continue
        others |= reached(needed(path), clients)
    return {group: reached(roots) - others for group, (roots, _) in GROUPS.items()}


def strip_host_audio(usr, needed=elf_needed):
    """Removes the audio client libraries from a native package's tree."""
    lib = Path(usr) / "lib"
    removed = set().union(*host_audio_groups(usr, needed).values())
    for name in removed:
        (lib / name).unlink()
    for link in lib.iterdir():
        if link.is_symlink() and not link.exists():
            link.unlink()
    return sorted(removed)


def apprun(abi):
    lines = [
        "#!/bin/sh",
        "# The PipeWire and PulseAudio client libraries come from the system when it",
        "# has them: its audio plugins load into Chilpa and must find their own.",
        "# Chilpa's copies are added only where the system has none.",
        'here="$(dirname "$(readlink -f "$0")")"',
        'known="$( (ldconfig -p || /sbin/ldconfig -p || /usr/sbin/ldconfig -p) 2>/dev/null)"',
    ]
    for group, (_, soname) in GROUPS.items():
        lines += [
            f'case "$known" in',
            f'    *"{soname} ({abi}"*) ;;',
            f'    *) [ -d "$here/{FALLBACK}/{group}" ] && LD_LIBRARY_PATH="$here/{FALLBACK}/{group}${{LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}}" ;;',
            "esac",
        ]
    lines += [
        '[ -n "${LD_LIBRARY_PATH:-}" ] && export LD_LIBRARY_PATH',
        f'exec "$here/{WRAPPED}" "$@"',
    ]
    return "\n".join(lines) + "\n"


def defer_host_audio(appdir, arch, needed=elf_needed):
    """Moves the AppImage's audio client libraries apart and starts it through a wrapper that adds them only when needed."""
    appdir = Path(appdir)
    lib = appdir / "usr/lib"
    groups = host_audio_groups(appdir / "usr", needed)
    for group, names in groups.items():
        target = appdir / FALLBACK / group
        target.mkdir(parents=True, exist_ok=True)
        for name in names:
            shutil.copy2(lib / name, target / name)
    for name in set().union(*groups.values()):
        (lib / name).unlink()
    launcher = appdir / "AppRun"
    launcher.rename(appdir / WRAPPED)
    launcher.write_text(apprun(LDCONFIG_ABI[arch]))
    launcher.chmod(0o755)
    return groups


def check_appdir_audio(appdir):
    appdir = Path(appdir).resolve()
    library = appdir / FALLBACK / "pipewire" / SONAME
    if not library.is_file() or not library.resolve().is_relative_to(appdir):
        raise SystemExit("The AppImage is missing its fallback PipeWire client library.")
    if not is_elf(library):
        raise SystemExit("The fallback PipeWire client library is not an ELF library.")
    if (appdir / "usr/lib" / SONAME).exists():
        raise SystemExit("The AppImage loads its own PipeWire client library before the system's.")
    launcher = appdir / "AppRun"
    if str(FALLBACK) not in launcher.read_text() or not (appdir / WRAPPED).exists():
        raise SystemExit("The AppImage does not start through its audio library wrapper.")


def check_native_audio(payload):
    """A native package's tree carries no audio client library of its own."""
    lib = Path(payload) / "opt/chilpa/lib"
    shipped = [name for roots, _ in GROUPS.values() for name in roots if (lib / name).exists()]
    if shipped:
        raise SystemExit("The native package ships its own audio client libraries: " + ", ".join(shipped))


def check_appimage_audio(image):
    with tempfile.TemporaryDirectory(prefix="chilpa-appimage-runtime-") as directory:
        subprocess.run([str(Path(image).resolve()), "--appimage-extract"], cwd=directory,
                       check=True, stdout=subprocess.DEVNULL)
        check_appdir_audio(Path(directory) / "squashfs-root")
    print("The extracted AppImage keeps its PipeWire client library as a fallback only.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("image", type=Path)
    check_appimage_audio(parser.parse_args().image)
