#!/usr/bin/env python3
"""I follow decoded music through a paced capture output into the actual views."""
import argparse
from array import array
import importlib.util
import json
import math
import os
from pathlib import Path
import subprocess
import tempfile
import time
import wave
import random
import shutil
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import threading
from contextlib import ExitStack

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary", type=Path)
parser.add_argument("output", type=Path)
parser.add_argument("--backend", choices=("opengl", "vulkan"), default="opengl")
parser.add_argument("--host-display", action="store_true")
parser.add_argument("--dsp-package", type=Path)
parser.add_argument("--queues", action="store_true")
parser.add_argument("--headless-rhi", action="store_true")
parser.add_argument("--main", action="store_true", help="Check compact and immersive views in real Main without plugins.")
parser.add_argument("--restored", action="store_true", help="Start Main with the compact visualizer already enabled.")
parser.add_argument("--http", action="store_true", help="Read the synthetic fixture through a loopback HTTP server.")
parser.add_argument("--private-pipewire",action="store_true",help="Use an isolated PipeWire graph containing only a null sink.")
parser.add_argument("--volume",type=int,default=99,choices=range(101),metavar="0..100",help="Software volume for the isolated playback fixture.")
parser.add_argument("--format",choices=("wav","flac"),default="wav",help="Encoding of the generated PCM fixture.")
parser.add_argument("--muted",action="store_true",help="Mute the isolated player while checking source analysis.")
parser.add_argument("--minimum-visual-peak",type=float,default=0,help="Minimum peak in the selected compact stream for the generated signal.")
args = parser.parse_args()
if args.headless_rhi and args.backend != "opengl":
    parser.error("--headless-rhi requires the OpenGL backend")
scripts = Path(__file__).resolve().parent
output = args.output.resolve()
output.mkdir(parents=True, exist_ok=True)
spec = importlib.util.spec_from_file_location("scene_checks", scripts / "check-immersive-scenes.py")
scene_checks = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scene_checks)

with tempfile.TemporaryDirectory(prefix="chilpa-live-visualizer-") as directory, ExitStack() as cleanup:
    profile = Path(directory)
    for name in ("config/chilpa", "data/chilpa/plugins", "cache", "runtime", "tmp"):
        (profile / name).mkdir(parents=True)
    (profile / "runtime").chmod(0o700)
    audio_device="file:capture";audio_environment={}
    if args.private_pipewire:
        graph_spec=importlib.util.spec_from_file_location("private_pipewire",scripts/"fixtures/private_pipewire.py")
        graph_module=importlib.util.module_from_spec(graph_spec);graph_spec.loader.exec_module(graph_module)
        graph_profile=Path(cleanup.enter_context(tempfile.TemporaryDirectory(prefix="cviz-pw-",dir="/tmp")))
        def preserve_audio_logs():
            destination=output/"private-audio";destination.mkdir(exist_ok=True)
            for path in (graph_profile/"private-pipewire").glob("*.log"):shutil.copy2(path,destination/path.name)
        cleanup.callback(preserve_audio_logs)
        graph=cleanup.enter_context(graph_module.private_pipewire(graph_profile))
        audio_device=graph["device"]
        audio_environment={key:value for key,value in graph["environment"].items() if key.startswith(("PIPEWIRE_","PULSE_")) or key=="ALSA_CONFIG_PATH"}
    settings="output = "+audio_device+"\nlanguage = es\n"
    settings+="playback.volume = "+str(args.volume)+"\n"
    settings+="playback.muted = "+str(args.muted).lower()+"\n"
    if args.restored:settings+="ui.queue.visualizer = true\nui.queue.visualizer.mode = 5\n"
    (profile / "config/chilpa/settings.conf").write_text(settings)
    if args.dsp_package:
        dsp_spec = importlib.util.spec_from_file_location("dsp_checks", scripts / "check-dsp.py")
        dsp_checks = importlib.util.module_from_spec(dsp_spec)
        dsp_spec.loader.exec_module(dsp_checks)
        manifest, files = dsp_checks.read_package(args.dsp_package.resolve(strict=True))
        plugins = profile / "data/chilpa/plugins"
        for name, data in files.items():
            destination = plugins / manifest["id"] / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)
        approval = {key: manifest.get(key) for key in ("kind", "capabilities", "auth")}
        approval["runtime"] = manifest.get("runtime", "wasm")
        (plugins / "approvals.json").write_text(json.dumps({manifest["id"]: approval}))
    music = profile / "fixture.wav"
    samples = array("h")
    rate = 48000
    generator = random.Random(143)
    for index in range(rate * 48):
        seconds = index / rate
        amplitude = 0.15 + 0.12 * math.sin(seconds * math.tau * 0.8)
        left = generator.uniform(-.5,.5) if args.main else amplitude * math.sin(index * math.tau * 440 / rate)
        right = generator.uniform(-.5,.5) if args.main else amplitude * math.sin(index * math.tau * 659 / rate + 0.7)
        samples.append(round(32767 * left))
        samples.append(round(32767 * right))
    if os.sys.byteorder != "little":
        samples.byteswap()
    with wave.open(str(music), "wb") as stream:
        stream.setparams((2, 2, rate, 0, "NONE", "not compressed"))
        stream.writeframes(samples.tobytes())
    if args.format == "flac":
        encoded = music.with_suffix(".flac")
        subprocess.run(["ffmpeg", "-nostdin", "-v", "error", "-i", str(music),
                        "-c:a", "flac", str(encoded)], check=True, timeout=30)
        music = encoded
    media_path=str(music)
    if args.http:
        class FixtureHandler(BaseHTTPRequestHandler):
            def log_message(self,*args): pass
            def do_GET(self):
                if self.path!="/"+music.name:self.send_error(404);return
                self.send_response(200);self.send_header("Content-Type","audio/flac" if args.format=="flac" else "audio/wav")
                self.send_header("Content-Length",str(music.stat().st_size));self.end_headers()
                try:
                    with music.open("rb") as source:
                        while chunk:=source.read(65536):self.wfile.write(chunk)
                except (BrokenPipeError,ConnectionResetError):pass
        server=ThreadingHTTPServer(("127.0.0.1",0),FixtureHandler)
        worker=threading.Thread(target=server.serve_forever,daemon=True);worker.start()
        cleanup.callback(server.server_close);cleanup.callback(worker.join,2);cleanup.callback(server.shutdown)
        media_path=f"http://127.0.0.1:{server.server_port}/{music.name}"
    buffer = dict(id=1, name="Now playing", renamed=False, epoch=1, paths=[media_path], origins=[],
        removed=[False], placement=["Original"], groups=[], order=[0], cursor=0,
        seconds=0, shuffle=False, repeat=0, stop_after=None)
    (profile / "data/chilpa/listenlist.json").write_text(json.dumps(dict(version=1, buffers=[buffer], active=1, selected=1, next=2)))
    fixture = profile / "live.qml"
    template = "main-live-visualizer.qml" if args.main else "queue-visualizer.qml" if args.queues else "live-visualizer.qml"
    fixture.write_text((scripts / "fixtures" / template).read_text()
        .replace('"OUTPUT_DIRECTORY"', json.dumps(str(output)))
        .replace('"RESTORED"', str(args.restored).lower())
        .replace('"MINIMUM_VISUAL_PEAK"',str(args.minimum_visual_peak))
        .replace('"FIXTURE_VOLUME"',str(args.volume))
        .replace('"FIXTURE_MUTED"',str(args.muted).lower())
        .replace('fixture.wav',music.name)
        .replace('"file:capture"', json.dumps(audio_device))
        .replace('"WITH_DSP"', "true" if args.dsp_package else "false"))
    inherited = {key:value for key,value in os.environ.items()
                 if not key.startswith(("CHILPA_", "QOBUZ_", "TIDAL_", "APPLE_", "SPOTIFY_", "PODCAST_"))}
    environment = dict(inherited, XDG_CONFIG_HOME=str(profile / "config"), XDG_DATA_HOME=str(profile / "data"),
        XDG_CACHE_HOME=str(profile / "cache"), XDG_RUNTIME_DIR=str(profile / "runtime"), TMPDIR=str(profile / "tmp"),
        QT_QPA_PLATFORM="offscreen" if args.headless_rhi else "xcb", QT_NO_XDG_DESKTOP_PORTAL="1", CHILPA_QML_SMOKE=str(fixture),
        CHILPA_CAPTURE_REALTIME="1", QSG_RHI_BACKEND=args.backend, CHILPA_RENDERER=args.backend)
    if args.headless_rhi:
        environment.update(QT_QUICK_BACKEND="rhi", QSG_INFO="1")
    environment.update(audio_environment)
    log_path = output / "live-visualizer.log"
    from contextlib import nullcontext
    display = nullcontext(os.environ.get("DISPLAY", "")) if args.host_display or args.headless_rhi else scene_checks.isolated_display(output)
    started = time.monotonic()
    bus=profile/"bus.conf"
    bus.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+str(profile/"runtime")+'</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*" eavesdrop="true"/><allow receive_sender="*" eavesdrop="true"/><allow own="*"/></policy></busconfig>')
    with display as address, log_path.open("w") as log:
        environment["DISPLAY"] = address
        result = subprocess.run(["timeout", "--kill-after=2s", "65s", "dbus-run-session", "--config-file="+str(bus), "--", str(args.binary.resolve())],
            env=environment, cwd=profile, stdout=log, stderr=subprocess.STDOUT, timeout=70)
    transcript = log_path.read_text()
    errors = ("SMOKE FAIL", "ReferenceError:", "TypeError:", "Binding loop", "Unable to assign", "is not a type",
              "Failed to compile shader", "Failed to build graphics pipeline state", "Failed to link shader program")
    diagnostics = dict.fromkeys(line for line in transcript.splitlines()
        if "SMOKE" in line or "qrc:" in line or any(error in line for error in errors))
    print("\n".join(list(diagnostics)[:30]))
    capture = profile / "tmp/chilpa-capture.raw"
    captured_bytes = capture.stat().st_size if capture.exists() else 0
    print(f"SMOKE live capture bytes={captured_bytes}")
    marker = "SMOKE Main visualizer completed" if args.main else "SMOKE queue visualizer completed" if args.queues else "SMOKE live visualizer completed"
    passed = (result.returncode == 0 and (args.private_pipewire or captured_bytes > rate * 4)
              and marker in transcript and not any(error in transcript for error in errors))
    (output / "report.json").write_text(json.dumps(dict(binary=str(args.binary.resolve()), backend=args.backend,
        captured_bytes=captured_bytes, audio_device=audio_device, private_pipewire=args.private_pipewire, volume=args.volume, muted=args.muted, format=args.format, minimum_visual_peak=args.minimum_visual_peak, exit_code=result.returncode, passed=passed, log=str(log_path),
        seconds=round(time.monotonic() - started, 2)), indent=2) + "\n")
    raise SystemExit(0 if passed else 1)
