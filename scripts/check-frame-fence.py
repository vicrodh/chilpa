#!/usr/bin/env python3
"""I verify the frame fence against an old frame that is still rendering."""
import argparse
import importlib.util
import os
from pathlib import Path
import shlex
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--backend", choices=("opengl", "vulkan"), default="opengl")
    arguments = parser.parse_args()
    scripts = Path(__file__).resolve().parent
    root = scripts.parent
    output = arguments.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    specification = importlib.util.spec_from_file_location("immersive_smoke", scripts / "check-immersive-scenes.py")
    display_tools = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(display_tools)
    native = root / "crates/chilpa-qt-kit/cxx"
    libexec = Path(subprocess.check_output(["pkg-config", "--variable=libexecdir", "Qt6Core"], text=True).strip())
    flags = shlex.split(subprocess.check_output(["pkg-config", "--cflags", "--libs",
        "Qt6Quick", "Qt6Qml", "Qt6Gui", "Qt6Core"], text=True))
    with tempfile.TemporaryDirectory(prefix="chilpa-frame-fence-") as directory:
        temporary = Path(directory)
        generated = temporary / "moc_frame_fence.cpp"
        binary = temporary / "frame-fence-race"
        with (output / "build.log").open("w") as log:
            subprocess.run([str(libexec / "moc"), str(native / "frame_fence.h"), "-o", str(generated)],
                check=True, stdout=log, stderr=subprocess.STDOUT, timeout=60)
            subprocess.run(["c++", "-std=c++17", "-O0", "-fPIC", "-I" + str(native),
                str(root / "crates/chilpa-qt-kit/tests/frame_fence_race.cpp"), str(native / "frame_fence.cpp"),
                str(generated), "-o", str(binary), *flags],
                check=True, stdout=log, stderr=subprocess.STDOUT, timeout=180)
        runtime = temporary / "runtime"
        runtime.mkdir(mode=0o700)
        environment = dict(os.environ, QT_QPA_PLATFORM="xcb", QSG_RENDER_LOOP="threaded",
            QSG_RHI_BACKEND=arguments.backend, XDG_RUNTIME_DIR=str(runtime), QT_NO_XDG_DESKTOP_PORTAL="1")
        log_path = output / "frame-fence-race.log"
        with display_tools.isolated_display(output) as display, log_path.open("w") as log:
            environment["DISPLAY"] = display
            result = subprocess.run([str(binary)], env=environment, stdout=log,
                stderr=subprocess.STDOUT, timeout=20)
        transcript = log_path.read_text()
        print(transcript, end="")
        if result.returncode or "FAIL" in transcript or "PASS one preparation for the ACK token" not in transcript:
            raise SystemExit(1)


if __name__ == "__main__":
    main()
