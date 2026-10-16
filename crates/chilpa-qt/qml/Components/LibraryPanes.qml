import QtQuick
import QtQuick.Effects
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The Library's filter columns side by side, in the design's proportions,
// with a hairline between them (design/02 9).
Rectangle {
    id: panes

    radius: Metrics.radiusMenu
    color: Theme.ambient ? Theme.color.ambientRaised : Theme.color.surfacePane
    clip: true

    // Each filter column can be turned on or off independently.
    property var shown: ["year", "genre", "artist", "album"]
    readonly property var all: [
        { column: "year", title: I18n.tr("Year", I18n.rev) },
        { column: "genre", title: I18n.tr("Genre", I18n.rev) },
        { column: "label", title: I18n.tr("Label", I18n.rev) },
        { column: "composer", title: I18n.tr("Composer", I18n.rev) },
        { column: "rating", title: I18n.tr("Rating", I18n.rev) },
        { column: "artist", title: I18n.tr("Artist", I18n.rev) },
        { column: "album", title: I18n.tr("Album", I18n.rev) }
    ]
    readonly property var columns: all.filter(c => shown.indexOf(c.column) >= 0)
    readonly property real shares: columns.reduce((sum, c) => sum + Metrics.paneShare[c.column], 0)

    Row {
        id: group
        anchors.fill: parent
        anchors.margins: 1
        spacing: 1
        layer.enabled: GraphicsInfo.api !== GraphicsInfo.Software
        layer.effect: MultiEffect { maskEnabled: true; maskSource: groupMask }

        Repeater {
            model: panes.columns

            PaneColumn {
                required property int index
                required property var modelData
                width: (panes.width - panes.columns.length - 1) * Metrics.paneShare[modelData.column] / panes.shares // derived: the seams between columns and at the edges
                height: parent.height
                column: modelData.column
                title: modelData.title
                Rectangle {
                    anchors.right: parent.right
                    width: 1; height: parent.height
                    visible: index < panes.columns.length - 1
                    color: Theme.color.strokeHairline
                }
            }
        }
    }
    Rectangle {
        id: groupMask
        width: group.width; height: group.height
        radius: panes.radius
        color: Theme.color.accent
        layer.enabled: true
        visible: false
    }
    Edge {}
}
