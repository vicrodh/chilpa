import QtQuick
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// The player bar's auto-hide (Settings > Playback > Hide when idle), as a
// Plasma panel's: once the pointer has been off the bar for a while the whole
// bar slides out of the window, and it comes back when the pointer reaches
// the window's bottom edge. It never leaves from under the pointer.
//
// The handler hangs from `target` -- the window's root, beneath everything --
// and not from an item laid over the window: over everything it took the
// hover from every control under it (their hover and their tooltips stayed
// on) and its arrow won over every cursor.
Item {
    id: watch

    required property Item target
    // The pointer is on the bar.
    property bool holding: false
    property bool away: false

    width: 0
    height: 0

    // Where the pointer brings the bar back: the strip the bar sits in.
    readonly property real edge: target.height - Metrics.playerSpace - Metrics.playerBottom * 2 // derived: the bar's own strip

    onHoldingChanged: if (!holding) wait.restart()

    HoverHandler {
        parent: watch.target
        onPointChanged: {
            if (watch.away && point.position.y >= watch.edge) {
                watch.away = false
                wait.restart()
            }
        }
    }

    Timer {
        id: wait
        interval: Metrics.playerIdle
        running: SettingsModel.hidePlayer && !watch.away && !watch.holding
        onTriggered: watch.away = SettingsModel.hidePlayer && !watch.holding
    }
}
