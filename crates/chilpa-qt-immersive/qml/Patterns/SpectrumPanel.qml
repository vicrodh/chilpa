import QtQuick
import QtQuick.Effects
import app.chilpa
import app.chilpa.kit
import app.chilpa.ambient
import app.chilpa.immersive
import app.chilpa.graphics
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "../Components"
import "../Primitives"
import "../Theme"

// I reuse the logarithmic queue renderer inside the existing Spectrum view.
// The immersive host owns capture; this panel only consumes its bars.
Rectangle {
    id: panel
    property bool shaders: true
    property var feed: Visualizer
    property int variant: [0, 5, 6].indexOf(Number(SettingsModel.kept("ui.immersive.spectrum.variant"))) >= 0
        ? Number(SettingsModel.kept("ui.immersive.spectrum.variant")) : 0
    readonly property string variantLabel: variant === 5 ? I18n.tr("PEAK", I18n.rev)
        : variant === 6 ? I18n.tr("Squares", I18n.rev) : I18n.tr("Bars", I18n.rev)
    function cycleVariant() {
        variant = variant === 0 ? 5 : variant === 5 ? 6 : 0
        SettingsModel.keep("ui.immersive.spectrum.variant", String(variant))
    }
    color: Theme.color.immersiveBlack
    readonly property real maxBar: panel.height * Stage.spectrumMaxShare

    readonly property var activeBins: [0, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]
    readonly property real slot: panel.width / Stage.spectrumColumns

    StreamSettle {
        id: bars
        target: panel.variant === 0 ? panel.feed.bars : []
        live: panel.visible && Immersive.open && panel.variant === 0
    }
    Gradient {
        id: barGradient
        GradientStop { position: 0; color: Ambient.secondary }
        GradientStop { position: 1; color: Ambient.primary }
    }
    Repeater {
        model: panel.variant === 0 ? Stage.spectrumColumns : 0
        Rectangle {
            required property int index
            readonly property int bin: index < Stage.spectrumColumns / 2 ? panel.activeBins[index] : panel.activeBins[Stage.spectrumColumns - 1 - index] // derived: mirrored
            x: index * panel.slot + Stage.spectrumBarLess / 2 // derived: inset in its slot
            width: panel.slot - Stage.spectrumBarLess
            height: bars.at(bin) * panel.maxBar
            anchors.bottom: parent.bottom
            anchors.bottomMargin: panel.height * Stage.spectrumBaseline
            gradient: barGradient
        }
    }

    Loader {
        id: variants
        width: panel.width
        height: panel.maxBar
        anchors.bottom: parent.bottom
        anchors.bottomMargin: panel.height * Stage.spectrumBaseline
        active: panel.visible && Immersive.open && panel.variant !== 0
        sourceComponent: Item {
            objectName: "immersiveSpectrumVisualizer"
            readonly property int mode: panel.variant
            readonly property bool gpuAvailable: GraphicsInfo.api !== GraphicsInfo.Software && GraphicsInfo.api !== GraphicsInfo.Null
            readonly property bool nativeSupported: gpuAvailable && width * 4 >= height
            CompactSpectrumState {
                id: animationState
                objectName: "immersiveSpectrumState"
                active: variants.active
                mode: panel.variant
                source: active ? panel.feed.bars : []
            }
            Loader {
                anchors.fill: parent
                active: parent.nativeSupported
                sourceComponent: SpectrumItem {
                    mode: panel.variant
                    values: animationState.values
                    peakValues: animationState.peaks
                    topColor: Ambient.secondary
                    bottomColor: Ambient.primary
                }
            }
            Loader {
                anchors.fill: parent
                active: parent.gpuAvailable && !parent.nativeSupported
                sourceComponent: CompactSpectrumShader {
                    mode: panel.variant
                    values: animationState.values
                    peakValues: animationState.peaks
                    topColor: Ambient.secondary
                    bottomColor: Ambient.primary
                }
            }
            Loader {
                anchors.fill: parent
                active: !parent.gpuAvailable
                sourceComponent: CompactSpectrumFallback {
                    mode: panel.variant
                    topColor: Ambient.secondary
                    bottomColor: Ambient.primary
                    modelState: QtObject {
                        readonly property int count: Stage.spectrumColumns
                        readonly property real slot: panel.slot
                        readonly property real height: panel.maxBar
                        function bin(i) { return panel.activeBins[i < count / 2 ? i : count - 1 - i] }
                        function at(i) { return animationState.values[bin(i)] || 0 }
                        function peak(i) { return animationState.peaks[bin(i)] || 0 }
                    }
                }
            }
        }
    }

    readonly property real artSize: Math.max(Stage.spectrumArtMin, Math.min(panel.height * Stage.spectrumArtShareTall, panel.width * Stage.spectrumArtShareWide, Stage.spectrumArtMax))
    Item {
        anchors.fill: parent
        anchors.topMargin: Stage.spectrumTop
        anchors.bottomMargin: Stage.reactivePadBottom
        Column {
            anchors.centerIn: parent
            width: panel.width
            spacing: Stage.spectrumApart
            Item {
                width: panel.width
                height: panel.artSize
                Rectangle {
                    width: panel.artSize
                    height: panel.artSize
                    x: (parent.width - width) / 2 // derived: centered
                    y: Stage.spectrumShadowDrop
                    radius: Stage.artRound
                    color: Theme.color.immersiveShadowDeep
                    layer.enabled: panel.shaders
                    layer.effect: MultiEffect { blurEnabled: true; blurMax: Stage.blurReachCard; blur: 1 }
                }
                ArtTile {
                    width: panel.artSize
                    height: panel.artSize
                    x: (parent.width - width) / 2 // derived: centered
                    radius: Stage.artRound
                    edged: false
                    source: Player.art
                }
            }
            Column {
                width: panel.width
                spacing: Stage.metaApart
                Text { width: parent.width; text: Player.title; color: Theme.color.immersiveTextStrong; font.pointSize: Stage.metaTitleText; font.weight: Type.bold; horizontalAlignment: Text.AlignHCenter; elide: Text.ElideRight }
                Text { width: parent.width; text: Player.artist; color: Theme.color.immersiveTextMuted; font.pointSize: Stage.metaArtistText; horizontalAlignment: Text.AlignHCenter; elide: Text.ElideRight }
                Text { visible: Player.album !== ""; width: parent.width; text: Player.album; color: Theme.color.immersiveTextGhost; font.pointSize: Stage.metaAlbumText; font.italic: true; horizontalAlignment: Text.AlignHCenter; elide: Text.ElideRight }
                Item {
                    visible: Player.qualityLong !== ""
                    width: parent.width
                    height: visible ? chip.height + Stage.metaBadgeTop : 0
                    QualityChip { id: chip; anchors.horizontalCenter: parent.horizontalCenter; anchors.bottom: parent.bottom; text: Player.qualityLong }
                }
            }
        }
    }
}
