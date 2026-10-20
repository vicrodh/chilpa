import QtQuick
import QtQuick.Effects
import app.chilpa
import app.chilpa.kit
import app.chilpa.ambient
import app.chilpa.immersive
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/lyrics/qml/Components"
import "../Components"
import "../Primitives"
import "../Theme"

// Split 4 and 5, the cinematic card (03-immersive §1.5): one glass card of
// 85 % of the width over the field, the cover blurred and darkened inside
// it, the cover sharp at the left, and at the right either the track's
// lines with a second seek in the cover's lifted accent or, in lyrics
// mode, the line being sung. Below, 48 bars of the waveform grow upward
// from a baseline of the same accent.
Item {
    id: panel
    property bool lyricsMode: false
    property bool shaders: true
    property LyricsFeed feed: null

    readonly property real stageTop: Math.min(Stage.cineTopMax, Math.max(Stage.cineTopMin, panel.height * Stage.cineTopShare))
    readonly property real playerTop: Math.max(stageTop + Stage.cinePlayerAbove, panel.height - Stage.cinePlayerLess)
    readonly property real stageTall: Math.max(Stage.cinePlayerAbove, playerTop - stageTop)
    readonly property real cardWide: Math.max(1, Math.min(panel.width - Stage.cineCardLess, panel.width * Stage.cineCardShare))
    readonly property real cardTall: Math.max(Stage.cineCardMinTall, Math.min(stageTall * Stage.cineCardTallShare, cardWide * Stage.cineCardRatio))
    readonly property real cardTop: stageTop + Math.max(0, stageTall - cardTall) * Stage.cineCardPlace
    readonly property real artSize: Math.min(cardTall * Stage.cineArtTallShare, cardWide * Stage.cineArtWideShare)
    readonly property real waveBottom: playerTop - Stage.cineWaveAbove
    readonly property real waveRoom: Math.max(2, waveBottom - (cardTop + cardTall))
    readonly property real waveTall: Math.max(2, waveRoom * Stage.cineWaveTallShare)
    readonly property real overscan: Math.max(Stage.cineBlurMin, Math.min(cardWide, cardTall) * Stage.cineBlurShare)
    readonly property color accent: Ambient.lifted
    readonly property bool hasArt: Player.art.toString() !== ""
    readonly property bool hasLine: feed !== null && feed.currentText !== ""

    StreamSettle {
        id: wave
        live: panel.visible && Immersive.open
        easeK: Stage.cineWaveEase
        target: {
            const samples = Visualizer.waveform
            const out = new Array(Stage.cineWaveColumns)
            for (let i = 0; i < Stage.cineWaveColumns; ++i) {
                let v = samples[Math.floor(i * 256 / Stage.cineWaveColumns)] || 0
                if (v < 0)
                    v = -v
                out[i] = Math.min(1, v)
            }
            return out
        }
    }
    Item {
        id: waveform
        anchors.horizontalCenter: parent.horizontalCenter
        y: panel.waveBottom - height
        width: panel.cardWide * Stage.cineWaveWide
        height: panel.waveTall
        Rectangle {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: Metrics.hairline
            color: panel.accent
            opacity: Stage.cineBaselineAlpha
        }
        Repeater {
            model: Stage.cineWaveColumns
            Rectangle {
                required property int index
                readonly property real slot: waveform.width / Stage.cineWaveColumns
                x: index * slot + (slot - width) / 2 // derived: centered in its slot
                anchors.bottom: parent.bottom
                width: Math.max(Stage.cineWaveBarMin, slot * Stage.cineWaveBarShare)
                height: Math.max(Stage.waveMinTall, wave.at(index) * (waveform.height - Metrics.hairline))
                radius: Math.min(Metrics.eqBar, width / 2) // derived: rounded ends
                color: panel.accent
                opacity: index % 5 === 0 ? 1 : 0.82
            }
        }
    }

    Item {
        id: card
        x: (panel.width - width) / 2 // derived: centered
        y: panel.cardTop
        width: panel.cardWide
        height: panel.cardTall
        clip: true
        Rectangle { anchors.fill: parent; radius: Stage.cineCardRound; color: Theme.color.immersiveCardGlass }
        Item {
            id: cardMask
            anchors.fill: parent
            visible: false
            layer.enabled: panel.shaders && panel.hasArt
            layer.smooth: true
            Rectangle { anchors.fill: parent; radius: Stage.cineCardRound; color: Theme.color.immersiveTextStrong }
        }
        Item {
            anchors.fill: parent
            visible: panel.shaders && panel.hasArt
            clip: true
            layer.enabled: visible
            layer.smooth: true
            layer.effect: MultiEffect { maskEnabled: true; maskSource: cardMask; maskThresholdMin: 0.5; maskSpreadAtMin: 1 }
            Item {
                anchors.fill: parent
                clip: true
                layer.enabled: parent.visible
                layer.smooth: true
                layer.effect: MultiEffect { blurEnabled: true; blurMax: Stage.blurReach; blur: Stage.cineBlur }
                Image {
                    anchors.fill: parent
                    anchors.margins: -panel.overscan
                    source: Player.art
                    asynchronous: true
                    cache: true
                    smooth: true
                    mipmap: true
                    fillMode: Image.PreserveAspectCrop
                    sourceSize.width: Stage.cineBackdropDecode
                }
            }
        }
        ArtTile { anchors.fill: parent; radius: Stage.cineCardRound; edged: false; source: Player.art; visible: !panel.shaders && panel.hasArt }
        Rectangle { anchors.fill: parent; radius: Stage.cineCardRound; color: Theme.color.immersiveCardVeil }
        Rectangle {
            anchors.fill: parent
            radius: Stage.cineCardRound
            gradient: Gradient {
                GradientStop { position: 0; color: Theme.color.immersiveCardGradientTop }
                GradientStop { position: 0.55; color: Theme.color.immersiveCardGradientMid }
                GradientStop { position: 1; color: Theme.color.immersiveCardGradientBottom }
            }
        }

        readonly property real artInset: (height - panel.artSize) / 2 // derived: centered vertically
        readonly property real contentGap: Math.max(Stage.cineTextGapMin, width * Stage.cineTextGapShare)
        Rectangle {
            x: card.artInset
            y: card.artInset + Stage.artShadowDrop
            width: panel.artSize
            height: panel.artSize
            radius: Stage.cineCardRound
            color: Theme.color.immersiveShadowDeep
            layer.enabled: panel.shaders
            layer.effect: MultiEffect { blurEnabled: true; blurMax: Stage.blurReachArt; blur: Stage.cineArtBlur }
        }
        Rectangle { x: card.artInset; y: card.artInset; width: panel.artSize; height: panel.artSize; radius: Stage.cineCardRound; color: Theme.color.immersivePlaceholder; visible: !panel.hasArt }
        ArtTile { x: card.artInset; y: card.artInset; width: panel.artSize; height: panel.artSize; radius: Stage.cineCardRound; edged: false; source: Player.art; visible: panel.hasArt }

        Item {
            id: content
            anchors.left: parent.left
            anchors.leftMargin: card.artInset + panel.artSize + card.contentGap
            anchors.right: parent.right
            anchors.rightMargin: card.contentGap
            anchors.top: parent.top
            anchors.topMargin: card.artInset
            anchors.bottom: parent.bottom
            anchors.bottomMargin: card.artInset

            Item {
                anchors.fill: parent
                visible: !panel.lyricsMode
                readonly property real titleSize: Math.max(Stage.cineTitleMin, Math.min(Stage.cineTitleMax, card.height * Stage.cineTitleShare)) * Type.pointsPerPixel // derived: pixels to points
                readonly property real metaSize: Math.max(Stage.cineSubMin, Math.min(Stage.cineSubMax, card.height * Stage.cineSubShare)) * Type.pointsPerPixel // derived: pixels to points
                readonly property real timeSize: Math.max(Stage.cineTimeMin, Math.min(Stage.cineTimeMax, card.height * Stage.cineTimeShare)) * Type.pointsPerPixel // derived: pixels to points
                id: nowPane
                Column {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: Stage.metaApart + 1 // derived: seven pixels
                    Item {
                        visible: Player.playing && !Player.paused
                        width: parent.width
                        height: visible ? Metrics.eqTall * Stage.cineNowScale : 0
                        Row {
                            anchors.left: parent.left
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: Stage.menuRowPad
                            PlayingBars { tint: panel.accent; barScale: Stage.cineNowScale; active: panel.visible && Immersive.open; anchors.verticalCenter: parent.verticalCenter }
                            Text { text: I18n.tr("Now Playing", I18n.rev); color: panel.accent; font.pointSize: Stage.metaArtistText; font.weight: Type.semibold; font.letterSpacing: Stage.nowTracking; anchors.verticalCenter: parent.verticalCenter }
                        }
                    }
                    Text { width: parent.width; text: Player.title; color: Theme.color.immersiveText; font.pointSize: nowPane.titleSize; font.weight: Type.bold; wrapMode: Text.WordWrap }
                    Text { visible: Player.album !== ""; width: parent.width; text: Player.album; color: Theme.color.immersiveTextGhost; font.italic: true; font.pointSize: nowPane.metaSize; elide: Text.ElideRight }
                    Text { visible: Player.artist !== ""; width: parent.width; text: Player.artist; color: Theme.color.immersiveTextMuted; font.pointSize: nowPane.metaSize; elide: Text.ElideRight }
                    QualityChip { text: Player.qualityLong; scale: Stage.cineNowScale }
                }
                TinyBar {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: times.top
                    anchors.bottomMargin: Stage.menuRowApart * 2 // derived: a hair above the times
                    value: Player.duration > 0 ? Player.position / Player.duration : 0
                    fill: panel.accent
                    enabled: Player.duration > 0 && !Player.live
                    onChanged: (v) => { if (Player.duration > 0 && !Player.live) Player.seek(v * Player.duration) }
                }
                Item {
                    id: times
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    height: Math.max(Stage.cineTimeMin, card.height * 0.07)
                    Text { anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter; text: Player.elapsed; color: panel.accent; opacity: 0.82; font.pointSize: nowPane.timeSize }
                    Text { anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; text: Player.duration > 0 ? Player.clock(Player.duration) : ""; color: Theme.color.immersiveTextQuiet; font.pointSize: nowPane.timeSize }
                }
            }

            Item {
                anchors.fill: parent
                visible: panel.lyricsMode
                LyricLine {
                    visible: panel.hasLine
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width
                    line: panel.feed ? (panel.feed.lines[panel.feed.current] || {}) : ({})
                    synced: panel.feed ? panel.feed.synced : false
                    position: panel.feed ? panel.feed.current : -1
                    current: panel.feed ? panel.feed.current : -1
                    nowMs: panel.feed ? panel.feed.nowMs : 0
                    endMs: panel.feed ? panel.feed.currentEnd : 0
                    translation: panel.feed && panel.feed.translating ? (panel.feed.translation[panel.feed.current] || "") : ""
                    size: Math.max(Stage.cineLineMin, Math.min(Stage.cineLineMax, card.height * Stage.cineTitleShare)) * Type.pointsPerPixel // derived: pixels to points
                    sungColor: Theme.color.immersiveTextStrong
                    dimColor: Theme.color.immersiveLyricInactive
                    dimming: panel.feed ? panel.feed.dimming : "strong"
                    uppercase: panel.feed ? panel.feed.uppercase : false
                    wordByWord: panel.feed ? !panel.feed.lite : true
                    shadowColor: Theme.color.immersiveTextShadow
                }
                Text { visible: !panel.hasLine && panel.feed !== null && panel.feed.waiting; anchors.centerIn: parent; text: "♪"; color: Theme.color.immersiveTextDim; font.pointSize: Math.max(Stage.cineLineMin + Stage.menuRowLead, card.height * 0.13) * Type.pointsPerPixel } // derived: pixels to points
                Text { visible: !panel.hasLine && panel.feed !== null && !panel.feed.waiting && panel.feed.none; anchors.fill: parent; text: I18n.tr("No lyrics", I18n.rev); color: Theme.color.immersiveTextGhost; font.pointSize: Math.max(Stage.cineTimeMax + Stage.chipRound, Math.min(Stage.splitGapMax, card.height * 0.07)) * Type.pointsPerPixel; font.italic: true; wrapMode: Text.WordWrap; verticalAlignment: Text.AlignVCenter } // derived: pixels to points
            }
        }
        Rectangle { anchors.fill: parent; radius: Stage.cineCardRound; color: "transparent"; border.width: Metrics.hairline; border.color: Theme.color.immersiveCardEdge }
        TranslateButton {
            objectName: "immersiveCardTranslate"
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            anchors.margins: Stage.lyricFollowPad
            feed: panel.feed
            shown: panel.lyricsMode
        }
    }
}
