pragma Singleton
import QtQuick
import "../Theme"

// The one tooltip of the window: which item asks for it, for what, and what it
// says. A button hovers and asks; Tooltip, placed once, draws it until the item
// is hidden. Nothing builds a tooltip of its own per button.
QtObject {
    id: tips

    property string text
    property Item target: null
    // What the item stands for: the same item standing for something else asks anew.
    property var key
    property bool shown: false
    // Set by what already shows the asked line whole another way: the tooltip stays out of its way.
    property bool quiet: false

    readonly property Timer delay: Timer {
        interval: Metrics.tooltipDelay
        onTriggered: tips.shown = tips.target !== null
    }
    readonly property Connections hiding: Connections { target: tips.target; function onVisibleChanged() { if (tips.target && !tips.target.visible) tips.hide(tips.target) } }

    function show(said, item, standsFor) {
        text = said
        if (target === item && key === standsFor)
            return
        target = item
        key = standsFor
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
