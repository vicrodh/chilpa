import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import app.chilpa.about
import "qrc:/qt/qml/app/chilpa/qml" as Shell

Item {
    id: smoke
    property var mainWindow
    property int phase: 0
    property bool stopped: false
    property string scenario: "SMOKE_SCENARIO"
    property int duration: SMOKE_DURATION
    property double began: Date.now()
    property double cycleAt: 0
    property int cycles: 0
    property int waits: 0
    property string path: ""
    property int volume: 0
    property int queueRevision: 0
    property string other: ""
    Component { id: mainTemplate; Shell.Main {} }
    SettingsPage { id: audio; section: "audio" }
    function check(condition, message) { if (!condition) throw new Error(message) }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) { const result = find(child, name); if (result) return result }
        return null
    }
    function document() {
        AudioOutput.refresh()
        const doc = JSON.parse(AudioOutput.snapshot || "{}")
        check(!doc.error,"audio task error: " + doc.error)
        return doc
    }
    function opening() {
        const doc = JSON.parse(AudioOutput.snapshot || "{}")
        return JSON.stringify({output:doc.selection,generation:doc.inspection.identity.generation,
            format:{sample_format:"s16",sample_rate_hz:44100,channels:2},exclusive:false})
    }
    function complete() {
        console.log("SMOKE audio real Main completed cycles=" + cycles)
        stopped = true; Qt.exit(0)
    }
    function advance() {
        if (phase === 0) {
            Onboarding.command("close")
            mainWindow = mainTemplate.createObject(null)
            check(!!mainWindow,"real Main did not construct")
            mainWindow.width = 1100; mainWindow.height = 850
            Qt.createQmlObject('import QtQuick; Rectangle { anchors.fill: parent; z: -100 }',mainWindow.contentItem).color = Qt.binding(() => mainWindow.color)
            phase = 1; return
        }
        if (scenario === "idle") {
            check(!Player.playing || Player.paused,"idle comparison started audio")
            if (Date.now() - began >= duration) complete()
            return
        }
        if (phase === 1) {
            if (!Player.path.endsWith("fixture.wav")) return
            audio.stopForDiagnosis()
            audio.press("audio.outputDetails")
            phase = 2; return
        }
        if (phase === 2) {
            if (!AudioOutput.shown) return
            const doc = document()
            if (doc.busy || !doc.inspection) return
            check(doc.selection === "file:capture","details did not inspect the selected output")
            check(!Player.playing && !Cast.active,"inspection started playback or casting")
            AudioOutput.command("check",opening())
            phase = 3; return
        }
        if (phase === 3) {
            const doc = document()
            if (doc.busy || !doc.last) return
            check(!doc.error && doc.saved && doc.last.state === "available","silent check failed: " + JSON.stringify(doc.last) + doc.error)
            check(doc.last.audible === "untested","opening claimed audibility")
            check(doc.last.opened.sample_rate_hz === 44100,"wrong opening result")
            mainWindow.contentItem.grabToImage(result => {
                check(result.saveToFile("SMOKE_CAPTURE"),"panel capture could not be saved")
                AudioOutput.close(); phase = 4; waits = 0
            })
            phase = 14; return
        }
        if (phase === 4) {
            check(!AudioOutput.shown && AudioOutput.snapshot === "{}","closed view retained its payload")
            AudioOutput.refresh()
            check(AudioOutput.snapshot === "{}","late refresh repopulated a closed view")
            if (++waits < 3) return
            Player.toggle(); phase = 5; return
        }
        if (phase === 5) {
            if (!Player.playing || !Player.outputFormat) return
            path = Player.path; volume = Player.volume; queueRevision = Player.queueRev
            AudioOutput.show("now"); phase = 6; return
        }
        if (phase === 6) {
            const doc = document()
            if (doc.busy || !doc.now?.route) return
            check(doc.now.destination === "file:capture","live route was rebuilt from inspection labels")
            check(doc.now.route.delivery.requested.sample_rate_hz === 44100,"requested stream was lost")
            check(doc.now.samplesChanged === false,"silent unity PCM was reported as processed")
            other = (doc.outputs || []).find(o => o.id !== "file:capture" && !o.id.startsWith("cast:"))?.id || ""
            if (other) AudioOutput.command("inspect",other)
            phase = 7; return
        }
        if (phase === 7) {
            const doc = document()
            if (doc.busy || !doc.inspection) return
            check(Player.playing && Player.path === path && Player.volume === volume && Player.queueRev === queueRevision,
                  "inspecting another output changed listening context")
            check(doc.current === "file:capture" && doc.now.destination === "file:capture","inspection selected another destination")
            if (other) check(doc.selection === other,"another output was not inspected")
            AudioOutput.close(); Player.toggle(); phase = 12; return
        }
        if (phase === 12) {
            if (!Player.paused) return
            AudioOutput.show("now"); phase = 13; return
        }
        if (phase === 13) {
            const doc = document()
            if (doc.busy || !doc.now?.route) return
            check(Player.paused && Player.path === path && doc.now.available,"details changed paused playback")
            AudioOutput.close(); audio.stopForDiagnosis(); phase = 8; return
        }
        if (phase === 8) {
            if (Player.playing) return
            AudioOutput.show("equipment"); phase = 9; return
        }
        if (phase === 9) {
            const doc = document()
            if (doc.busy || !doc.inspection) return
            check(doc.history.length > 0,"saved case did not survive closing the view")
            AudioOutput.command("check",opening()); AudioOutput.command("cancel","")
            phase = 10; return
        }
        if (phase === 10) {
            const doc = document()
            if (doc.busy || !doc.last) return
            check(["cancelled","available"].includes(doc.last.state),"cancel request left an unexpected result")
            check(doc.last.audible === "untested","cancelled opening claimed audibility")
            console.log("SMOKE audio cancel requested result=" + doc.last.state)
            AudioOutput.close(); cycles++; cycleAt = Date.now(); phase = 11; return
        }
        if (phase === 11) {
            check(!AudioOutput.shown && AudioOutput.snapshot === "{}","idle view retained output details")
            check(!Player.playing,"closed-cycle transport was not stopped")
            AudioOutput.refresh()
            check(AudioOutput.snapshot === "{}","late publication repopulated closed output details")
            if (scenario !== "lifecycle" || Date.now() - began >= duration) { complete(); return }
            if (Date.now() - cycleAt >= 110000) { Player.toggle(); phase = 5 }
        }
    }
    Timer {
        interval: 250; repeat: true; running: !smoke.stopped
        onTriggered: {
            try { smoke.advance() }
            catch (error) { smoke.stopped = true; console.error("SMOKE FAIL audio phase=" + smoke.phase + " " + error.message); Qt.exit(1) }
        }
    }
    Timer {
        interval: smoke.duration + 180000; running: !smoke.stopped
        onTriggered: { console.error("SMOKE FAIL audio timeout phase=" + smoke.phase); Qt.exit(1) }
    }
}
