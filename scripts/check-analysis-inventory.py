#!/usr/bin/env python3
"""I check the compiled inventory, live progress and persistence in an isolated profile."""
import argparse
import json
import os
from pathlib import Path
import signal
import sqlite3
import subprocess
import tempfile
import threading

parser = argparse.ArgumentParser()
parser.add_argument('binary', type=Path)
parser.add_argument('--library', type=Path, help='Use a read-only snapshot of an existing library.')
parser.add_argument('--log', type=Path)
args = parser.parse_args()
args.binary = args.binary.resolve(strict=True)
logs = []

BOOTSTRAP = '''import QtQuick
import QtQuick.Controls.Basic
import app.chilpa.stats
ApplicationWindow {
    width:800; height:600; visible:true
    Timer { interval:400; running:true; onTriggered: { Stats.inventory("all","",0,0); Qt.exit(0) } }
}
'''
PROBE = '''import QtQuick
import QtQuick.Controls.Basic
import app.chilpa.stats
import "qrc:/qt/qml/app/chilpa/stats/qml/Patterns"
ApplicationWindow {
    id: window
    width:1280; height:800; visible:true
    property int phase:0
    property int frames:0
    property int observedFrame:0
    property int total: EXPECTED_TOTAL
    property var before:null
    property double started:0
    AnalysisInventoryDialog { id: inventory }
    Connections { target:window; function onFrameSwapped() { window.frames++ } }
    function find(item,name) {
        if (item.objectName===name) return item
        for (const child of item.children || []) { const result=find(child,name); if(result) return result }
        return null
    }
    function fail(message) { console.log("SMOKE FAIL "+message); Qt.exit(1) }
    Timer { interval:500; running:true; onTriggered: { inventory.show(); window.started=Date.now(); steps.start() } }
    Timer { id: steps; interval:200; repeat:true; onTriggered: {
        const list=window.find(inventory.contentItem,"inventoryTrackList")
        if (inventory.error) return window.fail(inventory.error)
        if (window.phase===0) {
            if (!list || !list.itemAtIndex(0)) return
            if (inventory.summary.total!==window.total || list.count!==Math.min(100,window.total)) return window.fail("saved inventory count differs")
            if (!list.height || !list.itemAtIndex(0).height) return window.fail("track rows have no visible geometry")
            console.log("SMOKE saved inventory tracks="+inventory.summary.total+" rows="+list.count)
            inventory.chooseRun(RUN_ID); window.observedFrame=window.frames; window.phase=1; return
        }
        if (window.phase===1) {
            if (window.frames<=window.observedFrame || list.count!==EXPECTED_RESULTS || !list.itemAtIndex(0)) return
            if (inventory.summary.runs[0].state!=="interrupted") return window.fail("an unfinished previous pass was reported as running")
            if (EXPECTED_RESULTS===3) {
                if (inventory.summary.runs[0].processed!==3) return window.fail("saved progress was lost after reopening")
                console.log("SMOKE persisted inventory processed=3 results=3")
                inventory.history=true; window.observedFrame=window.frames; window.phase=3; return
            }
            window.before=list.itemAtIndex(0); window.phase=2
            console.log("SMOKE READY live-inventory")
            return
        }
        if (window.phase===2 && list.count===3 && inventory.summary.runs[0].processed===3) {
            window.observedFrame=window.frames; window.phase=4; return
        }
        if (window.phase===4 && window.frames>window.observedFrame) {
            if (EXPECTED_RESULTS===2 && list.itemAtIndex(1)!==window.before) return window.fail("live progress recreated an existing row")
            console.log("SMOKE live inventory processed=3 saved=2 failed=1 previous="+inventory.summary.runs[0].previous)
            inventory.history=true; window.observedFrame=window.frames; window.phase=3; return
        }
        if (window.phase===3) {
            const history=window.find(inventory.contentItem,"inventoryRunList")
            if (window.frames<=window.observedFrame || !history || !history.itemAtIndex(0)) return
            console.log("SMOKE analysis history rows="+history.count)
            Qt.exit(0)
        }
    }}
    Timer { interval:12000; running:true; onTriggered: window.fail("inventory timed out") }
}
'''

with tempfile.TemporaryDirectory(prefix='chilpa-inventory-smoke-') as directory:
    profile = Path(directory)
    for name in ('config', 'cache', 'runtime', 'data/chilpa'):
        (profile / name).mkdir(parents=True)
    (profile / 'runtime').chmod(0o700)
    database = profile / 'data/chilpa/library.db'
    if args.library:
        source = sqlite3.connect(args.library.resolve().as_uri() + '?mode=ro', uri=True)
        target = sqlite3.connect(database)
        source.backup(target)
        source.close(); target.close()
    environment = dict(os.environ, XDG_CONFIG_HOME=str(profile/'config'), XDG_DATA_HOME=str(profile/'data'),
        XDG_CACHE_HOME=str(profile/'cache'), XDG_RUNTIME_DIR=str(profile/'runtime'), QT_QPA_PLATFORM='offscreen', CHILPA_RENDERER='software')
    def run(qml, change=None):
        fixture = profile/'probe.qml'; fixture.write_text(qml)
        process = subprocess.Popen(['dbus-run-session', '--', str(args.binary)],
            env=dict(environment, CHILPA_QML_SMOKE=str(fixture)), stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            text=True, start_new_session=True)
        watchdog = threading.Timer(20, lambda: os.killpg(process.pid, signal.SIGKILL) if process.poll() is None else None)
        watchdog.start()
        output = []
        try:
            for line in process.stdout:
                output.append(line)
                if 'SMOKE READY live-inventory' in line and change: change()
            result = process.wait()
        finally:
            watchdog.cancel()
        text = ''.join(output); logs.append(text)
        for line in output:
            if 'SMOKE' in line or 'qrc:' in line: print(line.rstrip())
        errors = ('SMOKE FAIL','ReferenceError:','TypeError:','binding loop','QQmlApplicationEngine failed','is not a type')
        if result or any(error in text for error in errors): raise RuntimeError('The compiled inventory smoke failed.\n'+text)
    try:
        run(BOOTSTRAP)
        db = sqlite3.connect(database)
        if not args.library:
            db.execute("INSERT INTO sources(provider,instance,kind) VALUES ('local','inventory-fixture','local')")
            source = db.execute('SELECT last_insert_rowid()').fetchone()[0]
            for i in range(3):
                db.execute("INSERT INTO tracks(source_id,external_id,title,artist,added_at) VALUES (?,?,?,'Fixture artist',0)",
                    (source,f'/inventory-track-{i}.flac',f'Inventory track {i}'))
            db.commit()
        tracks = db.execute("SELECT t.id FROM tracks t JOIN sources s ON s.id=t.source_id WHERE s.kind='local' AND t.present=1 AND t.reachable=1 ORDER BY t.id LIMIT 3").fetchall()
        if len(tracks)<3: raise RuntimeError('The inventory fixture needs at least three local tracks.')
        total = db.execute("SELECT count(*) FROM tracks t JOIN sources s ON s.id=t.source_id WHERE s.kind='local' AND t.present=1 AND t.reachable=1").fetchone()[0]
        db.execute("INSERT INTO analysis_runs(analyzer,version,started_at,total,previous,processed,saved,failed) VALUES ('chilpa',2,1780000000,3,7,2,1,1)")
        run_id = db.execute('SELECT last_insert_rowid()').fetchone()[0]
        for i, state in enumerate(('saved','failed')):
            db.execute('INSERT INTO analysis_results(run_id,track_id,state,analyzed_at) VALUES (?,?,?,?)',(run_id,tracks[i][0],state,1780000000+i))
        db.commit()
        def advance():
            with db:
                db.execute("INSERT INTO analysis_results(run_id,track_id,state,analyzed_at) VALUES (?,?,'saved',1780000002)",(run_id,tracks[2][0]))
                db.execute('UPDATE analysis_runs SET processed=3,saved=2 WHERE id=?',(run_id,))
        def probe(expected):
            return PROBE.replace('EXPECTED_TOTAL',str(total)).replace('EXPECTED_RESULTS',str(expected)).replace('RUN_ID',str(run_id))
        run(probe(2),advance)
        run(probe(3))
        print('Compiled inventory, live progress, stable rows and persistence across application sessions passed')
    finally:
        if args.log:
            args.log.parent.mkdir(parents=True,exist_ok=True)
            args.log.write_text('\n'.join(logs))
