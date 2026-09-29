import QtQuick
import "../Theme"

// A row of colors to pick one from (design/03, the accent's swatches): each
// a circle, the chosen one ringed. `colors` are what is drawn -- an empty one
// is the theme's own accent -- and `values` what is picked, in that order.
Row {
    id: swatches

    property var colors: []
    property var values: []
    property string value

    signal picked(string value)

    spacing: Metrics.gap8

    Repeater {
        model: swatches.colors

        Item {
            id: swatch
            required property int index
            required property var modelData
            readonly property bool chosen: swatches.values[index] === swatches.value

            width: Metrics.button26
            height: Metrics.button26

            Rectangle {
                anchors.fill: parent
                radius: width / 2 // derived: a circle
                color: "transparent"
                border.width: swatch.chosen ? Metrics.focusRing : 0
                border.color: Theme.color.textPrimary
            }
            Rectangle {
                anchors.centerIn: parent
                width: parent.width - Metrics.gap6
                height: width
                radius: width / 2 // derived: a circle
                color: swatch.modelData === "" ? Theme.color.accentBase : swatch.modelData
            }
            HoverHandler { cursorShape: Qt.PointingHandCursor }
            TapHandler { gesturePolicy: TapHandler.ReleaseWithinBounds; onTapped: swatches.picked(swatches.values[swatch.index]) }
        }
    }
}
