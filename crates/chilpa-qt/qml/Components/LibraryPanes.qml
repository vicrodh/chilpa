import QtQuick
import com.blitzfc.chilpa
import "../Theme"

// The Library's four filter columns side by side, in the design's
// proportions, with a hairline between them (design/02 9).
Rectangle {
    id: panes

    radius: Metrics.radiusMenu
    color: Theme.color.strokeHairline
    clip: true

    Row {
        anchors.fill: parent
        anchors.margins: 1
        spacing: 1

        Repeater {
            model: [
                { column: "year", title: I18n.tr("Year", I18n.rev) },
                { column: "genre", title: I18n.tr("Genre", I18n.rev) },
                { column: "artist", title: I18n.tr("Artist", I18n.rev) },
                { column: "album", title: I18n.tr("Album", I18n.rev) }
            ]

            PaneColumn {
                required property int index
                required property var modelData
                width: (panes.width - Metrics.paneSeams) * Metrics.paneShares[index] / Metrics.paneShareTotal
                height: parent.height
                column: modelData.column
                title: modelData.title
            }
        }
    }
}
