import QtQuick
import "../Theme"

// A filter that is on or off (design/03 Chip): inverted when it is on.
Rectangle {
    id: chip

    property string text
    property bool active: false

    signal clicked()

    implicitWidth: label.implicitWidth + Metrics.gap10 * 2 // derived: padding on both sides
    implicitHeight: Metrics.chipHeight
    radius: height / 2 // derived: a pill
    color: active ? Theme.color.textPrimary : hover.hovered ? Theme.color.stateHover : Theme.color.fillField

    Text {
        id: label
        anchors.centerIn: parent
        text: chip.text
        font.pointSize: Type.meta
        font.weight: Type.medium
        color: chip.active ? Theme.color.surfaceWindow : Theme.color.textSecondary
    }

    HoverHandler { id: hover }
    HoverHandler { cursorShape: Qt.PointingHandCursor }
    TapHandler { gesturePolicy: TapHandler.ReleaseWithinBounds; onTapped: chip.clicked() }
}
