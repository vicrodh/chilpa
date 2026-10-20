import QtQuick
import QtQuick.Effects
import app.chilpa
import app.chilpa.immersive
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/lyrics/qml/Components"
import "../Components"
import "../Theme"

// The split (03-immersive §1.1): two halves of (width - 160) / 2 between
// margins of 56 at the sides, 56 above and 132 below, 48 apart. Left, the
// cover at max(240, min(half, height - 200, its own width)) with a shadow
// and the track's block 28 below; right, on a plate inset 20, the lines
// of lyrics (panel 0, with a click to seek) or the queue (panel 3).
Item {
    id: split
    property bool shaders: true
    property LyricsFeed feed: null
    readonly property real halfWide: (split.width - Stage.splitColumnsLess) / 2 // derived: two halves
    readonly property real columnTall: split.height - Stage.splitPadTop - Stage.splitPadBottom
    Image { id: probe; visible: false; asynchronous: true; source: split.visible ? Player.art : "" }
    readonly property real ownWidth: probe.status === Image.Ready ? probe.implicitWidth : 0
    readonly property real artSize: Math.max(Stage.splitArtMin, Math.min(halfWide, columnTall - Stage.splitArtLess, ownWidth))

    Row {
        anchors.fill: parent
        anchors.topMargin: Stage.splitPadTop
        anchors.bottomMargin: Stage.splitPadBottom
        anchors.leftMargin: Stage.splitPadSide
        anchors.rightMargin: Stage.splitPadSide
        spacing: Stage.splitApart
        Item {
            width: split.halfWide
            height: parent.height
            Column {
                anchors.centerIn: parent
                width: parent.width
                spacing: Stage.splitMetaApart
                Item {
                    width: parent.width
                    height: split.artSize
                    Rectangle {
                        x: (parent.width - width) / 2 // derived: centered
                        y: Stage.splitShadowDrop
                        width: split.artSize
                        height: split.artSize
                        radius: Stage.splitArtRound
                        color: Theme.color.immersiveShadowDeep
                        layer.enabled: split.shaders
                        layer.effect: MultiEffect { blurEnabled: true; blurMax: Stage.blurReach; blur: 0.625 }
                    }
                    Rectangle { visible: Player.art.toString() === ""; x: (parent.width - width) / 2; width: split.artSize; height: split.artSize; radius: Stage.splitArtRound; color: Theme.color.immersivePlaceholder } // derived: centered
                    ArtTile { visible: Player.art.toString() !== ""; x: (parent.width - width) / 2; width: split.artSize; height: split.artSize; radius: Stage.splitArtRound; edged: false; source: Player.art } // derived: centered
                }
                TrackMeta { anchors.horizontalCenter: parent.horizontalCenter; width: parent.width - Stage.splitMetaLess }
            }
        }
        Item {
            width: split.halfWide
            height: parent.height
            clip: true
            Item {
                anchors.fill: parent
                anchors.margins: Stage.splitPlatePad
                clip: true
                LyricLines {
                    objectName: "immersiveSplitLyrics"
                    anchors.fill: parent
                    visible: Immersive.splitPanel === 0 && (!split.feed || !split.feed.waiting)
                    lines: split.feed ? split.feed.lines : []
                    synced: split.feed ? split.feed.synced : false
                    current: split.feed ? split.feed.current : -1
                    nowMs: split.feed ? split.feed.nowMs : 0
                    sungEnd: split.feed ? split.feed.currentEnd : 0
                    translation: split.feed && split.feed.translating ? split.feed.translation : null
                    size: Math.round(Math.max(Stage.splitLineMin, Math.min(split.width * Stage.splitLineShare, Stage.splitLineMax))) * Type.pointsPerPixel // derived: QBZ's pixels to points
                    sungColor: Theme.color.immersiveTextStrong
                    dimColor: Theme.color.immersiveLyricInactive
                    pastColor: Theme.color.immersiveLyricInactive
                    dimming: split.feed ? split.feed.dimming : "strong"
                    uppercase: split.feed ? split.feed.uppercase : false
                    lite: split.feed ? split.feed.lite : false
                    onSought: (ms) => { if (Player.duration > 0) Player.seek(ms / 1000) }
                }
                Icon {
                    objectName: "immersiveSplitPrelude"
                    anchors.centerIn: parent
                    visible: Immersive.splitPanel === 0 && split.feed !== null && split.feed.waiting
                    glyph: Icons.music
                    size: Math.round(Stage.noteText / Type.pointsPerPixel)
                    color: Theme.color.immersiveTextDim
                }
                Loader {
                    anchors.fill: parent
                    active: Immersive.splitPanel === 3
                    visible: active
                    sourceComponent: QueueTabsPanel { centered: true }
                }
            }
            TranslateButton {
                objectName: "immersiveSplitTranslate"
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                anchors.margins: Stage.splitPlatePad
                feed: split.feed
                shown: Immersive.splitPanel === 0
            }
        }
    }
}
