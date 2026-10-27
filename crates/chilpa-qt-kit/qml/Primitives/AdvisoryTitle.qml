import QtQuick
import "../Theme"

Item {
    id: line
    property bool explicitContent: false
    property alias text: title.text
    property alias font: title.font
    property alias color: title.color
    property alias elide: title.elide
    property alias wrapMode: title.wrapMode
    property alias maximumLineCount: title.maximumLineCount
    property int horizontalAlignment: Text.AlignLeft
    readonly property alias truncated: title.truncated
    readonly property real advisoryWidth: badge.active ? badge.width + Metrics.gap6 : 0
    baselineOffset: title.baselineOffset
    implicitWidth: title.implicitWidth + (badge.active ? badge.width + Metrics.gap6 : 0)
    implicitHeight: Math.max(title.implicitHeight, badge.height)
    Text {
        id: title
        objectName: "advisoryText"
        width: Math.max(0, Math.min(implicitWidth, line.width - line.advisoryWidth))
        x: line.horizontalAlignment === Text.AlignHCenter ? (line.width - width - line.advisoryWidth) / 2
            : line.horizontalAlignment === Text.AlignRight ? line.width - width - line.advisoryWidth : 0
        horizontalAlignment: line.horizontalAlignment
        textFormat: Text.PlainText
        color: Theme.color.textPrimary
        elide: Text.ElideRight
    }
    Loader {
        id: badge
        active: line.explicitContent
        x: title.x + Math.min(title.paintedWidth, title.width) + Metrics.gap6
        anchors.verticalCenter: parent.verticalCenter
        sourceComponent: ExplicitBadge {}
    }
}
