import QtQuick
import QtQuick.Window
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/lyrics/qml/Primitives"

// The lyrics of what plays as the immersive surfaces read them: the
// panel's document, which line is current, where it ends, and the moment
// of the track carried between the player's positions while word-timed
// lyrics play and the window shows. One feed serves the lyric surfaces
// that are on screen; `live` parks the carry when none is.
Item {
    id: feed
    property bool live: true
    readonly property var lyrics: read(Player.lyricsJson)
    readonly property bool found: lyrics !== null && !!lyrics.document
    readonly property var lines: found ? lyrics.document.lines : []
    readonly property bool hasLines: lines.length > 0
    readonly property bool timed: found && ["line", "word", "syllable"].includes(lyrics.document.timing)
    readonly property bool synced: timed && Player.lyricsSynced
    readonly property int current: synced && !waiting ? Player.lyricsLine : -1
    readonly property bool wordTimed: found && (lyrics.document.timing === "word" || lyrics.document.timing === "syllable")
    // The parser has already applied file offsets to these timestamps.
    // I wait for the real clock and sync state, independently of follow.
    readonly property var firstTimedLine: lines.find(line => typeof line.start_ms === "number" && Number.isFinite(line.start_ms)) || null
    readonly property bool waiting: timed && hasLines && (!synced || Player.lyricsLine < 0
        || firstTimedLine === null || nowMs < firstTimedLine.start_ms)
    readonly property bool none: !hasLines
    // The translation the lyrics carry, by line, and whether it shows: asked
    // for from a corner of the lyric surface, put away with its track.
    readonly property var translation: found ? offered.lines : null
    property bool translating: false
    onFoundChanged: if (!found) translating = false
    LyricTranslation { id: offered; lyrics: feed.lyrics }
    readonly property string currentText: current >= 0 && current < lines.length ? (lines[current].text || "") : ""
    readonly property bool uppercase: SettingsModel.kept("ui.lyrics.uppercase") === "true"
    readonly property string dimming: SettingsModel.kept("ui.lyrics.dimming") || "strong"
    readonly property bool lite: SettingsModel.kept("ui.lyrics.lite") === "true"
    readonly property bool windowShowing: feed.Window.window
        ? (feed.Window.window.visibility !== Window.Hidden && feed.Window.window.visibility !== Window.Minimized) : true
    visible: false
    width: 0
    height: 0

    function read(json) {
        if (!json)
            return null
        try {
            return JSON.parse(json)
        } catch (e) {
            return null
        }
    }
    // Where a line stops being sung when its source did not say: where the
    // next one starts, or a few seconds later for the last.
    function lineEnd(index) {
        const line = lines[index]
        if (!line)
            return 0
        if (line.end_ms !== undefined)
            return line.end_ms
        for (let i = index + 1; i < lines.length; ++i)
            if (lines[i].start_ms > line.start_ms)
                return lines[i].start_ms
        return line.start_ms + Motion.lyricLastLine
    }
    readonly property real currentEnd: lineEnd(current)

    property real nowMs: Player.position * 1000
    property double anchoredAt: Date.now()
    Connections {
        target: Player
        function onPositionChanged() {
            feed.anchoredAt = Date.now()
            feed.nowMs = Player.position * 1000
        }
    }
    FrameAnimation {
        running: feed.live && feed.windowShowing && feed.wordTimed && !feed.lite && Player.playing && !Player.paused
        onTriggered: feed.nowMs = Player.position * 1000 + Math.min(Date.now() - feed.anchoredAt, Motion.lyricCarry)
    }
}
