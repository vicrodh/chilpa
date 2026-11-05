"""A private PCM graph with one null sink and no hardware discovery."""

from contextlib import contextmanager
import json
import os
from pathlib import Path
import selectors
import subprocess
import time
from xml.sax.saxutils import escape


_NAME = "chilpa-viz-null"
_SERVER = """
context.properties = {
    core.name = chilpa-viz-test
    core.daemon = true
    support.dbus = false
    default.clock.rate = 48000
    default.clock.allowed-rates = [ 44100 48000 96000 192000 ]
    default.clock.quantum = 256
}
context.spa-libs = {
    audio.convert.* = audioconvert/libspa-audioconvert
    support.* = support/libspa-support
}
context.modules = [
    { name = libpipewire-module-protocol-native }
    { name = libpipewire-module-metadata }
    { name = libpipewire-module-spa-node-factory }
    { name = libpipewire-module-client-node }
    { name = libpipewire-module-adapter }
    { name = libpipewire-module-link-factory }
    { name = libpipewire-module-session-manager }
    { name = libpipewire-module-access args = { access.force = unrestricted } }
]
context.objects = [
    { factory = spa-node-factory args = {
        factory.name = support.node.driver
        node.name = Visualizer-Test-Driver
        priority.driver = 20000
    } }
    { factory = adapter args = {
        factory.name = support.null-audio-sink
        node.name = chilpa-viz-null
        node.description = "Private visualizer null sink"
        media.class = Audio/Sink
        audio.position = [ FL FR ]
    } }
]
"""
_POLICY = """
wireplumber.profiles = { main = {
    monitor.alsa = disabled
    monitor.alsa-midi = disabled
    monitor.bluez = disabled
    monitor.bluez-midi = disabled
    monitor.v4l2 = disabled
    monitor.libcamera = disabled
} }
"""
_PULSE = """
context.properties = { support.dbus = false }
context.spa-libs = {
    audio.convert.* = audioconvert/libspa-audioconvert
    support.* = support/libspa-support
}
context.modules = [
    { name = libpipewire-module-protocol-native }
    { name = libpipewire-module-client-node }
    { name = libpipewire-module-adapter }
    { name = libpipewire-module-metadata }
    { name = libpipewire-module-protocol-pulse args = {
        server.address = [ "unix:SOCKET" ]
    } }
]
"""
_ALSA = """
pcm.pipewire {
    @args [ NODE EXCLUSIVE ]
    @args.NODE { type string default "chilpa-viz-null" }
    @args.EXCLUSIVE { type integer default 0 }
    type pipewire
    server "chilpa-viz-test"
    playback_node $NODE
    capture_node $NODE
    exclusive $EXCLUSIVE
}
pcm.default {
    type pipewire
    server "chilpa-viz-test"
    playback_node "chilpa-viz-null"
}
"""


def _clean_environment():
    return {key: value for key, value in os.environ.items()
            if not key.startswith(("PIPEWIRE_", "PULSE_", "WIREPLUMBER_"))}


@contextmanager
def private_pipewire(profile: Path):
    """Yield device/environment for a bounded graph; stop only owned PIDs.

    The caller owns ``profile`` and its eventual removal. Logs remain there.
    ``environment`` is a complete cleaned environment; apply the application's
    isolated XDG profile afterward. The daemons use a separate private bus,
    configuration and runtime. Neither ALSA nor Bluetooth factories are loaded.
    """
    root = (Path(profile) / "private-pipewire").resolve()
    if len(os.fsencode(root / "runtime" / "chilpa-viz-test")) >= 108:
        raise ValueError("Private audio requires a shorter profile path for Unix sockets")
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    runtime = root / "runtime"
    runtime.mkdir(mode=0o700)
    bus_config = root / "dbus.conf"
    bus_config.write_text("<busconfig><type>session</type><auth>EXTERNAL</auth>"
                          f"<listen>unix:path={escape(str(runtime / 'bus'))}</listen>"
                          '<policy context="default"><allow send_destination="*"/>'
                          '<allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
    config = root / "config"
    policy = config / "wireplumber" / "wireplumber.conf.d"
    policy.mkdir(parents=True)
    (policy / "isolated.conf").write_text(_POLICY)
    server_config = root / "pipewire.conf"
    server_config.write_text(_SERVER)
    pulse_socket = runtime / "pulse-native"
    pulse_config = root / "pipewire-pulse.conf"
    pulse_config.write_text(_PULSE.replace("SOCKET", str(pulse_socket)))
    alsa_config = root / "asound.conf"
    alsa_config.write_text(_ALSA)
    environment = _clean_environment()
    environment.update(PIPEWIRE_RUNTIME_DIR=str(runtime),
                       PIPEWIRE_REMOTE="chilpa-viz-test",
                       PULSE_SERVER=f"unix:{pulse_socket}",
                       PULSE_RUNTIME_PATH=str(runtime),
                       ALSA_CONFIG_PATH=str(alsa_config))
    daemon_environment = dict(environment, XDG_RUNTIME_DIR=str(runtime),
                              XDG_CONFIG_HOME=str(config),
                              XDG_STATE_HOME=str(root / "state"),
                              XDG_CACHE_HOME=str(root / "cache"),
                              GIO_USE_VFS="local")
    daemon_environment.pop("DBUS_SESSION_BUS_ADDRESS", None)
    processes = []
    logs = []

    def launch(name, command, *, address=False):
        log = (root / f"{name}.log").open("wb")
        logs.append(log)
        process = subprocess.Popen(command, env=daemon_environment,
                                   stdin=subprocess.DEVNULL,
                                   stdout=subprocess.PIPE if address else log,
                                   stderr=log)
        processes.append(process)
        return process

    def wait_for(check):
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            if any(process.poll() is not None for process in processes):
                raise RuntimeError(f"Private audio service exited; see {root}")
            if check():
                return
            time.sleep(0.05)
        raise RuntimeError(f"Private audio service did not become ready; see {root}")

    def null_sink_ready():
        result = subprocess.run(["pactl", "-f", "json", "list", "sinks"],
                                env=environment, capture_output=True, timeout=2)
        if result.returncode:
            return False
        sinks = json.loads(result.stdout)
        if not sinks:
            return False
        if len(sinks) != 1 or sinks[0].get("name") != _NAME:
            raise RuntimeError("The private audio graph has an unexpected sink")
        return True

    try:
        bus = launch("dbus", ["dbus-daemon", f"--config-file={bus_config}",
                              "--nofork", "--print-address=1"], address=True)
        with selectors.DefaultSelector() as selector:
            selector.register(bus.stdout, selectors.EVENT_READ)
            if not selector.select(timeout=5):
                raise RuntimeError("The private audio bus did not become ready")
            address = bus.stdout.readline().decode().strip()
        if not address.startswith("unix:"):
            raise RuntimeError("The private audio bus returned no Unix address")
        daemon_environment["DBUS_SESSION_BUS_ADDRESS"] = address
        launch("pipewire", ["pipewire", "-c", str(server_config)])
        wait_for(lambda: (runtime / "chilpa-viz-test").is_socket())
        launch("wireplumber", ["wireplumber"])
        launch("pulse", ["pipewire-pulse", "-c", str(pulse_config)])
        wait_for(null_sink_ready)
        devices = subprocess.run(["pw-cli", "ls", "Device"], env=environment,
                                 capture_output=True, timeout=2, check=True)
        if devices.stdout.strip():
            raise RuntimeError("The private audio graph has an unexpected device")
        yield {"device": f"pipewire:{_NAME}", "environment": environment}
    finally:
        for process in reversed(processes):
            if process.poll() is None:
                process.terminate()
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=3)
            if process.stdout is not None:
                process.stdout.close()
        for log in logs:
            log.close()
