import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// The Library's filter columns side by side, in the design's proportions,
// with a hairline between them (design/02 9).
Rectangle {
    id: panes

    radius: Metrics.radiusMenu
    color: Theme.color.strokeHairline
    clip: true

    // The columns asked for: year, genre, label and composer can be turned
    // on and off; artist and album are always there.
    property var shown: ["year", "genre"]
    readonly property var all: [
        { column: "year", title: I18n.tr("Year", I18n.rev) },
        { column: "genre", title: I18n.tr("Genre", I18n.rev) },
        { column: "label", title: I18n.tr("Label", I18n.rev) },
        { column: "composer", title: I18n.tr("Composer", I18n.rev) },
        { column: "artist", title: I18n.tr("Artist", I18n.rev) },
        { column: "album", title: I18n.tr("Album", I18n.rev) }
    ]
    readonly property var columns: all.filter(c => c.column === "artist" || c.column === "album" || shown.indexOf(c.column) >= 0)
    readonly property real shares: columns.reduce((sum, c) => sum + Metrics.paneShare[c.column], 0)

    Row {
        anchors.fill: parent
        anchors.margins: 1
        spacing: 1

        Repeater {
            model: panes.columns

            PaneColumn {
                required property var modelData
                width: (panes.width - panes.columns.length - 1) * Metrics.paneShare[modelData.column] / panes.shares // derived: the seams between columns and at the edges
                height: parent.height
                column: modelData.column
                title: modelData.title
            }
        }
    }
}
