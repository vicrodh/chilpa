import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Rectangle {
    id: row
    required property var destination
    signal chosen()
    signal moveFocus(int delta)
    objectName: "linkChoice"
    height: Metrics.settingsRow + Metrics.gap16
    radius: Metrics.radiusControl
    color: pointer.containsMouse ? Theme.color.stateHover : Theme.color.surfacePanel
    activeFocusOnTab: true
    border.width: activeFocus ? Metrics.focusRing : 0
    border.color: Theme.color.accent
    Accessible.role: Accessible.Button
    Accessible.name: [destination.source, destination.title, destination.artist].filter(Boolean).join(", ")
    Accessible.onPressAction: chosen()
    Keys.onPressed: (event) => {
        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
            chosen()
            event.accepted = true
        } else if (event.key === Qt.Key_Down || event.key === Qt.Key_Up) {
            moveFocus(event.key === Qt.Key_Down ? 1 : -1)
            event.accepted = true
        }
    }
    Row {
        anchors.fill: parent
        anchors.margins: Metrics.gap12
        spacing: Metrics.gap12
        Icon {
            anchors.verticalCenter: parent.verticalCenter
            glyph: row.destination.local ? Lucide.libraryBig : Lucide.audioLines
            iconFamily: Lucide.font
            size: Metrics.iconNav
            color: Theme.color.accent
        }
        Column {
            width: parent.width - Metrics.iconNav * 2 - Metrics.gap12 * 2 // derived: source icon, arrow and their gaps
            anchors.verticalCenter: parent.verticalCenter
            spacing: Metrics.gap4
            Text {
                width: parent.width
                text: row.destination.source || ""
                color: Theme.color.textPrimary
                font.pointSize: Type.body
                font.weight: Type.bold
                elide: Text.ElideRight
            }
            Text {
                width: parent.width
                text: (row.destination.title || "") + (row.destination.artist ? " · " + row.destination.artist : "")
                color: Theme.color.textSecondary
                font.pointSize: Type.small
                elide: Text.ElideRight
            }
        }
        Icon {
            anchors.verticalCenter: parent.verticalCenter
            glyph: Lucide.chevronRight
            iconFamily: Lucide.font
            size: Metrics.iconNav
            color: pointer.containsMouse ? Theme.color.accent : Theme.color.textTertiary
        }
    }
    MouseArea {
        id: pointer
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: { row.forceActiveFocus(Qt.MouseFocusReason); row.chosen() }
    }
}
