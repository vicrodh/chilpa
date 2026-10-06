import QtQuick
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "Screens"
import "qrc:/qt/qml/com/blitzfc/chilpa/sources/qml/Screens"
import "qrc:/qt/qml/com/blitzfc/chilpa/metadata/qml/Screens"

// I destroy the previous screen when navigating (frontend ADR-0014).
Loader {
    Connections { target: OpenMedia; function onOpened() { Navigation.go("openMedia", 0) } }
    readonly property var sourceDocument: JSON.parse(Sources.snapshot || "{}")
    readonly property bool medium: ["radio", "podcasts"].indexOf(sourceDocument.medium) >= 0
    readonly property var music: Navigation.screen === "source" ? JSON.parse(Sources.page(Navigation.routeKey, Library.remoteRev) || "{}") : ({})

    sourceComponent: {
        if (music.music === true)
            return music.shelves !== undefined ? sourceHome : music.albumsOnly === false ? sourceListing : sourceAlbums
        switch (Navigation.screen) {
        case "home": return home
        case "library": return library
        case "album": return album
        case "albums": return albums
        case "tracks": return tracks
        case "artists": return artists
        case "artist": return artist
        case "settings": return settings
        case "libraryFolders": return libraryFolders
        case "folders": return folders
        case "openMedia": return openMedia
        case "playlist": return playlist
        case "playlistManager": return playlistManager
        case "playlistImport": return playlistImport
        case "metadata": return metadata
        case "listenlist": return listenlist
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
    Component { id: artist; ArtistScreen {} }
    Component { id: settings; SettingsScreen {} }
    Component { id: libraryFolders; LibraryFoldersScreen {} }
    Component { id: folders; FoldersScreen {} }
    Component { id: openMedia; OpenMediaScreen {} }
    Component { id: playlist; PlaylistScreen {} }
    Component { id: playlistImport; PlaylistImportScreen {} }
    Component { id: playlistManager; PlaylistManagerScreen {} }
    Component { id: listenlist; ListenListScreen {} }
    Component { id: metadata; MetadataScreen {} }
    Component { id: source; SourceScreen {} }
    Component { id: mediaSource; MediaSourceScreen {} }
    Component { id: mediaDetail; MediaDetailScreen {} }
    Component { id: sourceHome; SourceHome { key: Navigation.routeKey } }
    Component { id: sourceAlbums; AlbumsScreen { source: Navigation.routeKey } }
    Component { id: sourceListing; SourceListing { source: Navigation.routeKey } }
    Component { id: sourceEntry; SourceEntryScreen {} }
    Component { id: pending; PendingScreen {} }
}
