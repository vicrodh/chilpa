import QtQuick
import "../Theme"

// An on/off switch (design/03 Toggle).
Rectangle {
    id: toggle

    property bool checked: false

    signal toggled(bool checked)

    implicitWidth: Metrics.toggleWidth
    implicitHeight: Metrics.toggleHeight
    radius: height / 2 // derived: a pill
    color: checked ? Theme.color.accent : Theme.color.stateSelected

    Behavior on color { ColorAnimation { duration: Motion.base } }

    Rectangle {
        x: toggle.checked ? toggle.width - width - Metrics.toggleInset : Metrics.toggleInset
        anchors.verticalCenter: parent.verticalCenter
        width: Metrics.toggleKnob
        height: Metrics.toggleKnob
        radius: width / 2 // derived: a circle
        color: Theme.color.textOnAccent

        Behavior on x { NumberAnimation { duration: Motion.base; easing.type: Easing.OutCubic } }
    }

    HoverHandler { cursorShape: Qt.PointingHandCursor }
    TapHandler { gesturePolicy: TapHandler.ReleaseWithinBounds; onTapped: toggle.toggled(!toggle.checked) }
}
