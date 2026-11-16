import QtQuick
import app.chilpa
import "../Theme"

Column {
    id: credits
    property double albumId: 0
    property var record: null
    property bool contrasted: false
    property bool showGenres: true
    property color color: Theme.color.textPrimary
    signal genresRequested()
    spacing: Metrics.gap6
    AlbumArtistLine {
        width: parent.width; albumId: credits.albumId; contrasted: credits.contrasted; color: credits.color
        document: credits.record && credits.record.artistInformation !== undefined ? JSON.parse(credits.record.artistInformation || "{}")
            : credits.albumId ? JSON.parse(AlbumActions.artistInformation(credits.albumId, Library.contentRev + Library.remoteRev) || "{}") : ({})
    }
    AlbumGenreLine {
        width: parent.width; contrasted: credits.contrasted
        document: credits.showGenres && credits.record ? JSON.parse(credits.record.genreLinks || "{}") : ({})
        onMoreRequested: credits.genresRequested()
    }
}
