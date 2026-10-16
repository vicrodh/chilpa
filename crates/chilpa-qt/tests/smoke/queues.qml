import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/controls/qml/Screens"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

ApplicationWindow {
    id: window
    width: 1100; height: 700; visible: true
    ListenListScreen { id: page; anchors.fill: parent }
    QueueProgress { id: notice }
    Component { id: trackMaker; TrackListModel { kind:"tracks"; key:"album\u001foff\u001fQueue smoke fixture" } }
    property var addedModel: null
    property int fixtureAlbum: 0
    property int phase: 0
    property double started: 0
    property double lastTick: Date.now()
    property double longestTick: 0
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) { const found = find(child, name); if (found) return found }
        return null
    }
    Timer {
        interval: 16; running: true; repeat: true
        onTriggered: {
            const now = Date.now()
            if (window.phase >= 3) window.longestTick = Math.max(window.longestTick, now - window.lastTick)
            window.lastTick = now
        }
    }
    function check(condition, message) {
        if (!condition) { console.log("SMOKE FAIL " + message); Qt.exit(1) }
        return condition
    }
    Timer {
        interval: 100; running: true; repeat: true
        onTriggered: {
            ListenList.refresh()
            const rows = window.find(page, "trackListView")
            if (!rows || rows.count < 80 || (window.phase < 3 && page.playingRow < 0)) return
            if (window.phase === 0) {
                if (!window.check(typeof rows.model.playingRow === "function", "the native model exposes playingRow")) return
                window.check(rows.model.playingRow() === page.playingRow, "the screen reads the actual native method")
                window.check(window.find(page, "queueFocusPlaying").enabled, "focus is enabled in the restored paused queue")
                window.find(page, "queueFocusPlaying").clicked()
            } else if (window.phase === 1) {
                const item = rows.itemAtIndex(page.playingRow)
                if (!window.check(!!item && Math.abs(item.y + item.height / 2 - rows.contentY - rows.height / 2) < 2, "focus centers the playing occurrence")) return
                window.check(page.followPlaying, "follow remains active")
                window.find(page, "queueBackToTop").clicked()
            } else if (window.phase === 2) {
                window.check(rows.atYBeginning && !page.followPlaying, "top returns to the first row and cancels follow")
                console.log("SMOKE queues native focus and top passed rows=" + rows.count)
                window.addedModel = trackMaker.createObject(window)
            } else if (window.phase === 3) {
                if (!window.addedModel.ready) return
                if (!window.check(window.addedModel.count === 20000, "the fixture query has 20000 tracks, found " + window.addedModel.count)) return
                window.started = Date.now()
                window.addedModel.queueAll()
                QueueActivity.refresh()
                window.check(QueueActivity.busy && notice.visible, "queue insertion reports work immediately")
                window.addedModel.destroy()
                window.addedModel = null
            } else if (window.phase === 4) {
                if (Number(page.document.tabs?.find(tab => tab.id === "1")?.count) !== 20080) return
                QueueActivity.refresh()
                if (QueueActivity.busy) return
                console.log("SMOKE queues added 20000 tracks across navigation elapsed=" + (Date.now() - window.started) + "ms")
                window.started = Date.now()
                AlbumActions.listAction(window.fixtureAlbum, "buffer:new")
                QueueActivity.refresh()
                window.check(QueueActivity.busy, "a long album reports path resolution")
            } else {
                if (!page.document.tabs?.some(tab => tab.id !== "1" && tab.count === 20000)) return
                QueueActivity.refresh()
                if (QueueActivity.busy) return
                console.log("SMOKE queues added 20000 album tracks elapsed=" + (Date.now() - window.started) + "ms longest-ui-tick=" + window.longestTick + "ms")
                if (!window.check(window.longestTick < 500, "long queue additions keep the event loop responsive")) return
                console.log("SMOKE queues completed")
                Qt.exit(0)
            }
            ++window.phase
        }
    }
    Timer { interval: 20000; running: true; onTriggered: { console.log("SMOKE FAIL queue smoke timed out phase=" + window.phase); Qt.exit(1) } }
}
