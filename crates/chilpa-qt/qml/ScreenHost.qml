import QtQuick
import app.chilpa
import app.chilpa.sources
import app.chilpa.immersive
import "qrc:/qt/qml/app/chilpa/immersive/qml/Components"
import "Screens"
import "Components"
import "qrc:/qt/qml/app/chilpa/sources/qml/Screens"
import app.chilpa.pages
import "qrc:/qt/qml/app/chilpa/discovery/qml/Screens"
import "qrc:/qt/qml/app/chilpa/metadata/qml/Screens"
import "qrc:/qt/qml/app/chilpa/folders/qml/Screens"
import "qrc:/qt/qml/app/chilpa/folders/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/stats/qml/Screens"
import "qrc:/qt/qml/app/chilpa/playlists/qml/Screens"
import "qrc:/qt/qml/app/chilpa/devices/qml/Screens"
import "qrc:/qt/qml/app/chilpa/offline/qml/Screens"
import "qrc:/qt/qml/app/chilpa/controls/qml/Screens"
import "qrc:/qt/qml/app/chilpa/controls/qml/Components" as Driving
import "qrc:/qt/qml/app/chilpa/playlists/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/kit/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"
// I acknowledge the route before I mount its view (frontend ADR-0014).
PageTransition {
    id: host
    visible: !Immersive.open
    retainedScreen: "folders"
    property bool panelAside: false
    Binding { target: CardLayout; property: "viewportWidth"; value: host.width }
    Connections { target: Navigation; function onRouteRequested() { host.request(true) } }
    Driving.FrameSettlement { navigation: Navigation; router: host }
    onPreparingRoute: {
        if (Navigation.prepare()) Sources.refresh()
        panelAside = Navigation.screen === "metadata" || Navigation.screen === "folderMetadata"
    }
    route: [Navigation.active, Navigation.screen, Navigation.routeId, Navigation.routeKey].join("\u001f")
    Driving.WindowRequests {}
    readonly property var sourceDocument: JSON.parse(Sources.snapshot || "{}")
    readonly property bool medium: ["radio", "podcasts"].indexOf(sourceDocument.medium) >= 0
    readonly property url displayEntry: screen === "source" && routeKey.indexOf(" ") < 0 ? Sources.display(routeKey, Sources.revision) : ""
    readonly property var music: screen === "source" && String(displayEntry) === "" && !(medium && sourceDocument.source === routeKey) ? JSON.parse(NativePages.presentation(routeKey,Library.remoteRev+Sources.revision) || "{}") : ({})

    sourceComponent: {
        if (screen === "source" && String(displayEntry) !== "") return pluginDisplay
        if (screen === "source" && medium && sourceDocument.source === routeKey) return mediaSource
        if (music.music === true) return nativeSource
        switch (screen) {
        case "home": return home
        case "library": return library
        case "album": return album
        case "albums": return albums
        case "tracks": return tracks
        case "artists": return artists
        case "genres": case "composers": case "compositions": return discovery
        case "artist": return artist
        case "artistScene": return artistScene
        case "settings": return settings
        case "pluginDownloads": return pluginDownloads
        case "libraryFolders": return libraryFolders
        case "offlineMusic": return offlineMusic
        case "folders": return folders
        case "openMedia": return openMedia
        case "playlist": return playlist
        case "playlistManager": return playlistManager
        case "playlistImport": return playlistImport
        case "deviceSync": return deviceSync
        case "metadata": return metadata
        case "folderMetadata": return folderMetadata
        case "listenlist": return listenlist
        case "search": return searchResults
        case "stats": return stats
        case "listeningHistory": return listeningHistory
        case "source": return source
        case "sourceEntry": return medium ? mediaDetail : sourceEntry
        default: return pending
        }
    }
    Component { id: home; HomeScreen {} }
    Component { id: pluginDisplay; PluginDisplayScreen { pluginId: host.routeKeyOf(parent); entry: host.displayEntry } }
    Component { id: library; LibraryScreen {} }
    Component { id: album; AlbumScreen { albumId: host.routeIdOf(parent) } }
    Component { id: albums; AlbumsScreen {} }
    Component { id: tracks; TracksScreen {} }
    Component { id: artists; ArtistsScreen {} }
    Component { id: discovery; DiscoveryScreen { mode: (parent?.pageRoute || "").split("\u001f")[1] || "genres"; routeKey: host.routeKeyOf(parent) } }
    // The library's artist is drawn by the same page as a source's.
    Component { id: artist; ArtistPage { key: "local artist:" + host.routeKeyOf(parent) } }
    Component { id: artistScene; ArtistScene {} }
    Component { id: settings; SettingsScreen { routeSection: host.routeKeyOf(parent); purchaseCard: purchaseAlbumCard } }
    Component { id: pluginDownloads; PluginDownloadsScreen { pluginId: host.routeKeyOf(parent); purchaseCard: purchaseAlbumCard } }
    Component { id: purchaseAlbumCard; PurchaseAlbumCard {} }
    Component { id: libraryFolders; LibraryFoldersScreen {} }
    Component { id: offlineMusic; OfflineMusicScreen {} }
    Component { id: folders; FoldersScreen { routeKey: host.routeKeyOf(parent) } }
    Component { id: openMedia; OpenMediaScreen {} }
    Component { id: playlist; PlaylistScreen { playlistId: host.routeIdOf(parent) } }
    Component { id: playlistImport; PlaylistImportScreen {} }
    Component { id: deviceSync; DeviceSyncScreen {} }
    Component { id: playlistManager; PlaylistManagerScreen {} }
    Component { id: listenlist; ListenListScreen {
        visualizerFeed: Visualizer
        visualizationsReady: true
        scopeRenderer: Component { CompactScope {} }
    } }
    Component { id: searchResults; SearchScreen { query: host.routeKeyOf(parent) } }
    Component { id: metadata; MetadataScreen { albumId: host.routeIdOf(parent) } }
    Component { id: folderMetadata; FolderMetadataScreen { routeKey:host.routeKeyOf(parent) } }
    Component { id: stats; StatsScreen {} }
    Component { id: listeningHistory; HistoryScreen {} }
    Component { id: source; SourceScreen { routeKey: host.routeKeyOf(parent) } }
    Component { id: mediaSource; MediaSourceScreen { routeKey: host.routeKeyOf(parent) } }
    Component { id: mediaDetail; MediaDetailScreen { key: host.routeKeyOf(parent) } }
    Component { id: nativeSource; NativeSourceHost { source: host.routeKeyOf(parent); presentation: host.music; scrollMemory: parent?.scroll || null } }
    Component { id: sourceEntry; SourceEntryScreen { key: host.routeKeyOf(parent) } }
    Component { id: pending; PendingScreen {} }

    // The one add-to-playlist picker; the menus only ask for it.
    PlaylistPickerModal {}
    AnalysisOffer {}
}
