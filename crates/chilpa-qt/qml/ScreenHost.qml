import QtQuick
import app.chilpa
import app.chilpa.sources
import "Screens"
import "Components"
import "qrc:/qt/qml/app/chilpa/sources/qml/Screens"
import "qrc:/qt/qml/app/chilpa/metadata/qml/Screens"
import "qrc:/qt/qml/app/chilpa/folders/qml/Screens"
import "qrc:/qt/qml/app/chilpa/folders/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/stats/qml/Screens"
import "qrc:/qt/qml/app/chilpa/playlists/qml/Screens"
import "qrc:/qt/qml/app/chilpa/controls/qml/Screens"
import "qrc:/qt/qml/app/chilpa/controls/qml/Components" as Driving
import "qrc:/qt/qml/app/chilpa/playlists/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/kit/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"
// I acknowledge the route before I mount its view (frontend ADR-0014).
PageTransition {
    id: host
    Binding { target: CardLayout; property: "viewportWidth"; value: host.width }
    Connections { target: Navigation; function onRouteRequested() { host.request(true) } }
    Driving.FrameSettlement { navigation: Navigation; router: host }
    onPreparingRoute: if (Navigation.prepare()) Sources.refresh()
    route: [Navigation.active, Navigation.screen, Navigation.routeId, Navigation.routeKey].join("\u001f")
    Connections { target: OpenMedia; function onOpened() { Navigation.go("openMedia", 0) } }
    Connections {
        target: Library
        function onContentRevChanged() { Ratings.refresh() }
        function onRemoteRevChanged() { Ratings.refresh() }
    }
    readonly property var sourceDocument: JSON.parse(Sources.snapshot || "{}")
    readonly property bool medium: ["radio", "podcasts"].indexOf(sourceDocument.medium) >= 0
    readonly property var requested: host.componentRoute.split("\u001f")
    readonly property string screen: requested.length > 1 ? requested[1] : ""
    readonly property string routeKey: requested.length > 3 ? requested[3] : ""
    readonly property var music: screen === "source" && !(medium && sourceDocument.source === routeKey) ? JSON.parse(Sources.page(routeKey, Library.remoteRev) || "{}") : ({})

    sourceComponent: {
        if (screen === "source" && medium && sourceDocument.source === routeKey) return mediaSource
        if (music.music === true)
            return music.artist === true || / (native:artist:|matching-artist-v[23]:)/.test(routeKey) ? sourceArtist : music.shelves !== undefined ? sourceHome : music.albumsOnly === false ? sourceListing : sourceAlbums
        switch (screen) {
        case "home": return home
        case "library": return library
        case "album": return album
        case "albums": return albums
        case "tracks": return tracks
        case "artists": return artists
        case "artist": return artist
        case "artistScene": return artistScene
        case "settings": return settings
        case "pluginDownloads": return pluginDownloads
        case "libraryFolders": return libraryFolders
        case "folders": return folders
        case "openMedia": return openMedia
        case "playlist": return playlist
        case "playlistManager": return playlistManager
        case "playlistImport": return playlistImport
        case "metadata": return metadata
        case "listenlist": return listenlist
        case "stats": return stats
        case "listeningHistory": return listeningHistory
        case "source": return source
        case "sourceEntry": return medium ? mediaDetail : sourceEntry
        default: return pending
        }
    }
    Component { id: home; HomeScreen {} }
    Component { id: library; LibraryScreen {} }
    Component { id: album; AlbumScreen { albumId: host.routeIdOf(parent) } }
    Component { id: albums; AlbumsScreen {} }
    Component { id: tracks; TracksScreen {} }
    Component { id: artists; ArtistsScreen {} }
    // The library's artist is drawn by the same page as a source's.
    Component { id: artist; ArtistPage { key: "local artist:" + host.routeKeyOf(parent) } }
    Component { id: artistScene; ArtistScene {} }
    Component { id: settings; SettingsScreen { routeSection: host.routeKeyOf(parent); purchaseCard: purchaseAlbumCard } }
    Component {
        id: pluginDownloads
        Item {
            readonly property string pluginId: host.routeKeyOf(parent)
            SettingsPage { id: backend; section: "plugins" }
            PluginManager { anchors.fill: parent; page: backend; initialDownloadsId: parent.pluginId; purchaseCard: purchaseAlbumCard }
        }
    }
    Component {
        id: purchaseAlbumCard
        AlbumCard {
            property var record: ({})
            signal opened()
            purchaseMode: true
            title: record.title || ""
            subtitle: record.artist || ""
            artistDestination: record.artist && record.plugin ? ({target:"source",key:record.plugin + " " + (record.artist_ref ? "native:" + record.artist_ref : "matching-artist-v3:" + record.artist)}) : ({})
            year: record.year || ""
            art: record.image || ""
            available: record.downloadable !== false
            onClicked: opened()
        }
    }
    Component { id: libraryFolders; LibraryFoldersScreen {} }
    Component { id: folders; FoldersScreen { routeKey: host.routeKeyOf(parent) } }
    Component { id: openMedia; OpenMediaScreen {} }
    Component { id: playlist; PlaylistScreen { playlistId: host.routeIdOf(parent) } }
    Component { id: playlistImport; PlaylistImportScreen {} }
    Component { id: playlistManager; PlaylistManagerScreen {} }
    Component { id: listenlist; ListenListScreen {} }
    Component { id: metadata; MetadataScreen { albumId: host.routeIdOf(parent) } }
    Component { id: stats; StatsScreen {} }
    Component { id: listeningHistory; HistoryScreen {} }
    Component { id: source; SourceScreen { routeKey: host.routeKeyOf(parent) } }
    Component { id: mediaSource; MediaSourceScreen { routeKey: host.routeKeyOf(parent) } }
    Component { id: mediaDetail; MediaDetailScreen { key: host.routeKeyOf(parent) } }
    Component { id: sourceHome; SourceHome { key: host.routeKeyOf(parent) } }
    Component { id: sourceArtist; ArtistPage { key: host.routeKeyOf(parent) } }
    Component { id: sourceAlbums; AlbumsScreen { source: host.routeKeyOf(parent) } }
    Component { id: sourceListing; SourceListing { source: host.routeKeyOf(parent) } }
    Component { id: sourceEntry; SourceEntryScreen { key: host.routeKeyOf(parent) } }
    Component { id: pending; PendingScreen {} }

    // The one add-to-playlist picker; the menus only ask for it.
    PlaylistPickerModal {}
    AnalysisOffer {}
}
