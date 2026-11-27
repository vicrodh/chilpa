import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// A card of the stats page: the panel's fill and hairline, a title and a
// note on its right, or in its place a link to where the card goes on,
// and what it holds under them.
Rectangle {
    id: card

    property string title
    property string note
    property string action
    default property alias content: body.data
    readonly property real bodyHeight: body.height

    signal acted()

    radius: Metrics.radiusPanel
    color: Theme.ambient ? Theme.color.ambientPanel : Theme.color.surfacePanel
    border.width: 1
    border.color: Theme.color.strokeHairline

    Text {
        id: head
        x: Metrics.gap16
        y: Metrics.gap16
        width: card.width - 2 * Metrics.gap16 - tail.implicitWidth // derived: inside the padding, beside the note
        visible: card.title !== ""
        text: card.title
        elide: Text.ElideRight
        font.pointSize: Type.section
        font.weight: Type.semibold
        color: Theme.color.textPrimary
    }
    Text {
        id: tail
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap16
        anchors.baseline: head.baseline
        text: card.action !== "" ? card.action : card.note
        font.pointSize: Type.small
        color: card.action === "" ? Theme.color.textTertiary : link.containsMouse ? Theme.color.textPrimary : Theme.color.textSecondary

        MouseArea {
            id: link
            objectName: "cardAction"
            anchors.fill: parent
            enabled: card.action !== ""
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: card.acted()
        }
    }
    Item {
        id: body
        x: Metrics.gap16
        y: head.visible ? head.y + head.height + Metrics.gap12 : Metrics.gap16
        width: card.width - 2 * Metrics.gap16 // derived: inside the padding
        height: card.height - y - Metrics.gap16
    }
}
