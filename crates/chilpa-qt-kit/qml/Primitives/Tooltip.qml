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
    width: label.implicitWidth + Metrics.gap16
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
        text: TipState.text
        font.pointSize: Type.small
        color: Theme.color.textPrimary
    }
}
