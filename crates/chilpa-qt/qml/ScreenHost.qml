import QtQuick
import com.blitzfc.chilpa
import "Screens"

// The active tab's screen. A synchronous Loader: the screen that leaves is
// destroyed, not hidden, and the one that arrives is built in one go
// (frontend ADR-0014, rules 7 and 8).
Loader {
    sourceComponent: {
        switch (Navigation.screen) {
        case "home": return home
        case "library": return library
        case "album": return album
        case "albums": return albums
        case "artists": return artists
        case "artist": return artist
        case "settings": return settings
        case "libraryFolders": return libraryFolders
        case "folders": return folders
        case "playlist": return playlist
        case "playlistManager": return playlistManager
        default: return pending
        }
    }

    Component { id: home; HomeScreen {} }
    Component { id: library; LibraryScreen {} }
    Component { id: album; AlbumScreen {} }
    Component { id: albums; AlbumsScreen {} }
    Component { id: artists; ArtistsScreen {} }
    Component { id: artist; ArtistScreen {} }
    Component { id: settings; SettingsScreen {} }
    Component { id: libraryFolders; LibraryFoldersScreen {} }
    Component { id: folders; FoldersScreen {} }
    Component { id: playlist; PlaylistScreen {} }
    Component { id: playlistManager; PlaylistManagerScreen {} }
    Component { id: pending; PendingScreen {} }
}
