import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The address of an upload in a bar, or why it failed: shown in both views,
// because Upload lives in the footer and its result has to be reachable there.
Rectangle {
    id: bar
    property string url
    property string error
    signal copyAsked()
    height: Metrics.button32
    visible: url !== "" || error !== ""
    radius: Metrics.radiusControl
    border.width: Metrics.hairline
    border.color: Theme.color.strokeHairline
    color: Theme.color.fillField
    Icon {
        id: linkGlyph
        anchors.left: parent.left
        anchors.leftMargin: Metrics.gap10
        anchors.verticalCenter: parent.verticalCenter
        visible: bar.error === ""
        glyph: Lucide.link
        iconFamily: Lucide.font
        size: Metrics.iconSm
        color: Theme.color.accent
    }
    // Read only but selectable: the point is to take the address.
    TextInput {
        anchors.left: linkGlyph.right
        anchors.leftMargin: Metrics.gap8
        anchors.right: copy.left
        anchors.rightMargin: Metrics.gap4
        anchors.verticalCenter: parent.verticalCenter
        visible: bar.error === ""
        text: bar.url
        readOnly: true
        selectByMouse: true
        color: Theme.color.textSecondary
        font.pointSize: Type.meta
        clip: true
    }
    Text {
        objectName: "developerUploadError"
        anchors.left: parent.left
        anchors.leftMargin: Metrics.gap10
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap10
        anchors.verticalCenter: parent.verticalCenter
        visible: bar.error !== ""
        text: bar.error
        color: Theme.color.statusNegative
        font.pointSize: Type.meta
        elide: Text.ElideRight
    }
    Item {
        id: copy
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap4
        anchors.verticalCenter: parent.verticalCenter
        visible: bar.error === ""
        width: Metrics.button26
        height: Metrics.button26
        Icon { anchors.centerIn: parent; glyph: Icons.copy; size: Metrics.iconSm; color: copyArea.containsMouse ? Theme.color.textPrimary : Theme.color.textTertiary }
        MouseArea { id: copyArea; objectName: "developerCopyUrl"; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: bar.copyAsked() }
    }
}
