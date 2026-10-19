import QtQuick
import "../Theme"

// A button with text: "primary" in the accent, "secondary" on a field, or
// "ghost": no fill until the pointer is over it, 28 high, as a panel's
// footer has them (design/03 QueueFooter). "backdrop" is "secondary" on a
// dark backdrop, the album header's: white ink on a faint white field in
// every theme, as QBZ switches its header actions off the theme there
// (`AlbumView.qml:304`, `hdrOverlay`).
Rectangle {
    id: root

    property string text
    property string glyph
    property string iconFamily
    // A glyph from the filled font: play, pause, a filled heart.
    property bool filled: false
    // "primary", "secondary", "ghost" or "backdrop".
    property string kind: "secondary"

    signal clicked()

    readonly property bool primary: kind === "primary"
    readonly property bool ghost: kind === "ghost"
    readonly property bool backdrop: kind === "backdrop"
    readonly property color ink: primary ? Theme.color.textOnAccent : backdrop ? Theme.color.backdropText
        : ghost ? Theme.color.textSecondary : Theme.color.textPrimary

    implicitWidth: row.implicitWidth + (ghost ? Metrics.gap8 : Metrics.pillPadding) * 2 // derived: padding on both sides
    implicitHeight: ghost ? Metrics.button28 : Metrics.pillHeight
    opacity: enabled ? 1 : Metrics.disabledOpacity
    radius: ghost ? Metrics.radiusControl : height / 2 // derived: a pill
    color: ghost ? (area.containsMouse ? Theme.color.stateHover : "transparent")
         : backdrop ? (area.containsMouse ? Theme.color.backdropFillHover : Theme.color.backdropFill)
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
            color: root.ink
        }

        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: root.text
            font.pointSize: root.ghost ? Type.small : Type.body
            font.weight: root.primary ? Type.semibold : Type.medium
            color: root.ink
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
