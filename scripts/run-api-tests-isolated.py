#!/usr/bin/env python3
"""Run an explicitly supplied test command in a disposable profile and session bus."""
import argparse
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def profile_environment(root: Path) -> dict[str, str]:
    root = root.resolve(strict=True)
    root.chmod(0o700)
    marker = root / ".chilpa-api-test-profile"
    marker.write_bytes(b"chilpa-api-unit-tests-v1\n")
    marker.chmod(0o600)
    environment = os.environ.copy()
    environment["CHILPA_TEST_PROFILE_ROOT"] = str(root)
    environment["RUST_TEST_THREADS"] = "1"
    for kind in ("config", "data", "cache", "state", "runtime"):
        folder = root / kind
        folder.mkdir(mode=0o700)
        key = "XDG_RUNTIME_DIR" if kind == "runtime" else f"XDG_{kind.upper()}_HOME"
        environment[key] = str(folder)
    return environment


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", nargs=argparse.REMAINDER,
                        help="Exact authorized command, after --; include the global cargo lock.")
    command = parser.parse_args().command
    if command[:1] == ["--"]:
        command = command[1:]
    if not command:
        parser.error("supply the exact authorized test command after --")
    # The context owns the profile until the command and its session bus exit.
    with tempfile.TemporaryDirectory(prefix="chilpa-api-tests-") as directory:
        environment = profile_environment(Path(directory))
        if sys.platform.startswith("linux"):
            command = ["dbus-run-session", "--", *command]
        return subprocess.run(command, env=environment, check=False).returncode


if __name__ == "__main__":
    raise SystemExit(main())
