import "qrc:/qt/qml/app/chilpa/sources/qml/Components"
import QtQuick
import app.chilpa
import app.chilpa.folders
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "qrc:/qt/qml/app/chilpa/folders/qml/Components"
import "qrc:/qt/qml/app/chilpa/folders/qml/Patterns"

TitledFlow {
    id: section
    title: "Components"

    TrackListModel { id: most; kind: "mostPlayed"; limit: 3; playingPath: Player.path }
    AlbumGridModel { id: added; kind: "recentlyAdded"; limit: 6; maxRows: 1; columns: grid.columns }
    PlaylistSourceWrite { id: sourceWritePreview }
    AlbumDetailModel { id: detail; albumId: added.albumId(0, added.rev) }
    AlbumArtistLine { document: ({artist_html:"<a href=\"0\">Artist</a>",artist_links:[{target:"artist",key:"Artist"}]}) }
    Clipboard {}
    ShelfSkeleton { width: Metrics.confirmWidth; count: 3 }
    QueueProgress { visible: true }

    Column {
        width: section.width
        spacing: Metrics.gap12

        PlaylistSourceChooser { sources: [{id: "demo", name: "Demo source", accepts: true}] }
        PlaylistNotice { text: "A source is currently unavailable." }
        Flow {
            width: parent.width
            spacing: Metrics.gap12
            NavItem { width: Metrics.sidebarWidth; text: "Library"; glyph: Icons.columns3 }
            NavItem { width: Metrics.sidebarWidth; text: "Active"; glyph: Icons.home; active: true }
            NavItem { width: Metrics.sidebarCollapsed; text: "Collapsed"; glyph: Icons.disc; collapsed: true }
            NavItem { width: Metrics.sidebarWidth; text: "Plugin fallback"; glyph: Icons.letterG }
            NavItem { width: Metrics.sidebarWidth; text: "Unavailable plugin image"; glyph: Icons.letterG; iconSource: "file:///missing-plugin-icon.png" }
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
        AnimatedCover { width: Metrics.heroArt; height: width; colors: [Theme.color.surfacePanel, Theme.color.accent, Theme.color.statusWarning]; label: "Daily mix" }
        ScrollMemory { key: "gallery" }
        LibraryCrumbs { width: parent.width; height: Metrics.breadcrumb }
        LibraryPanes { width: parent.width; height: Metrics.panesHeight }
        LibraryToolbar { width: parent.width; tracks: most }
        SortButton { value: "artist"; options: [{ value: "artist", text: "Artist" }, { value: "year", text: "Year" }] }
        AlbumRow { width: parent.width; albumId: 1; title: "Low Orbit"; subtitle: "Mora Vale"; year: "2019"; art: "" }
        AlbumArtistText { text: "Mora Vale"; albumId: 1 }
        AlphaStrip { height: Metrics.folderTreeWidth; letters: [["A", 0], ["M", 40], ["Z", 90]] }
        TrackList { width: parent.width; height: implicitHeight; fitContent: true; model: most }
        TrackList { width: parent.width; height: implicitHeight; fitContent: true; rich: true; showPlays: true; model: most }
        TrackList { width: parent.width; height: implicitHeight; fitContent: true; reorderable: true; model: most }
        // The list's own pointer, shown by the lists above; here it takes nothing.
        TrackListPointer { enabled: false }
        TrackGroupActions { row: 0; item: ({ headerLabel: "Disc 1" }); model: most }
        AlbumGrid { id: grid; width: parent.width; height: implicitHeight; fitContent: true; model: added }
        AlbumCarousel { width: parent.width; title: "From same artist"; model: added }
        Row {
            spacing: Metrics.gridGap
            AlbumCardOverlay { width: CardLayout.cardWidth; height: width; albumId: added.albumId(0, added.rev); year: "2026"; selected: true }
            AlbumCard { width: CardLayout.cardWidth; title: "Selected"; subtitle: "While selecting"; tint: 3; selected: true; selecting: true }
            AlbumCard { width: CardLayout.cardWidth; title: "Quick view open"; subtitle: "Mora Vale"; tint: 4; expanded: true }
        }
        AlbumQuickView { width: parent.width; albumId: added.albumId(0, added.rev); notchX: Metrics.cardMinWidth / 2 } // derived: under the first card
        AlbumHeaderBackground { width: parent.width; height: Metrics.heroArt; art: detail.art; mode: "full" }
        AlbumHero { width: parent.width; detail: detail }
        CreditLine { credit: "One Artist, Another Artist"; color: hovered ? Theme.color.accent : Theme.color.textPrimary }
        AlbumVersionSelect { detail: ({ versions: JSON.stringify([
            { id: 1, label: "Local · FLAC 24/96 · 13 tracks", source: { local: true } },
            { id: 2, label: "Server · FLAC 16/44.1 · 13 tracks", source: { local: false } }
        ]), selectedId: 1, selectVersion: function(value) {} }) }
        PageHero { width: parent.width; label: "Artist"; round: true; name: "Mora Vale"; meta: "8 records" }
        PageHero { width: parent.width; label: "Playlist"; name: "Road Trip 2026"; meta: "19 songs"; covers: ["", "", "", ""]; canQueue: true }
        Row {
            height: Metrics.panesHeight
            PaneColumn { width: Metrics.sidebarWidth; height: parent.height; title: "Genre"; column: "genre" }
            PaneRow { width: Metrics.sidebarWidth; key: ""; label: "A selected entry"; meta: "8"; selected: true; all: false }
        }
        TrackRowSlot { width: section.width; height: Metrics.rowCompact; inViewport: false }
        TrackColumns { id: rowCols; width: section.width }
        TrackHeader { cols: rowCols }
        TrackSourceLabel { cols: TrackColumns { showSource: true } name: "Local" }
        // A divider with nothing of its own, and the header of a record
        // among several.
        Item {
            width: Metrics.lcdArt; height: Metrics.lcdArt
            ArtPreview { target: parent; shown: false }
        }
        TrackGroupHeader { width: section.width; label: "Disc 2 \u2014 Live"; meta: "9 songs" }
        TrackRowHeader {
            row: ({ header: true, headerWork: false, record: false, headerLabel: "Disc 2", headerMeta: "9 songs", headerComposer: "", headerComposerTarget: "", artist: "", art: "", index: 2, actionHovered: 0, width: section.width, height: Metrics.rowGroup, cols: { indexX: 0 } })
            actionsEnabled: true
        }
        TrackGroupHeader { width: section.width; work: true; label: "Das Rheingold, Scene 1"; composer: "Richard Wagner"; composerTarget: "artist:Richard Wagner" }
        TrackGroupHeader { width: section.width; record: true; label: "Low Orbit \u2014 Live"; artist: "Mora Vale"; meta: "2019 \u00b7 Disc 2 \u00b7 9 tracks" }
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
                trackId: 1
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
        AlbumCard { width: CardLayout.cardWidth; title: "Low Orbit"; subtitle: "Mora Vale"; tint: 2 }
        AlbumCard { width: CardLayout.cardWidth; title: "Away Record"; subtitle: "Server that does not answer"; tint: 3; available: false }
        FolderModel { id: top; path: ""; content: true }
        TrackListModel { id: folderTracks; kind: "folder"; filterRev: top.rev }
        AlbumGridModel { id: folderAlbums; kind: "folder"; contentRev: top.rev; columns: 3 }
        FolderEntryRow { width: section.width; index: 0; name: "Low Orbit"; path: "/music/Low Orbit"; count: 11; folders: 2; album: 0; art: "" }
        FolderTree { width: Metrics.folderTreeWidth; height: Metrics.panesHeight; activePath: "/music/Mora Vale" }
        FolderTreeRow { width: Metrics.folderTreeWidth; index: 0; name: "Mora Vale"; path: "/music/Mora Vale"; count: 11; depth: 1; open: true; file: false; active: true }
        FolderCard { width: Metrics.folderCardMin; name: "Mora Vale"; path: "/music/Mora Vale"; count: 41; folders: 3; album: 0; art: "" }
        FolderMenu {}
        FolderBulkBar { width: Metrics.folderTreeWidth; count: 3 }
        FolderDetail { width: section.width; height: Metrics.panesHeight; folder: top; tracks: folderTracks; albums: folderAlbums }
        PanelHeader { width: Metrics.rightPanel; title: "Queue"; detail: "12 songs · 48 min" }
        QueueMenu { model: folderTracks; laterLabel: "Library selection" }
        VolumeControl {}
        QueueRow { width: Metrics.rightPanel; index: 0; title: "Faded Lines"; artist: "Mora Vale"; duration: "4:04"; playing: true; art: "" }
        QueueRow { width: Metrics.rightPanel; index: 1; title: "Rivers"; artist: "Mora Vale"; duration: "5:48"; playing: false; art: "" }
        ChoiceRow {
            width: Metrics.rightPanel
            title: "Size"
            SegmentedControl { options: [{ text: "S" }, { text: "M" }, { text: "L" }]; current: 1 }
        }
        SettingsPage { id: audio; section: "appearance" }
        Repeater {
            model: audio
            SettingsRow { width: section.width; page: audio }
        }
        FolderRow { width: section.width; path: "/music/Mora Vale/Low Orbit"; name: "Low Orbit"; lastScan: new Date(); reachable: true }
        FolderRow { width: section.width; path: "/mnt/nas/DSD"; name: "DSD"; lastScan: undefined; reachable: false }
        ScanProgress { width: section.width }
        PillButton {
            text: "Analysis offer"
            onClicked: analysisOffer.open()
            AnalysisOffer { id: analysisOffer }
        }
        TrackMenu {}
        PillButton {
            text: "Extra queues"
            onClicked: cascade.open()
            CascadingMenu {
                id: cascade
                y: parent.height
                entries: [{text:"Play now", act:"play"}, {text:"Extra queues", submenu:true, children:[{text:"New queue",act:"new"},{text:"Add to Jazz",act:"add"}]}]
            }
        }
        SeekBar { width: Metrics.searchFlyout; position: Metrics.searchFlyout / 4; duration: Metrics.searchFlyout; bubbleText: "1:10" } // derived: a quarter of the way
        OptionRow { width: Metrics.playbackOptionsWidth; title: "Gapless"; detail: "One track straight into the next."; checked: true }
        OptionRow { width: Metrics.playbackOptionsWidth; title: "Volume normalization"; detail: "Level loudness across tracks. This breaks strict bit perfect." }
        AlbumMenu {}
        SearchMenu {}
        PlaylistPicker {}
        PlaylistRow { width: Metrics.sidebarWidth; index: 0; playlistId: 1; name: "Road Trip 2026"; tracks: 19; source: ""; sourceName: "Local"; local: true; artwork: ""; folderId: 0; visibleInSidebar: true; covers: []; active: true }
        PlaylistRow { width: Metrics.sidebarWidth; index: 1; playlistId: 2; name: "Late Night"; tracks: 8; source: "demo"; sourceName: "Example source"; local: false; artwork: ""; folderId: 0; visibleInSidebar: true; covers: [] }
        PlaylistCollage { covers: [] }
        MarkButtons { kind: "album"; key: "1" }
        RatingButton { kind: "album"; target: 1 }
        Row {
            spacing: Metrics.gridGap
            TrackRowMove { height: Metrics.rowCompact }
            TrackRowMove { height: Metrics.rowRich; stacked: true }
            TrackRowLead { width: Metrics.colMoveCompact + Metrics.colIndexCompact; height: Metrics.rowCompact; gutter: Metrics.colMoveCompact; reorder: true; number: 7 }
            Item { id: oneDragged; width: Metrics.menuWidth; height: Metrics.rowRich; DragGhost { parent: oneDragged; title: "Different World"; detail: "Iron Maiden · A Matter Of Life And Death" } }
            Item { id: severalDragged; width: Metrics.menuWidth; height: Metrics.rowRich; DragGhost { parent: severalDragged; count: 3 } }
        }
        TrackRowEnd { height: Metrics.rowCompact; trackId: 1 }
        PlaylistFolderRow { width: Metrics.sidebarWidth; index: 0; folderId: 1; name: "Road music"; tracks: 4; folderColor: "warning"; expanded: true }
        PlaylistForm { width: Metrics.playlistModalWidth - Metrics.modalInsets; name: "Evening music" }
        SearchHit { width: Metrics.searchFlyout; index: 0; kind: 1; title: "Mora Vale"; detail: "Artist · 8 records"; art: ""; duration: "" }
        SearchHit { width: Metrics.searchFlyout; index: 1; kind: 4; title: "Blue Weather"; detail: "Mora Vale — Low Orbit"; art: ""; duration: "4:37" }
        SourceCrumbs { width: Metrics.searchFlyout; height: Metrics.breadcrumb; view: ({ name: "Radio", crumbs: ["By country", "Mexico"], explorer: false, columns: [], shortcuts: [{ id: "chilpa:saved", title: "Saved" }] }) }
        SourceColumn {
            width: Metrics.searchFlyout / 2 // derived: half a flyout
            height: Metrics.panesHeight
            column: ({ id: "country", title: "Country", picked: "mx", entries: [{ id: "ar", title: "Argentina", subtitle: "1,140" }, { id: "mx", title: "Mexico", subtitle: "980" }] })
        }
        SourceState { about: ({ error: "The source did not answer." }) }
        SourceTabs { model: [{ key: "", title: "All", active: true }, { key: "", title: "Releases", active: false }] }
        SourceAbout { width: Metrics.searchFlyout; entry: ({ title: "Low Orbit Radio", subtitle: "Mora Vale", image: "x", description: "A show about what plays between two records.", facts: [{ label: "Genre", value: "Music" }] }) }
    }
}
