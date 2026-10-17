import QtQuick
import app.chilpa
import app.chilpa.graphics
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
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
    readonly property var scenePack: ({time: 8, beat: 0.7, beatAbove: 0.7, level: 0.6, levelSmooth: 0.4,
        transientAmp: 0.8, energyLo: Qt.vector4d(0.8, 0.6, 0.4, 0.3), energyHi: Qt.vector4d(0.3, 0.6, 0, 0),
        bandsLo: Qt.vector4d(0.8, 0.3, 0.6, 0.4), bandsHi: Qt.vector4d(0.4, 0.3, 0.5, 0.2),
        primary: Theme.color.accent, secondary: Theme.color.immersiveAxis, accent: Theme.color.immersiveTextStrong})
    readonly property var exampleFeed: QtObject {
        property var goniometer: Array.from({length: 512}, (_, i) => 0.6 * Math.sin(i * 0.09 + (i % 2) * 0.8))
        property var oscilloscope: Array.from({length: 512}, (_, i) => 0.7 * Math.sin(i * 0.025))
        property real stereoCorrelation: 0.63
    }
    function bedSample() {
        const values = new Float32Array(256 * 200)
        for (let row = 0; row < 200; ++row)
            for (let band = 0; band < 256; ++band)
                values[row * 256 + band] = 60 * Math.pow(Math.max(0, Math.sin(band * 0.045 + row * 0.07)), 4)
        return values.buffer
    }
    function ribbonSample() {
        const buffer = new ArrayBuffer(532)
        const header = new DataView(buffer)
        header.setUint32(0, 600, true)
        header.setUint32(4, 1, true)
        header.setUint32(12, 1, true)
        const row = new Uint8Array(buffer)
        for (let i = 0; i < 512; ++i)
            row[i + 20] = Math.round(220 * Math.abs(Math.sin(i * 0.025)))
        return buffer
    }

    Column {
        id: column
        x: Stage.edge
        y: Stage.edge
        spacing: Stage.edge
        ImmersiveContent { width: gallery.sampleWide; height: gallery.sampleTall; visible: true }
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
        LeaveButton {}
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
        Item { width: gallery.sampleWide; height: Stage.flowArtMin; CoverFlowCard { artwork: Player.art; occupied: true } }
        CoverFlowDeck { width: gallery.sampleWide; height: Stage.flowArtMin; currentPath: Player.path; currentArt: Player.art }
        ImmersiveLyricColumn { width: gallery.sampleWide; height: gallery.sampleTall; lines: [{text: "The verse before"}, {text: "A line in focus"}, {text: "The verse to come"}] }
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
        Loader { width: gallery.sampleWide; height: gallery.sampleTall; active: Graphics.gpuTier; sourceComponent: ScopePanel { feed: gallery.exampleFeed } }
        Loader { width: gallery.sampleWide; height: gallery.sampleTall; active: Graphics.gpuTier; sourceComponent: ScopeStage {} }
        Loader { width: gallery.sampleWide; height: gallery.sampleTall; active: Graphics.gpuTier; sourceComponent: ShaderSceneLayer { opened: true; scene: 3; running: false } }
        Item { width: gallery.sampleWide; height: gallery.sampleTall; SpectralOverlay { anchors.fill: parent } }
        Loader { width: gallery.sampleWide; height: gallery.sampleTall; active: Graphics.gpuTier; sourceComponent: PlasmaScene { Component.onCompleted: pulseTick(gallery.scenePack) } }
        Loader { width: gallery.sampleWide; height: gallery.sampleTall; active: Graphics.gpuTier; sourceComponent: RibbonScene { Component.onCompleted: pulseTick(gallery.ribbonSample(), Qt.vector4d(0.3, 0.6, 0, 0)) } }
        Loader { width: gallery.sampleWide; height: gallery.sampleTall; active: Graphics.gpuTier; sourceComponent: LineBedScene { Component.onCompleted: pulseTick(gallery.bedSample()) } }
        Loader { width: gallery.sampleWide; height: gallery.sampleTall; active: Graphics.gpuTier; sourceComponent: TunnelFlowScene { Component.onCompleted: pulseTick(8, new Array(16).fill(0.6)) } }
        Loader { width: gallery.sampleWide; height: gallery.sampleTall; active: Graphics.gpuTier; sourceComponent: PackScene { uniforms: ({energyLo: Qt.vector4d(0, 0, 0, 0), energyHi: Qt.vector4d(0, 0, 0, 0), bandsLo: Qt.vector4d(0, 0, 0, 0), bandsHi: Qt.vector4d(0, 0, 0, 0), timeS: 1, phase: 0, beat: 0, level: 0, resW: gallery.sampleWide, resH: gallery.sampleTall, levelSmooth: 0, transientAmp: 0}); fragmentShader: "qrc:/qt/qml/app/chilpa/graphics/assets/shaders/aurora.frag.qsb" } }
    }
}
