import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

GallerySection {
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
    }

    SegmentedControl {
        options: [{ glyph: Icons.list, tip: "Compact" }, { glyph: Icons.layoutList, tip: "Rich" }]
        current: 1
    }

    SearchField { width: Metrics.sidebarWidth; placeholder: "Search"; shortcut: "Ctrl K" }

    Row {
        spacing: Metrics.gap8
        ResizeHandle { height: Metrics.sidebarCollapsed }
        ResizeHandle { vertical: false; width: Metrics.sidebarCollapsed }
        Toggle { checked: true }
        Chip { text: "All"; active: true }
        Chip { text: "Songs" }
        TextField { placeholder: "Token"; secret: true }
        Swatches { colors: ["", Theme.color.accentHover, Theme.color.textSecondary]; values: ["a", "b", "c"]; value: "b" }
        Toggle {}
        SelectField { options: ["ALSA", "PipeWire"]; values: ["alsa", "pipewire"]; marks: ["BP", ""]; value: "alsa" }
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

    Row {
        spacing: Metrics.gap8
        ArtTile { width: Metrics.heroArt / 2; height: width; tint: 0 } // derived: half a hero cover
        ArtTile { width: Metrics.artRow; height: width; tint: 3; radius: Metrics.radiusArtSmall }
    }

}
