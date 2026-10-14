import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/qml"

ApplicationWindow {
    id: window
    width: 1280; height: 800; visible: true
    property var albums: []
    property int sample: 0
    property int phase: 0
    property double started: 0
    property bool awaiting: false
    property real trackHeight: 0
    property var failures: []
    ScreenHost { id: host; anchors.fill: parent }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const found = find(child, name)
            if (found) return found
        }
        return null
    }
    function request() {
        if (sample >= albums.length) {
            for (const failure of failures) console.log("SMOKE FAIL " + failure)
            console.log("SMOKE album opening completed samples=" + sample + " failures=" + failures.length)
            Qt.exit(failures.length ? 1 : 0)
            return
        }
        phase = 0; awaiting = true; started = Date.now()
        Navigation.go("album", albums[sample])
    }
    function check() {
        const page = host.item, view = find(page, "trackListView")
        if (phase === 0) {
            trackHeight = view.contentHeight
            page.contentY = Math.max(0, page.contentHeight - page.height)
            phase = 1; next.restart(); return
        }
        if (phase === 1) {
            const last = view.itemAtIndex(view.count - 1)
            if (!last || !find(last, "trackRowBody")) failures.push("last track has no visual row")
            if (view.contentHeight !== trackHeight) failures.push("windowing changed track geometry")
            page.contentY = 0
            phase = 2; next.restart(); return
        }
        if (phase === 2 && sample === 0) {
            page.showInformation()
            phase = 3; next.restart(); return
        }
        if (phase === 3) {
            const popup = find(page, "albumInformationLoader")?.item
            if (!popup || !popup.opened) failures.push("record information failed to open")
            if (popup) popup.close()
            page.showActions(page, 30, 30)
            phase = 4; next.restart(); return
        }
        if (phase === 4) {
            const popup = find(page, "albumActionMenuLoader")?.item
            if (!popup || !popup.opened) failures.push("record menu failed to open")
            if (popup) popup.close()
        }
        sample++; request()
    }
    Connections {
        target: window
        function onFrameSwapped() {
            if (!window.awaiting || !host.item || host.item.albumId !== window.albums[window.sample]) return
            const elapsed = Date.now() - window.started
            const view = window.find(host.item, "trackListView")
            if (!view || !view.model.ready || !view.model.count) return
            let rows = 0
            for (let i = 0; i < view.count; i++) {
                const row = view.itemAtIndex(i)
                if (row && window.find(row, "trackRowBody")) rows++
            }
            console.log("SMOKE album=" + window.albums[window.sample] + " first-frame-with-tracks=" + elapsed + "ms tracks=" + view.model.count + " visual-rows=" + rows)
            if (elapsed > 500) window.failures.push("local record took " + elapsed + "ms to present its tracks")
            if (!rows || view.model.count > 100 && rows >= 60) window.failures.push("record built " + rows + " visual rows")
            window.awaiting = false
            next.restart()
        }
    }
    Timer { id: next; interval: 600; onTriggered: window.check() }
    Timer { interval: 1000; running: true; onTriggered: window.request() }
    Timer { interval: 25000; running: true; onTriggered: { console.log("SMOKE FAIL album opening timed out"); Qt.exit(1) } }
}
