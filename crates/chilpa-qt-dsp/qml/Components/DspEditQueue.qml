import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

Item {
    id: queue
    property string action: ""
    property var pendingInput: null
    property string latestAction: ""
    property var latestData: null
    signal command(string action, var data)
    function preview(action, data) {
        queue.latestAction = action
        queue.latestData = data
        queue.action = action
        queue.pendingInput = data
        if (!pending.running) pending.start()
    }
    function cancel() { pending.stop(); action = ""; pendingInput = null; latestAction = ""; latestData = null }
    function commit(action, data) { cancel(); command(action, data) }
    function flush() {
        if (!latestAction) return
        const next = latestAction === "filter_preview" ? "filter_commit" : latestAction === "graphic_preview" ? "graphic_band" : latestAction === "preview" ? "edit" : latestAction
        const value = latestData
        cancel()
        command(next, value)
    }
    Timer {
        id: pending
        interval: Metrics.dspPreviewInterval
        onTriggered: {
            if (!queue.action) return
            queue.command(queue.action, queue.pendingInput)
            queue.action = ""
            queue.pendingInput = null
        }
    }
}
