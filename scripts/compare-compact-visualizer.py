#!/usr/bin/env python3
"""I compare visualizers in isolated Qt processes with controlled synthetic input.

The optional native library adds the canonical producers and a silent PCM tap.
This is not a physical-device or whole-application budget. I retain raw CPU,
RSS/PSS, DRM engines and GPU frequencies. Missing hardware GPU counters leave
acceptance unproved; software rendering never earns a GPU gate.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import signal
import shutil
import statistics
import subprocess
import tempfile
import time
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
QT = Path('/usr/lib64/qt6/bin')
LIBEXEC = Path('/usr/lib64/qt6/libexec')


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def sample(pid):
    try:
        proc = Path('/proc') / str(pid)
        fields = (proc / 'stat').read_text().rsplit(')', 1)[1].split()
        memory = dict(re.findall(r'^(Rss|Pss|Anonymous):\s+(\d+)', (proc / 'smaps_rollup').read_text(), re.M))
        engines, clients = {}, set()
        for fd in (proc / 'fdinfo').iterdir():
            try:
                info = fd.read_text()
            except OSError:
                continue
            client = re.search(r'^drm-client-id:\s*(\S+)', info, re.M)
            driver = re.search(r'^drm-driver:\s*(\S+)', info, re.M)
            device = re.search(r'^drm-pdev:\s*(\S+)', info, re.M)
            if not client or not driver:
                continue
            key = (driver[1], device[1] if device else '', client[1])
            if key in clients:
                continue
            clients.add(key)
            for engine, value in re.findall(r'^drm-engine-([^:\s]+):\s*(\d+)\s+ns', info, re.M):
                name = driver[1] + ':' + engine
                engines[name] = engines.get(name, 0) + int(value)
        frequencies = {}
        for card in Path('/sys/class/drm').glob('card[0-9]'):
            for frequency in card.glob('gt/gt*/rps_*_freq_mhz'):
                if frequency.name not in ('rps_cur_freq_mhz', 'rps_act_freq_mhz'):
                    continue
                try:
                    frequencies[str(frequency)] = int(frequency.read_text())
                except OSError:
                    pass
        return dict(time=time.monotonic(), cpu_ticks=int(fields[11]) + int(fields[12]),
                    start_time=int(fields[19]), memory_kib={k: int(v) for k, v in memory.items()},
                    engines_ns=engines, gpu_frequency_mhz=frequencies)
    except (FileNotFoundError, ProcessLookupError):
        return None
    except (OSError, ValueError, IndexError) as error:
        raise RuntimeError(f"Cannot sample owned process {pid}: {error}") from error


def phase_summary(points, phase):
    selected = [point for point in points if point['phase'] == phase]
    if phase == 'idle' and selected:
        start = selected[0]['time'] + 1.0
        selected = [point for point in selected if point['time'] >= start]
    if len(selected) < 2:
        return None
    groups = {}
    for point in selected:
        groups.setdefault(point.get('cycle', 0), []).append(point)
    spans = [(group[0], group[-1]) for group in groups.values() if len(group) > 1]
    elapsed = sum(last['time'] - first['time'] for first, last in spans)
    cpu = sum(last['cpu_ticks'] - first['cpu_ticks'] for first, last in spans)
    engines = {}
    for first, last in spans:
        for key, value in first['engines_ns'].items():
            if key in last['engines_ns']:
                engines[key] = engines.get(key, 0) + last['engines_ns'][key] - value
    return dict(seconds=elapsed,
                cpu_one_core_percent=100 * cpu / os.sysconf('SC_CLK_TCK') / elapsed,
                rss_mib=statistics.median(point['memory_kib']['Rss'] for point in selected) / 1024,
                rss_peak_mib=max(point['memory_kib']['Rss'] for point in selected) / 1024,
                pss_mib=statistics.median(point['memory_kib']['Pss'] for point in selected) / 1024,
                gpu_engine_percent={key: 100 * value / (elapsed * 1e9) for key, value in engines.items()})


def prepare(base, reference, producer_library):
    imports = base / 'imports'
    shutil.copytree(ROOT / 'crates/chilpa-qt-kit/tests/qml/imports', imports)
    for module in ['ambient', 'immersive']:
        shutil.copytree(ROOT / f'crates/chilpa-qt-{module}/tests/qml/imports/app/chilpa/{module}', imports / f'app/chilpa/{module}', dirs_exist_ok=True)
    bridge = imports / 'com/blitzfc/qbz'
    bridge.mkdir(parents=True)
    (bridge / 'qmldir').write_text('module com.blitzfc.qbz\nsingleton QbzShell 1.0 QbzShell.qml\n' + ('' if producer_library else 'singleton QbzViz 1.0 QbzViz.qml\n'))
    (bridge / 'QbzShell.qml').write_text('''pragma Singleton
import QtQuick
QtObject {
 property int pulseMs: 0
 property int largeSpectrumMode: 0
 property bool gpuTier: true
 property color ambientPrimary: "#42d9c8"
 property color ambientSecondary: "#416bd8"
 property color ambientAccent: "#d8aa41"
 function largeCycleSpectrum() {}
}
''')
    (bridge / 'QbzViz.qml').write_text('''pragma Singleton
import QtQuick
QtObject {
 property var bars: []
 property var waveform: []
 property var energy: []
 property var goniometer: []
 property var oscilloscope: []
 property real stereoCorrelation: 0.6
 function setCompact(active, mode) {}
}
''')
    ref_qml = reference / 'crates/qbz-qt/qml'
    copied = base / 'reference'
    for folder in ['shell', 'controls', 'theme', 'assets/shaders']:
        (copied / folder).mkdir(parents=True)
    for relative in ['shell/SpectrumBand.qml', 'shell/VizSettle.qml', 'controls/ScopePanel.qml']:
        shutil.copy2(ref_qml / relative, copied / relative)
    for path in (ref_qml / 'assets/shaders').glob('spectrum*.qsb'):
        shutil.copy2(path, copied / 'assets/shaders' / path.name)
    (copied / 'theme/QbzTheme.qml').write_text('import QtQuick\nQtObject { property int radiusMd: 8; property color accent: "#42d9c8" }\n')
    resources = ET.Element('RCC')
    kit = ROOT / 'crates/chilpa-qt-kit'
    collection = ET.SubElement(resources, 'qresource', prefix='/qt/qml/app/chilpa/kit')
    for folder in ['qml', 'assets']:
        for path in sorted((kit / folder).rglob('*')):
            if path.is_file():
                ET.SubElement(collection, 'file', alias=path.relative_to(kit).as_posix()).text = str(path)
    immersive = ROOT / 'crates/chilpa-qt-immersive'
    collection = ET.SubElement(resources, 'qresource', prefix='/qt/qml/app/chilpa/immersive')
    for path in sorted((immersive / 'qml').rglob('*')):
        if path.is_file():
            ET.SubElement(collection, 'file', alias=path.relative_to(immersive).as_posix()).text = str(path)
    ET.ElementTree(resources).write(base / 'resources.qrc')
    subprocess.run([str(LIBEXEC / 'rcc'), '--binary', str(base / 'resources.qrc'), '-o', str(base / 'resources.rcc')], check=True)
    flags = shlex.split(subprocess.check_output(['pkg-config', '--cflags', '--libs', 'Qt6Quick', 'Qt6Qml'], text=True))
    feed_sources, feed_link = [], []
    if producer_library:
        feed = ROOT / 'crates/chilpa-qt-graphics/tests/visualizer-bench-feed.cpp'
        feed_moc = base / 'feed-moc.cpp'
        subprocess.run([str(LIBEXEC / 'moc'), str(feed.with_suffix('.h')), '-o', str(feed_moc)], check=True)
        feed_sources = [str(feed), str(feed_moc)]
        feed_link = [str(producer_library), '-Wl,-rpath,' + str(producer_library.parent)]
    for name, directory in [('candidate', ROOT / 'crates/chilpa-qt-graphics/cxx'), ('reference', reference / 'crates/qbz-qt/cxx')]:
        source = directory / 'scope_trace_item.cpp'
        moc = base / f'{name}-moc.cpp'
        subprocess.run([str(LIBEXEC / 'moc'), str(source.with_suffix('.h')), '-o', str(moc)], check=True)
        subprocess.run(['c++', '-O2', '-std=c++17', '-shared', '-fPIC', str(ROOT / 'crates/chilpa-qt-kit/tests/resource_loader.cpp'),
                        str(source), str(moc), *feed_sources, '-o', str(base / f'{name}.so'), *feed_link, *flags], check=True)
    return imports


def fixture(base, name, mode, args):
    component = '''CompactVisualizer {
 id: renderer; width: BAND_WIDTH; height: BAND_HEIGHT; feed: QbzViz; active: root.drawing; mode: MODE
 topColor: QbzShell.ambientPrimary; bottomColor: QbzShell.ambientSecondary
 scopeRenderer: Component { CompactScope {} }
}''' if name == 'candidate' else '''SpectrumBand {
 id: renderer; width: BAND_WIDTH; height: BAND_HEIGHT; shown: root.drawing; capturing: shown
}'''
    imports = 'import app.chilpa.graphics\nimport "qrc:/qt/qml/app/chilpa/kit/qml/Components"\nimport "qrc:/qt/qml/app/chilpa/immersive/qml/Components"' if name == 'candidate' else 'import "reference/shell"'
    if args.surface == 'immersive':
        imports = ('import \"qrc:/qt/qml/app/chilpa/immersive/qml/Components\"' if name == 'candidate' else 'import \"reference/controls\"')
        component = '''ScopePanel {
 id: renderer; width: BAND_WIDTH; height: BAND_HEIGHT; scopeMode: MODE - 3; visible: root.drawing
 traceColor: QbzShell.ambientPrimary
 ''' + ('feed: QbzViz' if name == 'candidate' else 'compact: false') + '\n}'
    text = '''import QtQuick
import QtQuick.Window
import app.chilpa.kit
import com.blitzfc.qbz
IMPORTS
Window {
 id: root; width: WINDOW_WIDTH; height: WINDOW_HEIGHT; visible: true; color: "#18202a"
 property string phase: "idle"
 readonly property bool drawing: phase === "warmup" || phase === "active"
 property int frameCount: 0
 property int cycle: 0
 property int index: 0
 property real started: Date.now()
 property var tables: []
 onFrameSwapped: ++frameCount
 COMPONENT
 Component.onCompleted: {
  Pulse.automatic = false
  QbzShell.largeSpectrumMode = REFERENCE_MODE
  CONFIGURE
  for (let f = 0; f < TABLE_COUNT; ++f) {
   let bars = [], wave = [], energy = [], gon = [], osc = []
   for (let i = 0; i < 16; ++i) bars.push(i === 1 || i === 15 ? 0 : 0.4 + 0.35 * Math.sin(f * 0.09 + i * 0.3))
   for (let i = 0; i < 5; ++i) energy.push(0.4 + 0.35 * Math.sin(f * 0.09 + i * 0.5))
   for (let i = 0; i < 512; ++i) {
    wave.push(0.7 * Math.sin(i * 0.13 + f * 0.08))
    gon.push(0.6 * Math.sin(Math.floor(i / 2) * 0.09 + f * 0.07 + (i % 2) * 0.8))
    osc.push(0.7 * Math.sin(i * 0.025 + f * 0.02))
   }
   tables.push({bars:bars,waveform:wave,energy:energy,goniometer:gon,oscilloscope:osc})
  }
  console.log("BENCH ready")
 }
 Timer {
  interval: 33; repeat: true; running: root.drawing
  onTriggered: {
   DELIVER
   QbzShell.pulseMs += 33; Pulse.ms += 33
  }
 }
 Timer {
  interval: 1000; repeat: true; running: true
  onTriggered: {
   const elapsed = (Date.now() - root.started) / 1000
   const duration = WARMUP + SECONDS + HIDDEN
   const afterIdle = Math.max(0, elapsed - IDLE)
   root.cycle = Math.min(CYCLES - 1, Math.floor(afterIdle / duration))
   const within = afterIdle % duration
   if (elapsed < IDLE) root.phase = "idle"
   else if (within >= WARMUP + SECONDS || afterIdle >= CYCLES * duration) root.phase = "hidden"
   else if (within >= WARMUP) root.phase = "active"
   else root.phase = "warmup"
   ENABLE
   console.log("BENCH sample " + JSON.stringify({phase:root.phase,cycle:root.cycle,frames:root.frameCount,elapsed:elapsed,api:root.GraphicsInfo.api STATS}))
   if (afterIdle >= CYCLES * duration) Qt.quit()
  }
 }
}
'''
    native = bool(args.producer_library)
    configure = f"QbzViz.configure({'true' if name == 'reference' else 'false'}, {mode}, {args.sample_rate}, {args.sample_rate * args.delay_ms // 1000}, {'true' if args.surface == 'compact' else 'false'})" if native else ""
    deliver = "QbzViz.poll()" if native else """let frame = root.tables[root.index++ % root.tables.length]
   if (MODE === 1) QbzViz.waveform = frame.waveform
   else if (MODE === 2) QbzViz.energy = frame.energy
   else if (MODE === 3) QbzViz.goniometer = frame.goniometer
   else if (MODE === 4) QbzViz.oscilloscope = frame.oscilloscope
   else QbzViz.bars = frame.bars"""
    replacements = dict(HIDDEN=str(args.hidden_seconds), CYCLES=str(args.cycles), CONFIGURE=configure, DELIVER=deliver, TABLE_COUNT='0' if native else '180',
                        ENABLE='QbzViz.setEnabled(root.drawing)' if native else '',
                        STATS=',submitted:QbzViz.submitted(),overwritten:QbzViz.overwritten()' if native else '',
                        IDLE=str(args.idle_seconds), IMPORTS=imports, COMPONENT=component, REFERENCE_MODE=str(mode if mode < 5 else 0), MODE=str(mode), WARMUP=str(args.warmup), SECONDS=str(args.seconds), BAND_WIDTH=str(args.band_width), BAND_HEIGHT=str(args.band_height), WINDOW_WIDTH=str(args.window_width), WINDOW_HEIGHT=str(args.window_height))
    for key, value in replacements.items():
        text = text.replace(key, value)
    path = base / f'{name}-{mode}.qml'
    path.write_text(text)
    return path


def run(base, output, imports, name, mode, pair, args):
    profile = base / f'{name}-{mode}-{pair}'
    profile.mkdir()
    for folder in ['config', 'data', 'cache', 'runtime', 'tmp']:
        (profile / folder).mkdir(mode=0o700)
    env = dict(os.environ, XDG_CONFIG_HOME=str(profile / 'config'), XDG_DATA_HOME=str(profile / 'data'),
               XDG_CACHE_HOME=str(profile / 'cache'), XDG_RUNTIME_DIR=str(profile / 'runtime'), TMPDIR=str(profile / 'tmp'),
               QT_QPA_PLATFORM=args.platform, QT_QUICK_BACKEND='rhi', QSG_RHI_BACKEND=args.backend,
               QSG_INFO='1', QT_NO_XDG_DESKTOP_PORTAL='1',
               QML_IMPORT_PATH=str(imports), LD_PRELOAD=str(base / f'{name}.so'), CHILPA_QML_RESOURCES=str(base / 'resources.rcc'))
    path = fixture(base, name, mode, args)
    log_path = output / f'{pair}-{mode}-{name}.log'
    points = []
    with log_path.open('w') as log:
        process = subprocess.Popen(['dbus-run-session', '--', str(QT / 'qml'), str(path)], env=env, stdout=log, stderr=log, start_new_session=True)
        target = None
        deadline = time.monotonic() + args.idle_seconds + (args.warmup + args.seconds + args.hidden_seconds) * args.cycles + 15
        try:
            while process.poll() is None and time.monotonic() < deadline:
                if target is None:
                    # I inspect only descendants of the process I started.
                    for proc in Path('/proc').iterdir():
                        if not proc.name.isdigit():
                            continue
                        try:
                            fields = (proc / 'stat').read_text().rsplit(')', 1)[1].split()
                            if int(fields[1]) == process.pid and (proc / 'comm').read_text().strip() != 'dbus-daemon':
                                target = int(proc.name)
                        except (OSError, ValueError, IndexError):
                            continue
                if target:
                    point = sample(target)
                    if point:
                        records = [json.loads(line.split('BENCH sample ', 1)[1]) for line in log_path.read_text().splitlines() if 'BENCH sample ' in line]
                        point['phase'] = records[-1]['phase'] if records else 'idle'
                        point['cycle'] = records[-1]['cycle'] if records else 0
                        points.append(point)
                time.sleep(0.25)
        finally:
            if process.poll() is None:
                # This fresh session contains only my bus and renderer.
                os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
    transcript = log_path.read_text()
    errors = ['ReferenceError:', 'TypeError:', 'is not a type', 'module "', 'Failed to link', 'Failed to deserialize', 'Unable to assign']
    if process.returncode != 0 or any(error in transcript for error in errors) or not points:
        raise RuntimeError(f'{name} mode {mode} failed (exit={process.returncode}, target={target}, samples={len(points)}); inspect {log_path}')
    active = [point for point in points if point['phase'] == 'active']
    if len(active) < 4:
        raise RuntimeError(f'not enough active samples in {log_path}')
    summary = phase_summary(points, 'active')
    summary['process_peak_rss_mib'] = max(point['memory_kib']['Rss'] for point in points) / 1024
    records = [json.loads(line.split('BENCH sample ', 1)[1]) for line in transcript.splitlines() if 'BENCH sample ' in line]
    hidden_groups, active_groups = {}, {}
    for record in records:
        groups = active_groups if record['phase'] == 'active' else hidden_groups if record['phase'] == 'hidden' else None
        if groups is not None:
            groups.setdefault(record['cycle'], []).append(record)
    active_spans = [(group[0], group[-1]) for group in active_groups.values() if len(group) >= 2]
    hidden_spans = [(group[1], group[-1]) for group in hidden_groups.values() if len(group) >= 3]
    duration = sum(last['elapsed'] - first['elapsed'] for first, last in active_spans)
    summary['hidden_frames_after_settle'] = sum(last['frames'] - first['frames'] for first, last in hidden_spans)
    summary['idle'] = phase_summary(points, 'idle')
    summary['hidden'] = phase_summary(points, 'hidden')
    if args.producer_library:
        summary['producer_frames_per_second'] = sum(last['submitted'] - first['submitted'] for first, last in active_spans) / duration
        summary['overwritten_frames'] = sum(last['overwritten'] - first['overwritten'] for first, last in active_spans)
        summary['hidden_producer_frames_after_settle'] = sum(last['submitted'] - first['submitted'] for first, last in hidden_spans)
    summary['frames_per_second'] = sum(last['frames'] - first['frames'] for first, last in active_spans) / duration
    result = dict(implementation=name, mode=mode, pair=pair, summary=summary, samples=points, frames=records, log=str(log_path))
    (output / f'{pair}-{mode}-{name}.json').write_text(json.dumps(result, indent=2) + '\n')
    if not 28 <= summary['frames_per_second'] <= 32:
        raise RuntimeError(f"Unmatched visible cadence ({summary['frames_per_second']:.2f} FPS); inspect {log_path}")
    if not summary['gpu_engine_percent'] or any(record['api'] in (0, 1) for record in records):
        raise RuntimeError(f'Hardware GPU counters are required; inspect {log_path}')
    print(json.dumps(dict(implementation=name, mode=mode, pair=pair, **summary)), flush=True)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference', type=Path, required=True, help='Local checkout with the canonical compact renderer')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--surface', choices=['compact', 'immersive'], default='compact')
    parser.add_argument('--producer-library', type=Path, help='Release cdylib of the canonical Rust producers and synthetic silent feed')
    parser.add_argument('--sample-rate', type=int, default=48_000)
    parser.add_argument('--delay-ms', type=int, default=500)
    parser.add_argument('--idle-seconds', type=float, default=5)
    parser.add_argument('--hidden-seconds', type=float, default=5)
    parser.add_argument('--backend', choices=['opengl', 'vulkan'], default='opengl')
    parser.add_argument('--platform', choices=['xcb', 'offscreen'], default='xcb')
    parser.add_argument('--modes', type=int, nargs='+', default=list(range(7)))
    parser.add_argument('--pairs', type=int, default=3)
    parser.add_argument('--cycles', type=int, default=1, help='Repeated visible/hidden cycles in the same process for retention checks')
    parser.add_argument('--implementations', choices=['reference', 'candidate'], nargs='+', default=['reference', 'candidate'])
    parser.add_argument('--band-width', type=int, default=232)
    parser.add_argument('--band-height', type=int, default=42)
    parser.add_argument('--window-width', type=int, default=800)
    parser.add_argument('--window-height', type=int, default=600)
    parser.add_argument('--warmup', type=float, default=5)
    parser.add_argument('--seconds', type=float, default=30)
    args = parser.parse_args()
    if args.cycles < 1 or args.pairs < 1 or args.seconds < 3 or args.hidden_seconds < 3 or args.idle_seconds < 1 or args.warmup < 1:
        parser.error('positive pairs/cycles, at least three measured/hidden seconds and one idle/warmup second are required')
    if not 8000 <= args.sample_rate <= 768000 or not 0 <= args.delay_ms <= 500:
        parser.error('the PCM fixture covers 8-768 kHz and 0-500 ms output delay')
    if args.surface == 'immersive' and any(mode not in (3, 4) for mode in args.modes):
        parser.error('immersive surface requires scope modes 3 and/or 4')
    if not os.environ.get('DISPLAY'):
        parser.error('DISPLAY must identify a usable GPU display; no silent software substitution')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    results = []
    with tempfile.TemporaryDirectory(prefix='chilpa-renderer-compare-') as directory:
        base = Path(directory)
        for key, folder in [('XDG_CONFIG_HOME', 'config'), ('XDG_DATA_HOME', 'data'), ('XDG_CACHE_HOME', 'cache'),
                            ('XDG_RUNTIME_DIR', 'runtime'), ('TMPDIR', 'tmp')]:
            (base / folder).mkdir(mode=0o700)
            os.environ[key] = str(base / folder)
        imports = prepare(base, args.reference.resolve(), args.producer_library.resolve() if args.producer_library else None)
        hashes = {
            'candidate': digest(ROOT / 'crates/chilpa-qt-kit/qml/Components/CompactVisualizer.qml'),
            'candidate_shader': digest(ROOT / 'crates/chilpa-qt-kit/assets/shaders/compact-visualizer.frag.qsb'),
            'candidate_scopes': digest(ROOT / 'crates/chilpa-qt-immersive/qml/Components/CompactScope.qml'),
            'reference': digest(args.reference / 'crates/qbz-qt/qml/shell/SpectrumBand.qml'),
            'candidate_native_library': digest(base / 'candidate.so'),
            'reference_native_library': digest(base / 'reference.so'),
            'resources': digest(base / 'resources.rcc'),
            'producer_library': digest(args.producer_library) if args.producer_library else None,
        }
        for pair in range(args.pairs):
            for mode in args.modes:
                for name in (args.implementations if pair % 2 == 0 else list(reversed(args.implementations))):
                    results.append(run(base, output, imports, name, mode, pair, args))
        report = dict(scope=('canonical Rust producers + silent PCM tap + common typed Qt bridge + real renderers' if args.producer_library else 'renderer-only; synthetic precomputed frames; no audio device or user profile'),
                      qt_runner_sha256=digest(QT / 'qml'), window=[args.window_width, args.window_height], band=[args.band_width, args.band_height], period_ms=33,
                      backend=args.backend, platform=args.platform, surface=args.surface, sample_rate=args.sample_rate, delay_ms=args.delay_ms, pairs=args.pairs, cycles=args.cycles, source_hashes=hashes,
                      acceptance='unproved: inspect paired distributions and hardware counters; this does not cover full applications or physical audio devices',
                      results=[{k: v for k, v in result.items() if k not in ['samples', 'frames']} for result in results])
        (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
