pragma Singleton
import QtQuick
import "../Theme"

// The one tooltip of the window: which item asks for it and what it says.
// A button hovers and asks; Tooltip, placed once, draws it. Nothing builds a
// tooltip of its own per button.
QtObject {
    id: tips

    property string text
    property Item target: null
    property bool shown: false

    readonly property Timer delay: Timer {
        interval: Metrics.tooltipDelay
        onTriggered: tips.shown = tips.target !== null
    }

    function show(said, item) {
        text = said
        target = item
        shown = false
        delay.restart()
    }

    function hide(item) {
        if (target !== item)
            return
        delay.stop()
        shown = false
        target = null
    }
}
