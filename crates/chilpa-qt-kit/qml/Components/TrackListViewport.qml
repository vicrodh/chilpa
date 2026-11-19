import QtQuick
import "../Theme"

// The visible row and its offset survive a reset of the same track list.
QtObject {
    id: viewport
    required property var model
    required property var view
    property var place: null
    // A list that scrolls itself pools at most three screens of compact rows, more than it holds at once at one size; past that, once laid out, its whole pool goes.
    readonly property var band: view ? [view.height, view.cacheBuffer, view.contentHeight] : []
    onBandChanged: if (view && view.interactive) Qt.callLater(viewport.trim)
    function trim() {
        view.forceLayout()
        if (view.visible && view.contentItem.children.filter(slot => slot.resident && !slot.visible).length > 3 * Math.ceil(view.height / Metrics.rowCompact)) { view.reuseItems = false; view.reuseItems = true }
    }
    // One reset restoration per turn, cancelled with the owning list.
    readonly property Timer pending: Timer { interval: 0; onTriggered: viewport.restore() }
    function restoreLater() { pending.restart() }
    function positionAt(row, mode) {
        place = null
        view.forceLayout()
        view.positionViewAtIndex(row, mode)
    }
    // Restart estimates before a far jump through section bands. Reusing the
    // old pool while rebuilding the model can briefly show the wrong row.
    function jumpTo(row) {
        view.positioning = true
        view.reuseItems = false
        try {
            view.model = null
            view.model = Qt.binding(() => viewport.model)
            positionAt(row, ListView.Beginning)
        } finally { view.positioning = false; view.reuseItems = true }
    }
    function identity() { return model ? model.kind + "\n" + model.albumId + "\n" + model.key : "" }
    function keep() {
        const row = view.indexAt(0, view.contentY)
        const item = view.itemAtIndex(row)
        place = item ? { list: identity(), row: row, into: view.contentY - item.y } : null
    }
    function restore() {
        const was = place
        place = null
        if (!was || was.list !== identity() || view.count === 0)
            return
        view.forceLayout()
        if (was.row >= view.count) {
            view.positionViewAtEnd()
            return
        }
        view.positionViewAtIndex(was.row, ListView.Beginning)
        view.contentY += was.into
        view.returnToBounds()
    }
}
