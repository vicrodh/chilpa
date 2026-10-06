// hot-path: one per entry of a filter column, recycled while it scrolls.
import QtQuick
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"

// An entry of a Library filter column: its label, and its count or year.
Item {
    id: row

    required property string key
    required property string label
    required property string meta
    required property bool selected
    required property bool all

    signal chosen(int modifiers)

    height: Metrics.paneRow

    Rectangle {
        anchors.fill: parent
        anchors.leftMargin: Metrics.gap4
        anchors.rightMargin: Metrics.gap4
        radius: Metrics.radiusRow
        color: row.selected ? Theme.color.stateSelected : hover.hovered ? Theme.color.stateHover : "transparent"
    }

    Text {
        anchors.left: parent.left
        anchors.leftMargin: Metrics.gap12
        anchors.right: meta.left
        anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        text: row.label
        elide: Text.ElideRight
        font.pointSize: Type.row
        font.weight: row.selected ? Type.semibold : Type.regular
        color: Theme.color.textPrimary
    }

    Text {
        id: meta
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter
        text: row.meta
        font.pointSize: Type.label
        font.features: { "tnum": 1 }
        color: Theme.color.textTertiary
    }

    HoverHandler { id: hover }
    HoverHandler { cursorShape: Qt.PointingHandCursor }
    TapHandler {
        id: tap
        onTapped: {
            const held = tap.point.modifiers
            row.chosen((held & (Qt.ControlModifier | Qt.MetaModifier) ? 1 : 0) | (held & Qt.ShiftModifier ? 2 : 0))
        }
    }
}
