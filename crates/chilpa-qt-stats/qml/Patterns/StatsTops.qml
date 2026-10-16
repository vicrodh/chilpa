import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// The period's artists, records and tracks, a tile each, three across
// while they fit and fewer when they do not.
Flow {
    id: tops

    property var report: ({})
    readonly property int across: Math.max(1, Math.min(3, Math.floor((width + spacing) / (Charts.tileMin + spacing))))
    readonly property real tile: (width - (across - 1) * spacing) / across

    spacing: Metrics.gap16

    TopTile {
        width: tops.tile
        height: Charts.tileTall
        kind: "artists"
        title: I18n.tr("Artists", I18n.rev)
        numbers: tops.report.artists || ({})
        tint: Theme.color.statsArtists
    }
    TopTile {
        width: tops.tile
        height: Charts.tileTall
        kind: "albums"
        title: I18n.tr("Albums", I18n.rev)
        numbers: tops.report.albums || ({})
        tint: Theme.color.statsAlbums
    }
    TopTile {
        width: tops.tile
        height: Charts.tileTall
        kind: "tracks"
        title: I18n.tr("Tracks", I18n.rev)
        numbers: tops.report.tracks || ({})
        tint: Theme.color.statsTracks
    }
}
