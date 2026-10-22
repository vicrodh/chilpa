import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

// The library's artists as Artist Scene draws them: a portrait, the name
// and a row with the icon of each source that holds their records, which
// opens the artist there. The cards keep the scene's size and rhythm.
GridView {
    id: grid
    objectName: "artistCardGrid"
    // An AlbumGridModel of kind "artists": how many there are and each card.
    property var artists: null
    readonly property int columns: Math.max(1, Math.floor((width + Metrics.sceneCardGap) / (Metrics.sceneCardWidth + Metrics.sceneCardGap)))
    model: artists ? artists.albums : 0
    cellWidth: Metrics.sceneCardWidth + Metrics.sceneCardGap
    cellHeight: Metrics.sceneCardHeight + Metrics.sceneRowGap
    cacheBuffer: cellHeight * 2 // derived: two rows of runway, as the scene keeps
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ViewportScrollBar {}

    function openSource(source) {
        if (source.local) Navigation.openArtist(source.key)
        else if (source.key) Navigation.goKey("source", source.key)
    }
    function openArtist(artist) {
        const first = (artist.sources || [])[0]
        if (first) openSource(first)
        else Navigation.openArtist(artist.title || "")
    }

    delegate: Item {
        required property int index
        width: grid.cellWidth
        height: grid.cellHeight
        SceneArtistCard {
            artist: JSON.parse(grid.artists.artist(parent.index, grid.artists.rev) || "{}")
            onActivated: grid.openArtist(artist)
            onSourceActivated: source => grid.openSource(source)
        }
    }
}
