import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Item {
    id: header
    objectName: "playlistSourceGroup"
    property string sourceName: ""
    property int count: 0
    property bool expanded: true
    signal toggled()
    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: sourceName + " " + count
    Accessible.onPressAction: toggled()
    Keys.onSpacePressed: toggled()
    Keys.onReturnPressed: toggled()
    property string title: ""
    property string sourceIcon: ""
    SourceIcon {
        id: logo
        x: Metrics.navItemPadding; anchors.verticalCenter: parent.verticalCenter
        sourceInfo: ({icon:parent.sourceIcon,label:parent.sourceName})
    }
    Text {
        anchors.left: logo.right; anchors.leftMargin: Metrics.navItemPadding
        anchors.right: countLabel.left; anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        text: parent.title; elide: Text.ElideRight
        font.pointSize: Type.small; font.weight: Type.semibold; color: Theme.color.textTertiary
    }
    Text {
        id: countLabel
        anchors.right: caret.left; anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        text: header.count
        font.pointSize: Type.meta; color: Theme.color.textTertiary
    }
    Icon {
        id: caret
        anchors.right: parent.right; anchors.rightMargin: Metrics.navItemPadding
        anchors.verticalCenter: parent.verticalCenter
        glyph: header.expanded ? Icons.chevronDown : Icons.chevronRight
        size: Metrics.iconXs
        color: Theme.color.textTertiary
    }
    HoverHandler { cursorShape: Qt.PointingHandCursor }
    TapHandler { onTapped: { header.forceActiveFocus(); header.toggled() } }
}
