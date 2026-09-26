import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Components"

GallerySection {
    id: section
    title: "Components"

    TrackListModel { id: most; kind: "mostPlayed"; limit: 3; playingPath: Player.path }
    AlbumGridModel { id: added; kind: "recentlyAdded"; limit: 6; maxRows: 1; columns: grid.columns }
    AlbumDetailModel { id: detail; albumId: added.albumId(0, added.rev) }

    Column {
        width: section.width
        spacing: Metrics.gap12

        Row {
            spacing: Metrics.gap12
            NavItem { width: Metrics.sidebarWidth; text: "Library"; glyph: Icons.columns3 }
            NavItem { width: Metrics.sidebarWidth; text: "Active"; glyph: Icons.home; active: true }
            NavItem { width: Metrics.sidebarCollapsed; text: "Collapsed"; glyph: Icons.disc; collapsed: true }
        }
        Row {
            spacing: Metrics.gap12
            TabButton { title: "Home"; glyph: Icons.home; active: true }
            TabButton { title: "Library"; glyph: Icons.columns3 }
            FilterCrumb { name: "Genre"; value: "Alternative" }
            QualityIcon { tier: 1 }
            QualityIcon { tier: 2 }
            QualityIcon { tier: 3 }
            PlayingIndicator {}
        }
        SectionHeader { width: parent.width; title: "Section header"; link: "See all" }
        LibraryCrumbs { width: parent.width; height: Metrics.breadcrumb }
        LibraryPanes { width: parent.width; height: Metrics.panesHeight }
        LibraryToolbar { width: parent.width; tracks: most }
        SortButton { value: "artist"; options: [{ value: "artist", text: "Artist" }, { value: "year", text: "Year" }] }
        TrackList { width: parent.width; height: implicitHeight; fitContent: true; model: most }
        TrackList { width: parent.width; height: implicitHeight; fitContent: true; rich: true; showPlays: true; model: most }
        AlbumGrid { id: grid; width: parent.width; height: implicitHeight; fitContent: true; model: added }
        AlbumCardOverlay { width: Metrics.cardMinWidth; height: width }
        AlbumHero { width: parent.width; detail: detail }
        PageHero { width: parent.width; label: "Artist"; round: true; name: "Mora Vale"; meta: "8 records" }
        PageHero { width: parent.width; label: "Playlist"; name: "Road Trip 2026"; meta: "19 songs"; covers: ["", "", "", ""]; canQueue: true }
        Row {
            height: Metrics.panesHeight
            PaneColumn { width: Metrics.sidebarWidth; height: parent.height; title: "Genre"; column: "genre" }
            PaneRow { width: Metrics.sidebarWidth; key: ""; label: "A selected entry"; meta: "8"; selected: true; all: false }
        }
        TrackColumns { id: rowCols; width: section.width }
        TrackHeader { cols: rowCols }
        Repeater {
            // A row resting, playing, selected, and a disc header.
            model: [
                { kind: 0, playing: false, selected: false },
                { kind: 0, playing: true, selected: false },
                { kind: 0, playing: false, selected: true },
                { kind: 1, playing: false, selected: false }
            ]
            TrackRow {
                required property var modelData
                cols: rowCols
                index: 1
                rowKind: modelData.kind
                number: 3
                title: "Faded Lines"
                artist: "Mora Vale"
                album: "Low Orbit"
                duration: "4:04"
                plays: 12
                quality: "FLAC 24/192"
                tier: 3
                playing: modelData.playing
                selected: modelData.selected
                art: ""
                headerLabel: "Disc 1"
                headerMeta: "6 songs"
            }
        }
        AlbumCard { width: Metrics.cardMinWidth; title: "Low Orbit"; subtitle: "Mora Vale"; tint: 2 }
        FolderModel { id: top; path: "" }
        FolderHead { width: section.width; folder: top }
        FolderCrumbs { width: section.width; path: "/music/Mora Vale/Low Orbit" }
        FolderEntryRow { width: section.width; index: 0; name: "Low Orbit"; count: 11; art: "" }
        PanelHeader { width: Metrics.rightPanel; title: "Queue"; detail: "12 next" }
        QueueRow { width: Metrics.rightPanel; index: 0; title: "Faded Lines"; artist: "Mora Vale"; duration: "4:04"; playing: true; art: "" }
        QueueRow { width: Metrics.rightPanel; index: 1; title: "Rivers"; artist: "Mora Vale"; duration: "5:48"; playing: false; art: "" }
        SettingsPage { id: audio; section: "appearance" }
        Repeater {
            model: audio
            SettingsRow { width: section.width; page: audio }
        }
        FolderRow { width: section.width; path: "/music/Mora Vale/Low Orbit"; name: "Low Orbit"; lastScan: new Date(); reachable: true }
        FolderRow { width: section.width; path: "/mnt/nas/DSD"; name: "DSD"; lastScan: undefined; reachable: false }
        ScanProgress { width: section.width }
        TrackMenu {}
        PlaylistMenu {}
        PlaylistRow { width: Metrics.sidebarWidth; index: 0; playlistId: 1; name: "Road Trip 2026"; tracks: 19; active: true }
        PlaylistRow { width: Metrics.sidebarWidth; index: 1; playlistId: 2; name: "Late Night"; tracks: 8 }
        SearchHit { width: Metrics.searchFlyout; index: 0; kind: 1; title: "Mora Vale"; detail: "Artist · 8 records"; art: ""; duration: "" }
        SearchHit { width: Metrics.searchFlyout; index: 1; kind: 4; title: "Blue Weather"; detail: "Mora Vale — Low Orbit"; art: ""; duration: "4:37" }
    }
}
