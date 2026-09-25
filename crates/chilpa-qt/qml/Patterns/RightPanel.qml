import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// The right panel (design/02 5, frontend ADR-0011): the queue and the
// lyrics share one card. With both open a divider splits them, and the
// queue's share is remembered; each keeps its minimum.
Rectangle {
    id: card

    readonly property bool both: SettingsModel.queueOpen && SettingsModel.lyricsOpen
    property real dragged: -1
    readonly property real queueShare: dragged >= 0 ? dragged
        : SettingsModel.queueHeight > 0 ? SettingsModel.queueHeight : Metrics.queueDefault
    readonly property real queueHeight: !SettingsModel.queueOpen ? 0
        : !SettingsModel.lyricsOpen ? height
        : Math.max(Metrics.queueMin, Math.min(queueShare, height - Metrics.lyricsMin))

    radius: Metrics.radiusPanel
    color: Theme.color.surfaceSidebar

    Edge {}

    QueuePanel {
        visible: SettingsModel.queueOpen
        width: parent.width
        height: card.queueHeight
        onClosed: SettingsModel.queueOpen = false
    }

    ResizeHandle {
        id: split
        visible: card.both
        vertical: false
        width: parent.width
        y: card.queueHeight - height / 2 // derived: centered on the seam
        onMoved: (delta) => card.dragged = Math.max(Metrics.queueMin, Math.min(card.height - Metrics.lyricsMin, (card.dragged >= 0 ? card.dragged : card.queueHeight) + delta))
        onReleased: {
            if (card.dragged >= 0)
                SettingsModel.queueHeight = Math.round(card.dragged)
            card.dragged = -1
        }
    }

    LyricsPanel {
        visible: SettingsModel.lyricsOpen
        y: card.queueHeight
        width: parent.width
        height: parent.height - y
        onClosed: SettingsModel.lyricsOpen = false
    }
}
