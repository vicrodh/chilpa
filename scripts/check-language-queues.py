#!/usr/bin/env python3
"""I check language selection and queue edits with the compiled Qt bridges."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

binary = Path(sys.argv[1]).resolve()
fixture = r'''
import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/qml/Screens" as Screens
import "qrc:/qt/qml/app/chilpa/controls/qml/Screens" as QueueScreens
import "qrc:/qt/qml/app/chilpa/controls/qml/Patterns" as Actions

ApplicationWindow {
    id: window
    width: 1100; height: 760; visible: true
    Actions.WindowControls { id: controls; window: window }
    Loader { id: page; anchors.fill: parent; sourceComponent: Navigation.screen === "listenlist" ? queues : settings }
    Component { id: settings; Screens.SettingsScreen {} }
    Component { id: queues; QueueScreens.ListenListScreen {} }
    property int phase: 0
    property int waits: 0
    function check(ok, message) {
        if (!ok) { console.log("SMOKE FAIL " + message); Qt.exit(1) }
        return ok
    }
    function descendants(item, visited) {
        visited = visited || new Set()
        if (!item || visited.has(item)) return []
        visited.add(item)
        let result = [item]
        for (const child of item.data || item.children || []) result = result.concat(descendants(child, visited))
        if (item.contentItem) result = result.concat(descendants(item.contentItem, visited))
        return result
    }
    function find(name) { return descendants(page.item).find(item => item.objectName === name) }
    function languageRow() { return descendants(page.item).find(item => item.key === "appearance.language") }
    Component.onCompleted: Navigation.openSection("appearance")
    Timer {
        interval: 100; repeat: true; running: true
        onTriggered: {
            if (!window.check(++window.waits < 100, "native queue and language checks timed out")) return
            ListenList.refresh()
            if (window.phase === 0) {
                const row = window.languageRow()
                if (!row) return
                const select = window.descendants(row).find(item => typeof item.picked === "function")
                if (!select) return
                select.picked("es")
                window.phase = 1
            } else if (window.phase === 1) {
                if (!window.check(I18n.language === "es" && window.languageRow().title === "Idioma", "the live selector speaks Spanish")) return
                window.check(Navigation.tabTitles[Navigation.active] === I18n.tr("Settings", I18n.rev), "tab labels follow the language")
                Navigation.open("albums", 0)
                const previous = Hotkeys.pressedIn("window", Qt.Key_Backtab, Qt.ControlModifier | Qt.ShiftModifier, "", false)
                window.check(previous === "nav.previousTab", "the native bridge recognizes reverse tab")
                controls.run(previous)
                window.check(Navigation.screen === "settings", "the previous tab retains its route")
                controls.run(Hotkeys.pressedIn("window", Qt.Key_Right, Qt.ControlModifier | Qt.AltModifier, "", false))
                window.phase = 2
            } else if (window.phase === 2) {
                if (!page.item || page.item.selectedQueue !== "2") return
                const rows = window.find("trackListView")
                const model = rows ? rows.model : null
                if (!model || model.count < 4) return
                model.listAction("split", 3, -1)
                ListenList.refresh()
                window.check(JSON.parse(ListenList.snapshot).active === "1", "splitting another queue preserves the playing queue")
                window.phase = 3
            } else if (window.phase === 3) {
                const doc = JSON.parse(ListenList.snapshot)
                if (!window.check(doc.tabs.length === 3 && doc.selected === "3", "the native row menu splits the selected queue")) return
                window.find("queueMerge").clicked()
                window.phase = 4
            } else if (window.phase === 4) {
                const dialog = window.find("queueMergeDialog")
                if (!dialog || !dialog.opened) return
                const all = window.descendants(dialog.contentItem)
                const first = all.find(item => item.objectName === "queueMergeChoice" && item.text.startsWith("First"))
                if (!first) return
                first.checked = true; first.toggled()
                all.find(item => item.objectName === "mergedQueueOriginals").picked("remove")
                all.find(item => item.objectName === "mergedQueueName").text = "Native combined"
                all.find(item => item.objectName === "confirmQueueMerge").clicked()
                window.check(dialog.error === "" && !dialog.opened, "the compiled combination action succeeds")
                window.phase = 5
            } else {
                const doc = JSON.parse(ListenList.snapshot)
                window.check(doc.tabs.length === 2 && doc.selected === "1" && doc.active === "1", "only the chosen originals are retired")
                window.check(doc.tabs.find(tab => tab.id === "1").count === 4, "the merged queue keeps repeated tracks")
                console.log("SMOKE language, shortcuts, split and merge completed")
                Qt.quit()
            }
        }
    }
}
'''
reopened = r'''
import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
ApplicationWindow {
    visible: true; width: 500; height: 400
    Timer {
        interval: 100; running: true
        onTriggered: {
            ListenList.refresh()
            const doc = JSON.parse(ListenList.snapshot)
            if (I18n.language !== "es" || doc.tabs.length !== 2 || doc.selected !== "1" || doc.name !== "Native combined") {
                console.log("SMOKE FAIL language or queues were lost between sessions"); Qt.exit(1); return
            }
            console.log("SMOKE language and queues restored in a new process")
            Qt.quit()
        }
    }
}
'''

with tempfile.TemporaryDirectory(prefix="chilpa-language-queues-") as temporary:
    profile = Path(temporary)
    for folder in ("config", "cache", "runtime", "data/chilpa"):
        (profile / folder).mkdir(parents=True)
    (profile / "runtime").chmod(0o700)
    def buffer(identity, name, names, cursor=0):
        paths = [str(profile / (name + track + ".flac")) for track in names]
        return dict(id=identity, name=name, renamed=True, epoch=1, paths=paths, origins=[None] * len(paths),
            removed=[False] * len(paths), placement=["Original"] * len(paths), groups=[], order=list(range(len(paths))),
            cursor=cursor, seconds=12 if cursor else 0, shuffle=False, repeat=0, stop_after=None)
    session = profile / "data/chilpa/listenlist.json"
    session.write_text(json.dumps(dict(version=1, buffers=[buffer(1, "First", ["a", "b", "a"], 1),
        buffer(2, "Second", ["c", "d"])], active=1, selected=1, next=3)))
    environment = dict(os.environ, XDG_CONFIG_HOME=str(profile / "config"), XDG_DATA_HOME=str(profile / "data"),
        XDG_CACHE_HOME=str(profile / "cache"), XDG_RUNTIME_DIR=str(profile / "runtime"),
        QT_QPA_PLATFORM="offscreen", CHILPA_RENDERER="software")
    for code in (fixture, reopened):
        path = profile / "probe.qml"
        path.write_text(code)
        result = subprocess.run(["timeout", "--signal=TERM", "--kill-after=2s", "20s", "dbus-run-session", "--", str(binary)],
            env=dict(environment, CHILPA_QML_SMOKE=str(path)), text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        errors = ("SMOKE FAIL", "ReferenceError:", "TypeError:", "binding loop", "QQmlApplicationEngine failed", "is not a type")
        if result.returncode or any(error in result.stdout for error in errors) or "SMOKE language" not in result.stdout:
            print(result.stdout); sys.exit(1)
        print("\n".join(line for line in result.stdout.splitlines() if "SMOKE" in line))
    stored = json.loads(session.read_text())
    merged = next(row for row in stored["buffers"] if row["id"] == 1)
    assert merged["paths"][1] == str(profile / "Firstb.flac")
    assert merged["cursor"] == 1 and merged["seconds"] == 12
    print("The compiled app retained the playback occurrence and position in its saved session.")
