import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// One visible track list owns the normal-mode cursor and its row actions.
QtObject {
    id: cursor
    required property var host
    required property var view
    required property var ratings
    required property var menu
    property int row: -1
    function setCursor(at, direction) {
        if (!host.model || view.count === 0) { row = -1; return }
        at = Math.max(0, Math.min(view.count - 1, at))
        while (at >= 0 && at < view.count && !host.model.isTrack(at)) at += direction
        if (at < 0 || at >= view.count) return
        row = at
        host.forceActiveFocus()
        host.model.select(row, 0)
        if (host.viewport) {
            const item = view.itemAtIndex(row)
            if (item) {
                const top = item.mapToItem(host.viewport.contentItem, 0, 0).y
                if (top < host.viewport.contentY) host.viewport.contentY = top
                else if (top + item.height > host.viewport.contentY + host.viewport.height) host.viewport.contentY = top + item.height - host.viewport.height
            }
        } else host.positionAt(row, ListView.Contain)
    }
    function run(action) {
        const step = Math.max(1, Math.floor((host.viewport ? host.viewport.height : view.height) / (host.rich ? Metrics.rowRich : Metrics.rowCompact) / 2))
        if (action === "first") setCursor(0, 1)
        else if (action === "last") setCursor(view.count - 1, -1)
        else if (action === "down" || action === "halfDown") setCursor(row < 0 ? 0 : row + (action === "down" ? 1 : step), 1)
        else if (action === "up" || action === "halfUp") setCursor(row < 0 ? view.count - 1 : row - (action === "up" ? 1 : step), -1)
        else {
            if (row < 0) setCursor(0, 1)
            if (row < 0 || !host.model) return
            if (action === "play") host.model.play(row)
            else if (action === "queue") host.model.queue(row)
            else if (action === "menu") {
                const item = view.itemAtIndex(row)
                menu.show(row, Metrics.gap12, item ? Math.max(0, view.y + item.y - view.contentY) : view.y)
            } else if (action === "favorite") {
                const item = view.itemAtIndex(row)
                if (item) { host.model.favoriteChosen(row, !ratings.favorite("track", item.trackId, ratings.rev)); ratings.refresh() }
            }
        }
    }

}
