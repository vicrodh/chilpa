import QtQuick
import "../Theme"

// The head of a side panel: its title, a detail next to it and its buttons
// on the right (design/03, 40 px).
Item {
    id: head

    property string title
    property string detail
    default property alias buttons: tools.data

    implicitHeight: Metrics.panelHeader

    Row {
        anchors.left: parent.left
        anchors.leftMargin: Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap8

        Text { text: head.title; font.pointSize: Type.body; font.weight: Type.heading; color: Theme.color.textPrimary }
        Text { text: head.detail; font.pointSize: Type.small; color: Theme.color.textTertiary; anchors.baseline: parent.children[0].baseline }
    }

    Row {
        id: tools
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap6
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap2
    }

    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: Theme.color.strokeHairline
    }
}
