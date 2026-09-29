import QtQuick
import "../Theme"

// A source's badge. Its icon is the name of a glyph, which takes `color` like
// any text, so a theme or a color that moves tints it. An adapter that brings
// a drawing of its own names a file instead, and that one keeps its colors.
Item {
    id: badge
    property var sourceInfo: ({})
    property color color: Theme.color.textSecondary
    readonly property string named: badge.sourceInfo.icon || ""
    readonly property bool drawing: named.indexOf("/") >= 0
    readonly property string own: drawing ? "" : SourceGlyphs.of(named)
    implicitWidth: Metrics.iconSm
    implicitHeight: Metrics.iconSm

    Image {
        id: image
        anchors.fill: parent
        source: badge.drawing ? badge.named : ""
        visible: status === Image.Ready
        sourceSize.width: width
        sourceSize.height: height
        fillMode: Image.PreserveAspectFit
        asynchronous: true
    }
    Icon {
        anchors.centerIn: parent
        visible: !image.visible
        glyph: badge.own !== "" ? badge.own : badge.sourceInfo.local ? SourceGlyphs.folderOpen : Icons.server
        iconFamily: badge.own !== "" || badge.sourceInfo.local ? SourceGlyphs.font : ""
        size: badge.width
        color: badge.color
    }
}
