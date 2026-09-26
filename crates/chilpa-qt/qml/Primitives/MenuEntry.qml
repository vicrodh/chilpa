import QtQuick
import "../Theme"

// One line of a Menu: a check, the text with its detail below, a mark and
// a hint on the right; a separator; or a note, a heading nothing happens on.
Item {
    id: line

    property var entry: ({})

    signal clicked()

    readonly property bool separator: entry.separator === true
    readonly property bool note: entry.note === true

    implicitWidth: row.implicitWidth + Metrics.gap10 * 2 // derived: padding on both sides
    implicitHeight: separator ? Metrics.gap8 + 1 : entry.detail ? Metrics.menuItemTall : Metrics.menuItem

    Rectangle {
        visible: line.separator
        anchors.verticalCenter: parent.verticalCenter
        x: Metrics.gap6
        width: parent.width - Metrics.gap12
        height: 1
        color: Theme.color.strokeHairline
    }

    Rectangle {
        visible: !line.separator
        anchors.fill: parent
        radius: Metrics.radiusItem
        color: hover.hovered && !line.note ? Theme.color.stateHover : "transparent"
    }

    Row {
        id: row
        visible: !line.separator
        x: Metrics.gap10
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap8

        Icon {
            anchors.verticalCenter: parent.verticalCenter
            glyph: line.entry.checked ? Icons.check : ""
            size: Metrics.iconMd
            color: Theme.color.accent
        }
        Column {
            anchors.verticalCenter: parent.verticalCenter
            Text {
                text: line.entry.text || ""
                font.pointSize: line.note ? Type.small : Type.body
                font.weight: line.note ? Type.semibold : Type.regular
                color: line.note ? Theme.color.textTertiary : Theme.color.textPrimary
            }
            Text {
                visible: !!line.entry.detail
                text: line.entry.detail || ""
                font.pointSize: Type.small
                color: Theme.color.textSecondary
            }
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            visible: !!line.entry.mark
            text: line.entry.mark || ""
            font.pointSize: Type.micro
            font.weight: Type.bold
            color: Theme.color.accent
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            visible: !!line.entry.hint
            leftPadding: Metrics.gap16
            text: line.entry.hint || ""
            font.pointSize: Type.label
            color: Theme.color.textTertiary
        }
    }

    HoverHandler { id: hover; enabled: !line.separator }
    TapHandler { enabled: !line.separator && !line.note; onTapped: line.clicked() }
}
