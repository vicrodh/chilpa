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
        TrackList { width: parent.width; height: implicitHeight; fitContent: true; model: most }
        TrackList { width: parent.width; height: implicitHeight; fitContent: true; rich: true; showPlays: true; model: most }
        AlbumGrid { id: grid; width: parent.width; height: implicitHeight; fitContent: true; model: added }
        AlbumCardOverlay { width: Metrics.cardMinWidth; height: width }
        AlbumHero { width: parent.width; detail: detail }
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
                headerLabel: "Disc 1"
                headerMeta: "6 songs"
            }
        }
        AlbumCard { width: Metrics.cardMinWidth; title: "Low Orbit"; subtitle: "Mora Vale"; tint: 2 }
    }
}
