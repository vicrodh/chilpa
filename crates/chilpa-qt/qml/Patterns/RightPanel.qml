import QtQuick
import app.chilpa
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The right panel (design/02 5, frontend ADR-0011): the queue and the
// lyrics share one card. With both open a divider splits them anywhere
// between their minimums, and the queue's share is remembered.
Rectangle {
    id: card

    property Component mediaContent
    readonly property var source: JSON.parse(Sources.snapshot || "{}")
    readonly property bool spoken: !!source.playing_entry && ["radio", "podcasts"].indexOf(source.medium) >= 0
    readonly property bool both: SettingsModel.queueOpen && SettingsModel.lyricsOpen
    // While the divider is dragged: the queue's height, and what it was when
    // the drag started.
    property real dragged: -1
    property real grabbed: 0
    // Half and half until the divider is moved; a share and not a height,
    // so the split keeps its proportion when the window changes size.
    readonly property real share: SettingsModel.queueShare > 0 ? SettingsModel.queueShare / Metrics.shareUnits : 0.5
    readonly property real queueHeight: !SettingsModel.queueOpen ? 0
        : !SettingsModel.lyricsOpen ? height
        : clamp(dragged >= 0 ? dragged : height * share)

    // Anywhere between the two minimums (design/04 7).
    function clamp(h) {
        return Math.max(Metrics.queueMin, Math.min(h, height - Metrics.lyricsMin))
    }

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
        onStarted: card.grabbed = card.queueHeight
        onMoved: (delta) => card.dragged = card.clamp(card.grabbed + delta)
        onReleased: {
            if (card.dragged >= 0)
                SettingsModel.queueShare = Math.max(1, Math.round(card.dragged / card.height * Metrics.shareUnits))
            card.dragged = -1
        }
    }

    Loader {
        active: card.spoken && SettingsModel.lyricsOpen
        visible: SettingsModel.lyricsOpen && card.spoken
        sourceComponent: card.mediaContent
        y: card.queueHeight; width: parent.width; height: parent.height - y
    }

    LyricsPanel {
        visible: SettingsModel.lyricsOpen && !card.spoken
        y: card.queueHeight
        width: parent.width
        height: parent.height - y
        onClosed: SettingsModel.lyricsOpen = false
    }
}
