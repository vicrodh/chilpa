import QtQuick
import "../Theme"

// A button with text: "primary" in the accent, "secondary" on a field, or
// "ghost": no fill until the pointer is over it, 28 high, as a panel's
// footer has them (design/03 QueueFooter).
Rectangle {
    id: root

    property string text
    property string glyph
    property string iconFamily
    // A glyph from the filled font: play, pause, a filled heart.
    property bool filled: false
    // "primary", "secondary" or "ghost".
    property string kind: "secondary"

    signal clicked()

    readonly property bool primary: kind === "primary"
    readonly property bool ghost: kind === "ghost"

    implicitWidth: row.implicitWidth + (ghost ? Metrics.gap8 : Metrics.pillPadding) * 2 // derived: padding on both sides
    implicitHeight: ghost ? Metrics.button28 : Metrics.pillHeight
    opacity: enabled ? 1 : Metrics.disabledOpacity
    radius: ghost ? Metrics.radiusControl : height / 2 // derived: a pill
    color: ghost ? (area.containsMouse ? Theme.color.stateHover : "transparent")
         : !primary ? (area.containsMouse ? Theme.color.stateSelected : Theme.color.fillField)
         : area.pressed ? Theme.color.accentPressed
         : area.containsMouse ? Theme.color.accentHover : Theme.color.accent

    Row {
        id: row
        anchors.centerIn: parent
        spacing: Metrics.gap6

        Icon {
            anchors.verticalCenter: parent.verticalCenter
            visible: root.glyph !== ""
            glyph: root.glyph
            filled: root.filled
            iconFamily: root.iconFamily
            size: Metrics.iconSm
            color: root.primary ? Theme.color.textOnAccent : root.ghost ? Theme.color.textSecondary : Theme.color.textPrimary
        }

        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: root.text
            font.pointSize: root.ghost ? Type.small : Type.body
            font.weight: root.primary ? Type.semibold : Type.medium
            color: root.primary ? Theme.color.textOnAccent : root.ghost ? Theme.color.textSecondary : Theme.color.textPrimary
        }
    }

    // Tab reaches it, and Space or Enter presses it, as QBZ's buttons.
    activeFocusOnTab: enabled
    Keys.onPressed: (event) => {
        if (root.enabled && !event.isAutoRepeat && (event.key === Qt.Key_Space || event.key === Qt.Key_Return || event.key === Qt.Key_Enter)) {
            root.clicked()
            event.accepted = true
        }
    }
    FocusRing { onAccent: root.primary }

    MouseArea {
        id: area
        cursorShape: Qt.PointingHandCursor
        anchors.fill: parent
        hoverEnabled: true
        onClicked: root.clicked()
    }
}
