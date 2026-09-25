import QtQuick
import "../Theme"

// Exclusive options as icons in a field (design/03 SegmentedControl).
// `options` is a list of { glyph, tip }.
Rectangle {
    id: root

    property var options: []
    property int current: 0

    signal activated(int index)

    implicitWidth: row.implicitWidth + Metrics.gap2 * 2 // derived: padding on both sides
    implicitHeight: Metrics.segmentHeight + Metrics.gap2 * 2 // derived: padding above and below
    radius: Metrics.radiusCard
    color: Theme.color.fillField

    Row {
        id: row
        anchors.centerIn: parent
        spacing: Metrics.gap2

        Repeater {
            model: root.options

            Rectangle {
                id: segment
                required property int index
                required property var modelData
                readonly property bool active: index === root.current

                width: Metrics.segmentWidth
                height: Metrics.segmentHeight
                radius: Metrics.radiusItem
                color: active ? Theme.color.surfaceRaised : "transparent"
                border.color: active ? Theme.color.strokeHairline : "transparent"

                Icon {
                    anchors.centerIn: parent
                    glyph: segment.modelData.glyph
                    size: Metrics.iconMd
                    color: segment.active ? Theme.color.textPrimary : Theme.color.textSecondary
                }

                MouseArea {
                    anchors.fill: parent
                    hoverEnabled: true
                    onClicked: root.activated(segment.index)
                    onContainsMouseChanged: containsMouse ? TipState.show(segment.modelData.tip, segment) : TipState.hide(segment)
                }
            }
        }
    }
}
