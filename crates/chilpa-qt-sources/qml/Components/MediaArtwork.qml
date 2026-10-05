import QtQuick
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Theme"

Rectangle {
    id: tile
    property string title: ""
    property string source: ""
    property bool station: false
    radius: Metrics.radiusCard; color: Theme.color.stateSelected; clip: true
    Text {
        anchors.centerIn: parent; visible: picture.status !== Image.Ready
        text: tile.title.split(/\s+/).filter(Boolean).slice(0, 2).map(v => v[0]).join("").toUpperCase()
        font.pointSize: Type.section; font.weight: Type.semibold; color: Theme.color.textSecondary
    }
    Image {
        id: picture; anchors.centerIn: parent
        width: tile.station ? Math.max(0, Math.min(parent.width - Metrics.gap24, sourceSize.width)) : parent.width
        height: tile.station ? Math.max(0, Math.min(parent.height - Metrics.gap24, sourceSize.height)) : parent.height
        visible: status === Image.Ready; source: tile.source; fillMode: Image.PreserveAspectFit; asynchronous: true
    }
}
