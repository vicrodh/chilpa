import QtQuick
import QtQuick.Controls.Basic
import app.chilpa

ApplicationWindow {
    id: window
    visible: true
    width: 1280; height: 800
    property int sample: -1
    property double started: 0
    property bool waiting: false
    property bool burst: false
    property string dsdSummary: ""
    property var cases: ["dsd", "all", "dsd", "all", "hires", "all", "flac", "all"]
    function advance() {
        sample++
        if (sample === cases.length) {
            burst = true
            started = Date.now()
            waiting = true
            Library.toggleFilter("quality", "dsd")
            Library.resetExtra()
            Library.toggleFilter("quality", "hires")
            Library.resetExtra()
            Library.toggleFilter("format", "flac")
            Library.resetExtra()
            Library.toggleFilter("quality", "dsd")
            return
        }
        waiting = true
        started = Date.now()
        if (cases[sample] === "all") Library.resetExtra()
        else Library.toggleFilter(cases[sample] === "flac" ? "format" : "quality", cases[sample])
    }
    Connections {
        target: Library
        function onBusyChanged() {
            if (!Library.busy && window.waiting) {
                window.waiting = false
                if (window.burst) {
                    if (Library.summary !== window.dsdSummary) {
                        console.log("SMOKE FAIL latest filter summary changed: " + Library.summary)
                        Qt.exit(1)
                        return
                    }
                    console.log("SMOKE burst latest-filter elapsed=" + (Date.now() - window.started) + "ms")
                    console.log("SMOKE filtering completed samples=" + window.sample)
                    Qt.exit(0)
                    return
                }
                if (window.sample === 0) window.dsdSummary = Library.summary
                console.log("SMOKE filtering " + window.cases[window.sample] + " elapsed="
                    + (Date.now() - window.started) + "ms summary=" + Library.summary)
                next.restart()
            }
        }
    }
    Timer { id: next; interval: 100; onTriggered: window.advance() }
    Timer {
        interval: 100; repeat: true; running: true
        onTriggered: { if (!Library.busy) { stop(); next.restart() } }
    }
    Timer { interval: 80000; running: true; onTriggered: { console.log("SMOKE FAIL filtering timed out"); Qt.exit(1) } }
}
