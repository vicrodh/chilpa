import QtQuick
import com.blitzfc.chilpa
import "Theme"

// When the player bar may step away (Settings > Playback > Hide when idle):
// after a while with the mouse still, anywhere in the window. Any movement
// brings it back and starts the wait again. The handler is passive, so
// nothing under it loses its hover.
Item {
    id: watch

    // The pointer is on the bar itself: it does not step away from under it.
    property bool holding: false
    property bool away: false

    // Where the pointer was last seen. Qt hands the hover again with the
    // pointer standing still whenever what is under it changes -- the bar
    // sliding away is one -- and that is not the person moving the mouse.
    property point last: Qt.point(-1, -1)

    HoverHandler {
        onPointChanged: {
            const at = point.position
            if (at.x === watch.last.x && at.y === watch.last.y)
                return
            watch.last = at
            watch.away = false
            wait.restart()
        }
    }

    Timer {
        id: wait
        interval: Metrics.playerIdle
        running: SettingsModel.hidePlayer
        onTriggered: watch.away = SettingsModel.hidePlayer && !watch.holding
    }
}
