import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Components"
import "../Patterns"
import "../Primitives"
import "../Theme"

// The immersive view's pieces, one by one, over the dark stage: the
// buttons of the bar, the bar, the header, the card and the metadata, and
// each panel at a small size. The overlay itself is a Screen and is seen
// in the window.
Rectangle {
    id: gallery
    color: Theme.color.immersiveStage
    implicitWidth: column.implicitWidth + Stage.edge * 2 // derived: the pieces and the margins
    implicitHeight: column.implicitHeight + Stage.edge * 2 // derived: the pieces and the margins
    readonly property int sampleWide: 720
    readonly property int sampleTall: 440

    Column {
        id: column
        x: Stage.edge
        y: Stage.edge
        spacing: Stage.edge
        Row {
            spacing: Stage.menuRowPad
            GhostButton { glyph: Icons.heart }
            GhostButton { glyph: Icons.heartFilled; filled: true; active: true }
            GhostButton { glyph: Icons.arrowsShuffle; able: false }
            NavButton { glyph: Icons.playerSkipBackFilled }
            PlayDisc { playing: false }
            PlayDisc { playing: true }
            QualityChip { text: "FLAC · 24-bit / 96 kHz" }
            PlayingBars { active: true; anchors.verticalCenter: parent.verticalCenter }
        }
        TinyBar { width: Stage.barWide / 2; value: 0.4 } // derived: half a bar
        VolumeBar { width: Stage.volumeBarWide; value: 0.7 }
        ViewTrigger {}
        WindowCapsule {}
        ImmersivePlayerBar { viewportWide: gallery.sampleWide }
        Item { width: gallery.sampleWide; height: Stage.headerTall; ImmersiveHeader { anchors.fill: parent } }
        TrackMeta { width: gallery.sampleWide / 2 } // derived: half the sample
        SongCard {}
        StreamSettle {}
        Row {
            spacing: Stage.menuRowPad
            ImmersiveQueueTab { label: "Queue"; count: 3; active: true }
            ImmersiveQueueTab { label: "History"; count: 12 }
        }
        ImmersiveQueueRow { width: gallery.sampleWide / 2; title: "A title"; artist: "An artist"; duration: "3:21" } // derived: half the sample
        ImmersiveQueueRow { width: gallery.sampleWide / 2; title: "The one playing"; artist: "An artist"; duration: "4:01"; current: true } // derived: half the sample
        LyricsFeed {}
        Row {
            spacing: Stage.menuRowPad
            StaticPanel { width: gallery.sampleWide / 2; height: gallery.sampleTall } // derived: half the sample
            AlbumReactivePanel { width: gallery.sampleWide / 2; height: gallery.sampleTall } // derived: half the sample
        }
        Row {
            spacing: Stage.menuRowPad
            SpectrumPanel { width: gallery.sampleWide / 2; height: gallery.sampleTall } // derived: half the sample
            WaveBedPanel { width: gallery.sampleWide / 2; height: gallery.sampleTall } // derived: half the sample
        }
        Row {
            spacing: Stage.menuRowPad
            CoverFlowPanel { width: gallery.sampleWide / 2; height: gallery.sampleTall } // derived: half the sample
            ReactiveRingsPanel { width: gallery.sampleWide / 2; height: gallery.sampleTall } // derived: half the sample
        }
        Row {
            spacing: Stage.menuRowPad
            LyricsFocusPanel { width: gallery.sampleWide / 2; height: gallery.sampleTall } // derived: half the sample
            QueueTabsPanel { width: gallery.sampleWide / 2; height: gallery.sampleTall; centered: false } // derived: half the sample
        }
        SplitLayout { width: gallery.sampleWide; height: gallery.sampleTall }
        ReactiveSplitPanel { width: gallery.sampleWide; height: gallery.sampleTall }
        ReactiveSplitPanel { width: gallery.sampleWide; height: gallery.sampleTall; lyricsMode: true }
        Item { width: gallery.sampleWide; height: gallery.sampleTall; SearchResults { anchors.fill: parent; query: "ab"; open: true } }
        ViewMenu { visible: false }
    }
}
