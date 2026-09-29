import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

GallerySection {
    id: section
    title: "Components"

    TrackListModel { id: most; kind: "mostPlayed"; limit: 3; playingPath: Player.path }
    AlbumGridModel { id: added; kind: "recentlyAdded"; limit: 6; maxRows: 1; columns: grid.columns }
    AlbumDetailModel { id: detail; albumId: added.albumId(0, added.rev) }
    QtObject {
        id: playlistManager
        function open(index) {}
        function moveCustom(index, by) {}
        function folderIds() { return [] }
        function folderNames() { return [] }
        function setVisible(id, visible) {}
        function moveToFolder(id, folder) {}
        function removeFolder(id) {}
    }

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
            RatingStars { value: 3; onRated: (stars) => value = stars }
            PlayingIndicator {}
        }
        SectionHeader { width: parent.width; title: "Section header"; link: "See all" }
        LibraryCrumbs { width: parent.width; height: Metrics.breadcrumb }
        LibraryPanes { width: parent.width; height: Metrics.panesHeight }
        LibraryToolbar { width: parent.width; tracks: most }
        SortButton { value: "artist"; options: [{ value: "artist", text: "Artist" }, { value: "year", text: "Year" }] }
        AlphaStrip { height: Metrics.folderTreeWidth; letters: [["A", 0], ["M", 40], ["Z", 90]] }
        TrackList { width: parent.width; height: implicitHeight; fitContent: true; model: most }
        TrackList { width: parent.width; height: implicitHeight; fitContent: true; rich: true; showPlays: true; model: most }
        TrackGroupActions { row: 0; item: ({ headerLabel: "Disc 1" }); model: most }
        AlbumGrid { id: grid; width: parent.width; height: implicitHeight; fitContent: true; model: added }
        AlbumCarousel { width: parent.width; title: "From same artist"; model: added }
        Row {
            spacing: Metrics.gridGap
            AlbumCardOverlay { width: Metrics.cardMinWidth; height: width; albumId: added.albumId(0, added.rev); year: "2026"; selected: true }
            AlbumCard { width: Metrics.cardMinWidth; title: "Selected"; subtitle: "While selecting"; tint: 3; selected: true; selecting: true }
            AlbumCard { width: Metrics.cardMinWidth; title: "Quick view open"; subtitle: "Mora Vale"; tint: 4; expanded: true }
        }
        AlbumQuickView { width: parent.width; albumId: added.albumId(0, added.rev); notchX: Metrics.cardMinWidth / 2 } // derived: under the first card
        AlbumHeaderBackground { width: parent.width; height: Metrics.heroArt; art: detail.art; mode: "full" }
        AlbumHero { width: parent.width; detail: detail }
        CreditLine { credit: "One Artist, Another Artist"; color: hovered ? Theme.color.accent : Theme.color.textPrimary }
        AlbumVersionSelect { detail: ({ versions: JSON.stringify([
            { id: 1, label: "Local · FLAC 24/96 · 13 tracks", source: { local: true } },
            { id: 2, label: "Server · FLAC 16/44.1 · 13 tracks", source: { local: false } }
        ]), selectedId: 1, selectVersion: function(value) {} }) }
        ArtworkModal { id: artworkModal }
        PillButton { text: "Open artwork"; onClicked: artworkModal.show(detail.art) }
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
            // Resting, playing, selection-mode, selected, unavailable and
            // disc-header rows.
            model: [
                { kind: 0, playing: false, selected: false, selecting: false, available: true },
                { kind: 0, playing: true, selected: false, selecting: false, available: true },
                { kind: 0, playing: false, selected: false, selecting: true, available: true },
                { kind: 0, playing: false, selected: true, selecting: true, available: true },
                { kind: 0, playing: false, selected: false, selecting: false, available: false },
                { kind: 1, playing: false, selected: false, selecting: false, available: true }
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
                selectionMode: modelData.selecting
                art: ""
                headerLabel: "Disc 1"
                headerMeta: "6 songs"
                available: modelData.available
            }
        }
        AlbumCard { width: Metrics.cardMinWidth; title: "Low Orbit"; subtitle: "Mora Vale"; tint: 2 }
        AlbumCard { width: Metrics.cardMinWidth; title: "Away Record"; subtitle: "Server that does not answer"; tint: 3; available: false }
        FolderModel { id: top; path: ""; content: true }
        TrackListModel { id: folderTracks; kind: "folder"; filterRev: top.rev }
        AlbumGridModel { id: folderAlbums; kind: "folder"; contentRev: top.rev; columns: 3 }
        FolderHead { width: section.width; folder: top }
        FolderEntryRow { width: section.width; index: 0; name: "Low Orbit"; path: "/music/Low Orbit"; count: 11; art: "" }
        FolderTree { width: Metrics.folderTreeWidth; height: Metrics.panesHeight; activePath: "/music/Mora Vale" }
        FolderTreeBranch { width: Metrics.folderTreeWidth; name: "Mora Vale"; path: "/music/Mora Vale"; activePath: "/music/Mora Vale/Low Orbit" }
        FolderCard { width: Metrics.folderCardMin; name: "Low Orbit"; path: "/music/Low Orbit"; count: 11; art: "" }
        FolderMenu {}
        FolderDetail { width: section.width; height: Metrics.panesHeight; folder: top; tracks: folderTracks }
        PanelHeader { width: Metrics.rightPanel; title: "Queue"; detail: "12 songs · 48 min" }
        QueueFooter { width: Metrics.rightPanel }
        QueueMenu { model: folderTracks; laterLabel: "Library selection" }
        VolumeControl {}
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
        SeekBar { width: Metrics.searchFlyout; position: Metrics.searchFlyout / 4; duration: Metrics.searchFlyout; bubbleText: "1:10" } // derived: a quarter of the way
        OptionRow { width: Metrics.playbackOptionsWidth; title: "Gapless"; detail: "One track straight into the next."; checked: true }
        OptionRow { width: Metrics.playbackOptionsWidth; title: "Normalization"; detail: "Changed tracks are reported as processed." }
        PlaylistMenu {}
        AlbumMenu {}
        PlaylistPicker {}
        PlaylistRow { width: Metrics.sidebarWidth; index: 0; playlistId: 1; name: "Road Trip 2026"; tracks: 19; source: ""; sourceName: "Local"; local: true; artwork: ""; folderId: 0; visibleInSidebar: true; cover1: ""; cover2: ""; cover3: ""; cover4: ""; active: true }
        PlaylistRow { width: Metrics.sidebarWidth; index: 1; playlistId: 2; name: "Late Night"; tracks: 8; source: "demo"; sourceName: "Example source"; local: false; artwork: ""; folderId: 0; visibleInSidebar: true; cover1: ""; cover2: ""; cover3: ""; cover4: "" }
        PlaylistFolderRow { width: Metrics.sidebarWidth; index: 0; folderId: 1; name: "Road music"; tracks: 4; folderColor: "warning"; expanded: true }
        Row {
            spacing: Metrics.gap12
            PlaylistManagerRow { width: Metrics.playlistManagerCardWidth; index: 0; rowKind: 1; entryId: 1; name: "Road music"; detail: ""; source: ""; sourceName: "Local"; artwork: ""; cover1: ""; cover2: ""; cover3: ""; cover4: ""; colorName: "warning"; tracks: 4; plays: 0; folderId: 0; local: true; visibleInSidebar: true; manager: playlistManager }
            PlaylistManagerRow { width: Metrics.playlistManagerCardWidth; index: 1; rowKind: 0; entryId: 2; name: "Late Night"; detail: "Quiet records"; source: "demo"; sourceName: "Example source"; artwork: ""; cover1: ""; cover2: ""; cover3: ""; cover4: ""; colorName: ""; tracks: 18; plays: 9; folderId: 0; local: false; visibleInSidebar: true; manager: playlistManager }
        }
        PlaylistForm { width: Metrics.playlistModalWidth - Metrics.modalInsets; name: "Evening music" }
        Repeater {
            model: [
                { playlist: { name: "Empty playlist", mixed: true, track_count: 0 }, accepts: true, exact: 0, similar: 0 },
                { playlist: { name: "Already saved", mixed: true, track_count: 12 }, accepts: true, exact: 2, similar: 1 },
                { playlist: { name: "Source playlist", provider: "Home server", mixed: false, track_count: 9 }, accepts: false, exact: 0, similar: 0 }
            ]
            PlaylistTargetRow {
                required property var modelData
                required property int index
                width: Metrics.playlistModalWidth - Metrics.modalInsets
                rowIndex: index
                target: modelData
            }
        }
        SearchHit { width: Metrics.searchFlyout; index: 0; kind: 1; title: "Mora Vale"; detail: "Artist · 8 records"; art: ""; duration: "" }
        SearchHit { width: Metrics.searchFlyout; index: 1; kind: 4; title: "Blue Weather"; detail: "Mora Vale — Low Orbit"; art: ""; duration: "4:37" }
    }
}
