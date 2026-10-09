import QtQuick
import QtQuick.Effects
import app.chilpa
import app.chilpa.ambient
import app.chilpa.immersive
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"
import "../Primitives"
import "../Theme"

// Focus 3, Spectrum (03-immersive §1.4): opaque black, 28 columns that
// are the 14 active bars of the 16 (the first and last stay empty)
// mirrored about the center, each a gradient from the cover's secondary
// at the tip to its primary at the base, rising from 15 % of the height
// to at most 70 % of it. The cover sits smaller over the bars with its
// own block: no "Now Playing" sign here.
Rectangle {
    id: panel
    property bool shaders: true
    color: Theme.color.immersiveBlack
    readonly property var activeBins: [0, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]
    readonly property real slot: panel.width / Stage.spectrumColumns
    readonly property real maxBar: panel.height * Stage.spectrumMaxShare

    StreamSettle {
        id: bars
        target: Visualizer.bars
        live: panel.visible && Immersive.open
    }
    Gradient {
        id: barGradient
        GradientStop { position: 0; color: Ambient.secondary }
        GradientStop { position: 1; color: Ambient.primary }
    }
    Repeater {
        model: Stage.spectrumColumns
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
