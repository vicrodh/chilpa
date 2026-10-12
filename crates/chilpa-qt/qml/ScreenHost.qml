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
import "qrc:/qt/qml/app/chilpa/playlists/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"

// I destroy the previous screen when navigating (frontend ADR-0014).
Loader {
    id: host
    ScrollMemory { viewport: host.item && host.item.contentY !== undefined ? host.item : host.item?.scrollView || null; key: [Navigation.active, Navigation.screen, Navigation.routeId, Navigation.routeKey].join("\u001f") }
    Connections { target: OpenMedia; function onOpened() { Navigation.go("openMedia", 0) } }
    // The ratings the screens show follow the library as it changes.
    Connections {
        target: Library
        function onContentRevChanged() { Ratings.refresh() }
        function onRemoteRevChanged() { Ratings.refresh() }
    }
    readonly property var sourceDocument: JSON.parse(Sources.snapshot || "{}")
    readonly property bool medium: ["radio", "podcasts"].indexOf(sourceDocument.medium) >= 0
    readonly property var music: Navigation.screen === "source" ? JSON.parse(Sources.page(Navigation.routeKey, Library.remoteRev) || "{}") : ({})

    sourceComponent: {
        if (music.music === true)
            return music.artist === true ? sourceArtist : music.shelves !== undefined ? sourceHome : music.albumsOnly === false ? sourceListing : sourceAlbums
        switch (Navigation.screen) {
        case "home": return home
        case "library": return library
        case "album": return album
        case "albums": return albums
        case "tracks": return tracks
        case "artists": return artists
        case "artist": return artist
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
        case "source": return medium && sourceDocument.source === Navigation.routeKey ? mediaSource : source
        case "sourceEntry": return medium ? mediaDetail : sourceEntry
        default: return pending
        }
    }

    Component { id: home; HomeScreen {} }
    Component { id: library; LibraryScreen {} }
    Component { id: album; AlbumScreen {} }
    Component { id: albums; AlbumsScreen {} }
    Component { id: tracks; TracksScreen {} }
    Component { id: artists; ArtistsScreen {} }
    // The library's artist is drawn by the same page as a source's.
    Component { id: artist; ArtistPage { key: "local artist:" + Navigation.routeKey } }
    Component { id: settings; SettingsScreen { purchaseCard: purchaseAlbumCard } }
    Component {
        id: pluginDownloads
        Item {
            SettingsPage { id: backend; section: "plugins" }
            PluginManager { anchors.fill: parent; page: backend; initialDownloadsId: Navigation.routeKey; purchaseCard: purchaseAlbumCard }
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
            year: record.year || ""
            art: record.image || ""
            available: record.downloadable !== false
            onClicked: opened()
        }
    }
    Component { id: libraryFolders; LibraryFoldersScreen {} }
    Component { id: folders; FoldersScreen {} }
    Component { id: openMedia; OpenMediaScreen {} }
    Component { id: playlist; PlaylistScreen {} }
    Component { id: playlistImport; PlaylistImportScreen {} }
    Component { id: playlistManager; PlaylistManagerScreen {} }
    Component { id: listenlist; ListenListScreen {} }
    Component { id: metadata; MetadataScreen {} }
    Component { id: stats; StatsScreen {} }
    Component { id: listeningHistory; HistoryScreen {} }
    Component { id: source; SourceScreen {} }
    Component { id: mediaSource; MediaSourceScreen {} }
    Component { id: mediaDetail; MediaDetailScreen {} }
    Component { id: sourceHome; SourceHome { key: Navigation.routeKey } }
    Component { id: sourceArtist; ArtistPage { key: Navigation.routeKey } }
    Component { id: sourceAlbums; AlbumsScreen { source: Navigation.routeKey } }
    Component { id: sourceListing; SourceListing { source: Navigation.routeKey } }
    Component { id: sourceEntry; SourceEntryScreen {} }
    Component { id: pending; PendingScreen {} }

    // The one add-to-playlist picker; the menus only ask for it.
    PlaylistPickerModal {}
    AnalysisOffer {}
}
