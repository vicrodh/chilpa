import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/lyrics/qml/Components"
import "../Components"
import "../Theme"

// Focus 4, Lyrics (03-immersive §1.4): one line at a time, centered, at
// max(30, min(width * 0.026, 48)) px, white with the unsung part at 35 %,
// and on every change the new line rises 24 px into place over 500 ms.
// Before the first line of a synced document a note waits; with no lines,
// "No lyrics" says so.
Item {
    id: panel
    property LyricsFeed feed: null
    readonly property bool hasLine: feed !== null && feed.currentText !== ""
    readonly property real lineSize: Math.round(Math.max(Stage.lineMin, Math.min(panel.width * Stage.lineShare, Stage.lineMax))) * Type.pointsPerPixel // derived: QBZ's pixels to points

    readonly property int watched: feed !== null ? feed.current : -1
    property bool entering: false
    onWatchedChanged: {
        // Snap to the start pose, then let go a tick later so the motion
        // runs forward.
        entering = true
        enter.restart()
    }
    Timer { id: enter; interval: Stage.menuRowApart * 16; onTriggered: panel.entering = false } // derived: one frame

    Item {
        anchors.fill: parent
        anchors.topMargin: Stage.lyricsPadTop
        anchors.leftMargin: Stage.lyricsPadSide
        anchors.rightMargin: Stage.lyricsPadSide
        anchors.bottomMargin: Stage.lyricsPadBottom
        Item {
            id: line
            visible: panel.hasLine
            anchors.centerIn: parent
            anchors.verticalCenterOffset: panel.entering ? Stage.lineRise : 0
            width: Math.min(parent.width, panel.width * Stage.lyricsWideShare)
            height: row.height
            opacity: panel.entering ? 0 : 1
            Behavior on anchors.verticalCenterOffset { enabled: !panel.entering; NumberAnimation { duration: Motion.reduced ? 0 : Stage.lineEnter; easing.type: Easing.Bezier; easing.bezierCurve: [0.22, 1, 0.36, 1] } }
            Behavior on opacity { enabled: !panel.entering; NumberAnimation { duration: Motion.reduced ? 0 : Stage.lineEnter; easing.type: Easing.Bezier; easing.bezierCurve: [0.22, 1, 0.36, 1] } }
            LyricLine {
                id: row
                width: parent.width
                line: panel.feed ? (panel.feed.lines[panel.feed.current] || {}) : ({})
                synced: panel.feed ? panel.feed.synced : false
                position: panel.feed ? panel.feed.current : -1
                current: panel.feed ? panel.feed.current : -1
                nowMs: panel.feed ? panel.feed.nowMs : 0
                endMs: panel.feed ? panel.feed.currentEnd : 0
                size: panel.lineSize
                sungColor: Theme.color.immersiveTextStrong
                dimColor: Theme.color.immersiveLyricInactive
                dimming: panel.feed ? panel.feed.dimming : "strong"
                uppercase: panel.feed ? panel.feed.uppercase : false
                wordByWord: panel.feed ? !panel.feed.lite : true
                centered: true
                shadowColor: Theme.color.immersiveTextShadow
            }
        }
        Text {
            visible: !panel.hasLine && panel.feed !== null && panel.feed.waiting
            anchors.centerIn: parent
            text: "♪"
            color: Theme.color.immersiveTextStrong
            opacity: Stage.noteOpacity
            font.pointSize: Stage.noteText
        }
        Text {
            objectName: "immersiveNoLyrics"
            visible: !panel.hasLine && panel.feed !== null && !panel.feed.waiting && panel.feed.none
            anchors.centerIn: parent
            width: parent.width
            text: I18n.tr("No lyrics", I18n.rev)
            color: Theme.color.immersiveTextGhost
            font.pointSize: Stage.noLyricsText
            font.italic: true
            horizontalAlignment: Text.AlignHCenter
        }
    }
}
