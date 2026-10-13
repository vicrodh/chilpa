import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Item {
    objectName: "playlistSourceGroup"
    property string sourceName: ""
    Accessible.role: Accessible.Heading
    Accessible.name: sourceName + " " + title
    property string title: ""
    property string sourceIcon: ""
    SourceIcon {
        id: logo
        x: Metrics.navItemPadding; anchors.verticalCenter: parent.verticalCenter
        sourceInfo: ({icon:parent.sourceIcon,label:parent.sourceName})
    }
    Text {
        anchors.left: logo.right; anchors.leftMargin: Metrics.navItemPadding
        anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
        text: parent.title; elide: Text.ElideRight
        font.pointSize: Type.small; font.weight: Type.semibold; color: Theme.color.textTertiary
    }
}
