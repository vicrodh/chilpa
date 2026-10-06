import "qrc:/qt/qml/com/blitzfc/chilpa/sources/qml/Components"
import QtQuick
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Components"

TitledFlow {
    CircleCheck { text: "Selected"; checked: true }
    ViewportScrollBar { visible: false }
    title: "Primitives"

    Row {
        spacing: Metrics.gap8
        Icon { glyph: Icons.home; color: Theme.color.accent }
        Icon { glyph: Icons.heartFilled; filled: true; color: Theme.color.accent }
        Icon { glyph: Icons.waveSquare }
    }

    Row {
        spacing: Metrics.gap8
        IconButton { tip: "Resting"; glyph: Icons.list }
        IconButton { tip: "Checked"; glyph: Icons.list; checked: true }
        IconButton { tip: "Accent"; glyph: Icons.arrowsShuffle; tone: "accent" }
        IconButton { tip: "Disabled"; glyph: Icons.chevronLeft; enabled: false }
        IconButton { tip: "Round"; glyph: Icons.x; round: true; size: Metrics.windowControl }
    }

    Row {
        spacing: Metrics.gap8
        PillButton { kind: "primary"; glyph: Icons.playerPlayFilled; filled: true; text: "Play" }
        PillButton { glyph: Icons.arrowsShuffle; text: "Shuffle" }
        PillButton { text: "Open modal"; onClicked: sampleModal.open() }
    }

    Modal {
        id: sampleModal
        width: Metrics.playlistFolderDialog
        Column {
            spacing: Metrics.gap12
            Text { text: "Modal"; font.pointSize: Type.titleSmall; font.weight: Type.bold; color: Theme.color.textPrimary }
            Text { text: "Shared padding and surface"; color: Theme.color.textSecondary }
            PillButton { kind: "primary"; text: "Close"; onClicked: sampleModal.close() }
        }
    }

    SegmentedControl {
        options: [{ glyph: Icons.list, tip: "Compact" }, { glyph: Icons.layoutList, tip: "Rich" }]
        current: 1
    }

    SearchField { width: Metrics.sidebarWidth; placeholder: "Search"; shortcut: "Ctrl K" }

    Column {
        width: Metrics.playlistManagerCardWidth
        ListRowSurface { width: parent.width; height: Metrics.rowCompact; rowIndex: 0 }
        ListRowSurface { width: parent.width; height: Metrics.rowCompact; rowIndex: 1 }
    }

    Row {
        spacing: Metrics.gap8
        ResizeHandle { height: Metrics.sidebarCollapsed }
        ResizeHandle { vertical: false; width: Metrics.sidebarCollapsed }
        Toggle { checked: true }
        Chip { text: "All"; active: true }
        Chip { text: "Songs" }
        TextField { placeholder: "Token"; secret: true }
        Swatches { colors: ["", Theme.color.accentHover, Theme.color.textSecondary]; values: ["a", "b", "c"]; value: "b" }
        LevelSlider { value: 60 }
        Toggle {}
        SelectField { options: ["ALSA", "PipeWire"]; values: ["alsa", "pipewire"]; marks: ["BP", ""]; value: "alsa" }
        SourceIcon { sourceInfo: ({ local: true }) }
        Item { width: Metrics.button32; height: Metrics.button32; Hold {} }
        Repeater {
            model: ["opensubsonic", "navidrome", "jellyfin", "plex", "collection", "cassette", "cast-audio", "dlna", "musicbrainz", "discogs", "gandalf"]
            SourceIcon {
                required property string modelData
                sourceInfo: ({ icon: modelData })
                color: Theme.color.accent
            }
        }
        StatusField { state: "fine"; options: ["Disconnect", "Uninstall"]; values: ["disconnect", "uninstall"] }
    }

    Rectangle {
        width: Metrics.menuWidth
        height: entries.implicitHeight
        radius: Metrics.radiusMenu
        color: Theme.color.surfaceRaised
        Edge {}
        Column {
            id: entries
            width: parent.width
            MenuEntry { width: parent.width; entry: ({ text: "Settings", hint: "Ctrl+," }) }
            MenuEntry { width: parent.width; entry: ({ separator: true }) }
            MenuEntry { width: parent.width; entry: ({ text: "USB DAC", detail: "hw:1,0", checked: true, mark: "BP" }) }
        }
        Menu { entries: [{ text: "A menu, closed" }] }
    }

    Rectangle {
        width: Metrics.sidebarWidth
        height: Metrics.fieldHeight
        radius: Metrics.radiusPanel
        color: Theme.color.surfaceSidebar
        Edge {}
    }

    Rectangle {
        width: Metrics.sidebarWidth
        height: Metrics.fieldHeight
        color: Theme.color.accent
        PlaneCorners { width: parent.width }
    }

    Row {
        spacing: Metrics.gap8
        ArtTile { width: Metrics.heroArt / 2; height: width; tint: 0 } // derived: half a hero cover
        ArtTile { width: Metrics.artRow; height: width; tint: 3; radius: Metrics.radiusArtSmall }
    }

}
