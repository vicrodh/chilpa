import QtQuick
import QtQuick.Controls.Basic
import "../Theme"

// Draws TipState's tooltip under the item that asked, inside the window --
// in the layer popups are drawn in and above them, so an entry of an open
// menu can have one too. Given what the window holds, any line in it that
// is cut short says itself whole there as well.
Rectangle {
    id: tip

    property Item watching: null

    parent: Overlay.overlay
    z: Metrics.tooltipLayer

    readonly property Item target: TipState.target
    readonly property point at: target ? target.mapToItem(parent, 0, target.height) : Qt.point(0, 0)

    visible: TipState.shown && target !== null
    // As wide as its words, up to the window less its margins; a longer line
    // wraps there instead of running past the window's edge.
    width: Math.min(label.implicitWidth, parent.width - Metrics.spaceWindow * 2 - Metrics.gap16) + Metrics.gap16 // derived: the padding on both sides
    height: label.implicitHeight + Metrics.gap10
    x: Math.max(Metrics.spaceWindow, Math.min(at.x + (target ? target.width : 0) / 2 - width / 2, parent.width - width - Metrics.spaceWindow)) // derived: centered under the target
    y: Math.min(at.y + Metrics.gap6, parent.height - height - Metrics.spaceWindow)
    radius: Metrics.radiusMenu
    color: Theme.color.surfaceRaised

    Edge {}

    Loader {
        active: tip.watching !== null
        sourceComponent: CutTextTips { watching: tip.watching }
    }

    Text {
        id: label
        anchors.centerIn: parent
        width: parent.width - Metrics.gap16
        wrapMode: Text.Wrap
        text: TipState.text
        font.pointSize: Type.small
        color: Theme.color.textPrimary
    }
}
