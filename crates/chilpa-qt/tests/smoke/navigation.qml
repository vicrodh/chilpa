import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/qml"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

ApplicationWindow {
    id: window
    width: 1280; height: 800; visible: true
    property var destinations: ["albums", "home", "albums", "home", "albums", "home"]
    property int sample: 0
    property string requested: ""
    property double started: 0
    property bool awaiting: false
    property var failures: []
    property int resets: 0
    property int initialResets: -1
    property int checkedRevision: -1
    ScreenHost { id: host; anchors.fill: parent }
    AlbumGridModel { id: snapshot; kind: "all"; columns: 6 }
    Connections { target: snapshot; function onModelReset() { window.resets++ } }
    Timer {
        id: audit
        interval: 100; repeat: true; running: true
        onTriggered: {
            if (!snapshot.ready) return
            if (window.initialResets < 0) {
                window.initialResets = window.resets
                window.checkedRevision = snapshot.rev
                snapshot.contentRev++
            } else if (snapshot.rev !== window.checkedRevision) {
                if (window.resets !== window.initialResets) window.failures.push("unchanged album snapshot reset its rows")
                console.log("SMOKE album snapshot refresh resets=" + (window.resets - window.initialResets))
                stop()
            }
        }
    }
    function seedModules() {
        const sections = [], choices = []
        for (let index = 0; index < 40; index++) {
            const id = "source:fixture:module-" + index
            sections.push({id:"module-" + index, homeId:"fixture:module-" + index, sourceId:"fixture",
                title:"Module " + index, enabled:true, kind:"album", items:[{ref:"album:fixture:" + index}]})
            choices.push({id:id, enabled:true, size:0})
        }
        Visit.keep("home.source.sections", sections)
        Visit.keep("home.source.list", [{id:"fixture", name:"Fixture", enabled:true}])
        SettingsModel.keep("ui.home.sections", JSON.stringify(choices))
    }
    function request() {
        if (sample >= destinations.length) {
            if (audit.running) { next.restart(); return }
            for (const failure of failures) console.log("SMOKE FAIL " + failure)
            console.log("SMOKE navigation completed samples=" + sample + " failures=" + failures.length)
            Qt.exit(failures.length ? 1 : 0)
            return
        }
        requested = destinations[sample]
        if (requested === "home") seedModules()
        awaiting = true
        started = Date.now()
        Navigation.go(requested, 0)
    }
    Connections {
        target: window
        function onFrameSwapped() {
            if (!window.awaiting || host.committedRoute.split("\u001f")[1] !== window.requested) return
            const elapsed = Date.now() - window.started
            console.log("SMOKE navigation " + window.requested + " first-frame=" + elapsed + "ms")
            if (elapsed > 500) window.failures.push(window.requested + " first frame took " + elapsed + "ms")
            window.awaiting = false
            window.sample++
            next.restart()
        }
    }
    Timer { id: next; interval: 400; onTriggered: window.request() }
    Timer { interval: 1000; running: true; onTriggered: window.request() }
    Timer { interval: 20000; running: true; onTriggered: { console.log("SMOKE FAIL navigation timed out"); Qt.exit(1) } }
}
