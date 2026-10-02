import QtQuick
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "Screens"
import "qrc:/qt/qml/com/blitzfc/chilpa/sources/qml/Screens"

// The active tab's screen. A synchronous Loader: the screen that leaves is
// destroyed, not hidden, and the one that arrives is built in one go
// (frontend ADR-0014, rules 7 and 8). A source the catalog draws opens on
// the library's screens: its own page, its records on Albums, a folder.
Loader {
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
        case "playlist": return playlist
        case "playlistManager": return playlistManager
        case "metadata": return metadata
        case "listenlist": return listenlist
        case "source": return source
        case "sourceEntry": return sourceEntry
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
    Component { id: playlist; PlaylistScreen {} }
    Component { id: playlistManager; PlaylistManagerScreen {} }
    Component { id: listenlist; ListenListScreen {} }
    Component { id: metadata; MetadataScreen {} }
    Component { id: source; SourceScreen {} }
    Component { id: sourceHome; SourceHome { key: Navigation.routeKey } }
    Component { id: sourceAlbums; AlbumsScreen { source: Navigation.routeKey } }
    Component { id: sourceListing; SourceListing { source: Navigation.routeKey } }
    Component { id: sourceEntry; SourceEntryScreen {} }
    Component { id: pending; PendingScreen {} }
}
