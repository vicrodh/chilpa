import QtQuick
import QtQuick.Controls.impl as ControlsImpl
import "../Theme"

// Neutral vectors follow the text palette; colored artwork keeps its colors.
// ColorImage preserves the rasterized alpha, without a threshold mask pass.
Item {
    id: badge
    property var sourceInfo: ({})
    property color color: Theme.color.textSecondary
    readonly property string named: badge.sourceInfo.icon || ""
    readonly property bool drawing: named.indexOf("/") >= 0
    readonly property string own: drawing ? "" : SourceGlyphs.of(named)
    implicitWidth: Metrics.iconSm
    implicitHeight: Metrics.iconSm

    ControlsImpl.ColorImage {
        id: image; objectName: "sourceImage"
        anchors.fill: parent
        color: badge.named.indexOf("monochrome=1") >= 0 ? badge.color : "transparent"
        source: badge.drawing ? badge.named : ""
        visible: status === Image.Ready
        sourceSize.width: width; sourceSize.height: height
        fillMode: Image.PreserveAspectFit; asynchronous: true
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
