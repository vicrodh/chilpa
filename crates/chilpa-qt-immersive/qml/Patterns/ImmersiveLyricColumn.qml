import QtQuick
import QtQuick.Effects
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/lyrics/qml/Components"
import "../Components"
import "../Theme"

// I anchor the active line while the surrounding verses move past it.
// A drag or wheel leaves the reader in charge until follow is requested.
ListView {
    id: view
    property LyricsFeed feed: null
    property bool live: true
    property bool shaders: true
    property bool follow: true
    property real lineSize: Stage.lineMin * Type.pointsPerPixel
    property var lines: feed ? feed.lines : []
    readonly property int watched: feed ? feed.current : -1
    readonly property bool synced: feed !== null && feed.synced
    readonly property real anchor: height * Stage.lyricAnchor
    readonly property bool gliding: tracker.moving
    function resume() { follow = true; tracker.request(true) }
    function browse() { tracker.browse() }
    onLinesChanged: tracker.reset()
    onLineSizeChanged: tracker.request(false)
    LyricTracker {
        id: tracker
        view: view
        current: view.watched
        following: view.synced && view.follow
        live: view.live
        anchor: Stage.lyricAnchor
        duration: Stage.lineEnter
        onBrowseRequested: view.follow = false
    }
    model: lines
    spacing: Stage.lyricLineApart
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    cacheBuffer: 0
    header: Item { height: view.synced ? view.anchor : 0 }
    footer: Item { height: view.synced ? view.height - view.anchor : 0 }
    delegate: Item {
        id: row
        required property int index
        required property var modelData
        objectName: "immersiveLyricRow"
        readonly property bool active: view.feed !== null && view.synced && index === view.watched
        readonly property real distance: !view.synced || !view.follow ? 0 : Math.abs(index - Math.max(0, view.watched))
        readonly property real softness: Math.min(Stage.lyricBlurMax, Math.max(0, distance - 1) * Stage.lyricBlurStep)
        width: view.width
        height: words.height
        onHeightChanged: tracker.request(false)
        onYChanged: tracker.request(false)
        opacity: active || distance === 0 ? 1 : Math.max(Stage.lyricFarOpacity, Stage.lyricNearOpacity - (distance - 1) * Stage.lyricFadeStep)
        Behavior on opacity { enabled: view.live && !Motion.reduced; NumberAnimation { duration: Stage.lineEnter; easing.type: Easing.OutCubic } }
        layer.enabled: view.live && view.shaders && softness > 0 && view.feed !== null && !view.feed.lite
        layer.effect: MultiEffect { blurEnabled: true; blurMax: Stage.lyricBlurReach; blur: row.softness }
        LyricLine {
            id: words
            objectName: "immersiveLyricText"
            width: row.width
            line: row.modelData
            synced: view.synced
            position: row.index
            current: view.watched
            nowMs: row.active && view.live ? view.feed.nowMs : 0
            endMs: row.active ? view.feed.currentEnd : 0
            size: view.lineSize
            sungColor: Theme.color.immersiveTextStrong
            dimColor: row.active ? Theme.color.immersiveLyricInactive : Theme.color.immersiveTextStrong
            pastColor: Theme.color.immersiveTextStrong
            dimming: "off"
            uppercase: view.feed ? view.feed.uppercase : false
            wordByWord: view.live && view.feed !== null && view.feed.wordTimed && !view.feed.lite
            shadowColor: Theme.color.immersiveTextShadow
            onSought: ms => { if (view.synced) Player.seek(ms / 1000) }
        }
    }
}
