import QtQuick
import "../Theme"

// An adapter supplies its own icon; an unconfigured source still has a badge.
Item {
    id: badge
    property var sourceInfo: ({})
    property color color: Theme.color.textSecondary
    implicitWidth: Metrics.iconSm
    implicitHeight: Metrics.iconSm

    Image {
        id: image
        anchors.fill: parent
        source: badge.sourceInfo.icon || ""
        sourceSize.width: width
        sourceSize.height: height
        fillMode: Image.PreserveAspectFit
        asynchronous: true
    }
    Icon {
        anchors.centerIn: parent
        visible: image.status !== Image.Ready
        glyph: badge.sourceInfo.local ? Icons.folder : Icons.server
        size: badge.width
        color: badge.color
    }
}
