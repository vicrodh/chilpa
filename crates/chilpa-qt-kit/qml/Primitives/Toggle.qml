import QtQuick
import "../Theme"

// An on/off switch (design/03 Toggle).
Rectangle {
    id: toggle

    property bool checked: false
    property bool compact: false
    readonly property int knobInset: compact ? Metrics.toggleCompactInset : Metrics.toggleInset

    signal toggled(bool checked)

    implicitWidth: compact ? Metrics.toggleCompactWidth : Metrics.toggleWidth
    implicitHeight: compact ? Metrics.toggleCompactHeight : Metrics.toggleHeight
    radius: height / 2 // derived: a pill
    color: checked ? Theme.color.accent : Theme.color.stateSelected

    Behavior on color { ColorAnimation { duration: Motion.base } }

    // Tab reaches it, and Space or Enter flips it, as QBZ's; taking the key
    // keeps Space from the window's play and pause.
    activeFocusOnTab: enabled
    Keys.onPressed: (event) => {
        if (toggle.enabled && !event.isAutoRepeat && (event.key === Qt.Key_Space || event.key === Qt.Key_Return || event.key === Qt.Key_Enter)) {
            toggle.toggled(!toggle.checked)
            event.accepted = true
        }
    }
    FocusRing { onAccent: toggle.checked }

    Rectangle {
        x: toggle.checked ? toggle.width - width - toggle.knobInset : toggle.knobInset
        anchors.verticalCenter: parent.verticalCenter
        width: toggle.compact ? Metrics.toggleCompactKnob : Metrics.toggleKnob
        height: width
        radius: width / 2 // derived: a circle
        color: Theme.color.toggleKnob

        Behavior on x { NumberAnimation { duration: Motion.base; easing.type: Easing.OutCubic } }
    }

    HoverHandler { margin: toggle.compact ? Metrics.gap4 : 0; cursorShape: Qt.PointingHandCursor }
    TapHandler { margin: toggle.compact ? Metrics.gap4 : 0; gesturePolicy: TapHandler.ReleaseWithinBounds; onTapped: toggle.toggled(!toggle.checked) }
}
