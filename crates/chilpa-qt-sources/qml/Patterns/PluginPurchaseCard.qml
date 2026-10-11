import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

Rectangle {
    id: card
    required property var modelData
    signal opened()
    radius: Metrics.radiusControl
    color: area.containsMouse ? Theme.color.stateHover : Theme.color.surfacePanel
    border.color: Theme.color.strokeHairline
    Image {
        id: art
        width: parent.width; height: parent.width
        source: card.modelData.image || ""
        asynchronous: true; fillMode: Image.PreserveAspectCrop
        visible: status === Image.Ready
    }
    Text { anchors.centerIn: art; visible: art.status !== Image.Ready; text: "♫"; color: Theme.color.textTertiary; font.pointSize: Type.titleSmall }
    Column {
        anchors.top: art.bottom; anchors.topMargin: Metrics.gap8
        anchors.left: parent.left; anchors.right: parent.right
        Text { width: parent.width; text: card.modelData.title || ""; textFormat: Text.PlainText; elide: Text.ElideRight; color: Theme.color.textPrimary; font.pointSize: Type.body }
        Text { width: parent.width; text: card.modelData.artist || ""; textFormat: Text.PlainText; elide: Text.ElideRight; color: Theme.color.textSecondary; font.pointSize: Type.meta }
    }
    MouseArea { id: area; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: card.opened() }
}
