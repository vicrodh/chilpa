import QtQuick
import "../Theme"

// Draws TipState's tooltip under the item that asked, inside the window.
Rectangle {
    id: tip

    readonly property Item target: TipState.target
    readonly property point at: target ? target.mapToItem(parent, 0, target.height) : Qt.point(0, 0)

    visible: TipState.shown && target !== null
    width: label.implicitWidth + Metrics.gap16
    height: label.implicitHeight + Metrics.gap10
    x: Math.max(Metrics.spaceWindow, Math.min(at.x + (target ? target.width : 0) / 2 - width / 2, parent.width - width - Metrics.spaceWindow)) // derived: centered under the target
    y: Math.min(at.y + Metrics.gap6, parent.height - height - Metrics.spaceWindow)
    radius: Metrics.radiusMenu
    color: Theme.color.surfaceRaised
    border.color: Theme.color.strokeHairline

    Text {
        id: label
        anchors.centerIn: parent
        text: TipState.text
        font.pointSize: Type.small
        color: Theme.color.textPrimary
    }
}
