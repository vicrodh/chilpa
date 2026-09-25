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
        PillButton { kind: "primary"; glyph: Icons.playerPlayFilled; text: "Play" }
        PillButton { glyph: Icons.arrowsShuffle; text: "Shuffle" }
    }

    SegmentedControl {
        options: [{ glyph: Icons.list, tip: "Compact" }, { glyph: Icons.layoutList, tip: "Rich" }]
        current: 1
    }

    SearchField { width: Metrics.sidebarWidth; placeholder: "Search"; shortcut: "Ctrl K" }

    Row {
        spacing: Metrics.gap8
        ArtTile { width: Metrics.heroArt / 2; height: width; tint: 0 } // derived: half a hero cover
        ArtTile { width: Metrics.artRow; height: width; tint: 3; radius: Metrics.radiusArtSmall }
    }

}
