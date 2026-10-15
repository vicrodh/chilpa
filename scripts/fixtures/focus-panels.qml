import QtQuick
import QtQuick.Window
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/immersive/qml/Components" as Parts
import "qrc:/qt/qml/app/chilpa/immersive/qml/Patterns" as Patterns

Window {
    id: window
    width: 1100
    height: 760
    visible: true
    color: "#101820"
    property string outputDirectory: "OUTPUT_DIRECTORY"
    property string expectedBackend: "EXPECTED_BACKEND"
    property var trackPaths: "TRACK_PATHS"
    property var trackIds: "TRACK_IDS"
    readonly property bool playing: Player.playing
    property int queueResets: 0
    property int historyResets: 0
    property var resetBaseline: [0, 0]
    property var transitionResets: ({})
    property string phase: "startup"
    property int ticks: 0
    property int phaseTicks: 0
    property double segmentStarted: 0
    property var remembered: ({})
    property bool capturing: false
    property int serial: 0
    property int nextSamples: 0
    property int backSamples: 0
    property double lastSample: 0
    property bool lyricMode: false
    property string lyricTiming: "word"
    property int lyricIndex: 4
    property real lyricShift: 0
    property real lyricPosition: 12.3
    property real firstProgress: 0
    property var lyricSpot: null
    property bool sawGlide: false

    Rectangle { anchors.fill: parent; color: window.color; z: -1 }
    TrackListModel {
        id: queueRows
        kind: "queue"
        filterRev: Player.queueRev + ListenList.revision
        playingPath: Player.path
    }
    TrackListModel {
        id: historyRows
        kind: "history"
        filterRev: Player.queueRev + ListenList.revision
        playingPath: Player.path
    }
    Connections { target: queueRows; function onModelReset() { ++window.queueResets } }
    Connections { target: historyRows; function onModelReset() { ++window.historyResets } }
    Parts.CoverFlowDeck {
        id: deck
        anchors.fill: parent
        ahead: queueRows
        behind: historyRows
        currentPath: Player.path
        currentArt: Player.art
        artSize: 340
        visible: !window.lyricMode
        live: visible
    }
    Parts.LyricsFeed { id: lyricFeed; live: window.lyricMode }
    Patterns.ImmersiveLyricColumn {
        id: column
        x: 180
        y: 36
        width: 740
        height: window.height - 72
        lineSize: 30
        feed: lyricFeed
        visible: window.lyricMode
        live: visible
    }
    // The visual fixture owns this document and its paused clock. No lyric
    // provider, parser or playback synchronization is claimed by this stage.
    Connections {
        target: Player
        function onLyricsJsonChanged() { if (window.lyricMode) window.injectLyrics() }
        function onLyricsSyncedChanged() { if (window.lyricMode) window.injectLyrics() }
        function onLyricsLineChanged() { if (window.lyricMode) window.injectLyrics() }
        function onPositionChanged() { if (window.lyricMode) window.injectLyrics() }
        function onPausedChanged() { if (window.lyricMode) window.injectLyrics() }
    }
    function fail(message) {
        console.error("SMOKE FAIL focus panels: " + message)
        Qt.exit(1)
        throw new Error(message)
    }
    function check(value, message) { if (!value) fail(message) }
    function enter(value) { phase = value; phaseTicks = 0; segmentStarted = 0 }
    function audioReady(index) {
        if (!Player.playing || Player.paused || Player.path !== trackPaths[index]) {
            segmentStarted = 0
            return false
        }
        if (!segmentStarted) segmentStarted = Date.now()
        // Previous reopens and truncates the capture sink. Give every actual
        // playback segment enough paced PCM, including the final opening.
        return Date.now() - segmentStarted >= 1250
    }
    function child(item, name) {
        if (item.objectName === name) return item
        for (const nested of item.children || []) {
            const found = child(nested, name)
            if (found) return found
        }
        return null
    }
    function cover(id) { return deck.covers().find(card => card.occupied && card.identity === "track:" + id) }
    function image(tile) { return tile.children.find(item => item.sourceSize !== undefined && item.status !== undefined) }
    function remember() {
        const saved = {}
        for (const id of trackIds) {
            const card = cover(id), tile = child(card, "flowArtwork"), decoded = image(tile)
            check(decoded && tile.ready, "native cover did not decode")
            saved[id] = {card:card, tile:tile, image:decoded, decode:tile.decode, source:decoded.source.toString(),
                         width:decoded.sourceSize.width, height:decoded.sourceSize.height}
        }
        remembered = saved
    }
    function stable() {
        for (const id of trackIds) {
            const before = remembered[id], card = cover(id)
            check(card === before.card, "model reset replaced the cover instance for " + id)
            const tile = child(card, "flowArtwork"), decoded = image(tile)
            check(tile === before.tile && decoded === before.image, "model reset replaced an Image for " + id)
            check(tile.ready && decoded.status === Image.Ready, "loaded image blinked or restarted for " + id)
            check(tile.decode === before.decode && decoded.source.toString() === before.source
                && decoded.sourceSize.width === before.width && decoded.sourceSize.height === before.height,
                "animation changed the source or decode size for " + id)
        }
    }
    function moving() { return trackIds.some(id => Math.abs(cover(id).offset - cover(id).destination) > 0.01) }
    function sampleMotion() {
        if (capturing || Date.now() - lastSample < 38) return
        const cards = trackIds.map(id => cover(id)).sort((a, b) => Math.abs(a.offset) - Math.abs(b.offset))
        const top = cards[0]
        const point = top.mapToItem(deck, top.width / 2, top.height / 2)
        const index = trackIds.findIndex(id => top.identity === "track:" + id)
        const record = {phase:phase, file:phase + "-" + (++serial) + ".png", x:point.x, y:point.y,
                        channel:index, offset:top.offset, destination:top.destination, identity:top.identity}
        capturing = true
        lastSample = Date.now()
        check(deck.grabToImage(result => {
            check(result.saveToFile(outputDirectory + "/" + record.file), "cannot save animation capture")
            if (record.phase === "next") ++nextSamples
            else ++backSamples
            console.log("SMOKE focus sample " + JSON.stringify(record))
            capturing = false
        }), "cannot request animation capture")
    }
    function captureLyrics(name, nextPhase) {
        capturing = true
        check(column.grabToImage(result => {
            check(result.saveToFile(outputDirectory + "/" + name + ".png"), "cannot save lyric capture")
            if (name === "lyrics-word-a") {
                check(window.contentItem.grabToImage(composed => {
                    check(composed.saveToFile(outputDirectory + "/lyrics-word-a-composited.png"), "cannot save composed lyric capture")
                    capturing = false
                    enter(nextPhase)
                }), "cannot request composed lyric capture")
            } else {
                capturing = false
                enter(nextPhase)
            }
        }), "cannot request lyric capture")
    }
    function document() {
        return JSON.stringify({document:{timing:lyricTiming, lines:Array.from({length:14}, (_, i) => ({
            text:"A verse with room to breathe " + i,
            start_ms:lyricTiming === "unsynced" ? undefined : lyricShift + i * 3000,
            end_ms:lyricTiming === "unsynced" ? undefined : lyricShift + (i + 1) * 3000,
            segments:lyricTiming === "word" ? [
                {text:"A verse", start_ms:lyricShift + i * 3000, end_ms:lyricShift + i * 3000 + 1400, space_after:true},
                {text:"with room to breathe " + i, start_ms:lyricShift + i * 3000 + 1400, end_ms:lyricShift + (i + 1) * 3000}
            ] : []
        })), layers:[{kind:"translation",language:"es",lines:[{line:0,text:"The translation also waits"}]}]},translation:0})
    }
    function injectLyrics() {
        const value = document()
        if (Player.lyricsJson !== value) Player.lyricsJson = value
        const synced = lyricTiming !== "unsynced"
        if (Player.lyricsSynced !== synced) Player.lyricsSynced = synced
        if (Player.lyricsLine !== lyricIndex) Player.lyricsLine = lyricIndex
        if (Player.position !== lyricPosition) Player.position = lyricPosition
        if (!Player.paused) Player.paused = true
    }
    function centered() {
        const row = column.itemAtIndex(lyricIndex)
        return row && !column.gliding && Math.abs(row.y - column.contentY + row.height / 2 - column.anchor) < 2
    }
    function lyricWords() { return child(column.itemAtIndex(lyricIndex), "sungLine") }
    FrameAnimation {
        running: window.phase === "next" || window.phase === "back"
        onTriggered: {
            window.stable()
            const target = window.phase === "next" ? window.trackIds[1] : window.trackIds[0]
            if (window.cover(target).destination !== 0) return
            if (window.moving()) window.sampleMotion()
        }
    }
    Timer {
        interval: 40
        repeat: true
        running: true
        onTriggered: {
            ++window.ticks
            ++window.phaseTicks
            if (window.ticks > 1100) return window.fail("timed out in " + window.phase)
            ListenList.refresh()
            if (window.capturing) return
            if (window.phase === "startup") {
                const expected = window.expectedBackend === "vulkan" ? GraphicsInfo.Vulkan : GraphicsInfo.OpenGL
                if (window.GraphicsInfo.api !== expected) {
                    if (window.phaseTicks > 50) window.fail("requested GPU backend was not selected")
                    return
                }
                Motion.reduced = false
                if (queueRows.count !== 3) return
                ListenList.command("play", "1", "")
                window.enter("playing")
            } else if (window.phase === "playing") {
                if (!window.audioReady(0)) return
                if (!window.trackIds.every(id => window.cover(id) && window.cover(id).ready)) return
                window.remember()
                window.check(queueRows.count === 3 && historyRows.count === 0, "initial native queue/history mismatch")
                window.resetBaseline = [window.queueResets, window.historyResets]
                Player.next()
                window.enter("next")
            } else if (window.phase === "next" || window.phase === "back") {
                const forward = window.phase === "next", index = forward ? 1 : 0
                if (!window.audioReady(index)) return
                if (Player.path !== window.trackPaths[index] || window.cover(window.trackIds[index]).destination !== 0 || window.moving()) return
                window.stable()
                window.check(window.queueResets > window.resetBaseline[0] && window.historyResets > window.resetBaseline[1],
                             "native transport did not reset both changed queue/history models")
                window.transitionResets[window.phase] = {queue:window.queueResets - window.resetBaseline[0],
                    history:window.historyResets - window.resetBaseline[1]}
                window.check((forward ? window.nextSamples : window.backSamples) >= 3, "too few captures during animation")
                window.check(queueRows.count === (forward ? 2 : 3) && historyRows.count === (forward ? 1 : 0), "native history did not follow transport")
                window.check(window.trackIds.every((id, i) => window.cover(id).destination === i - index), "settled native cover destinations differ")
                if (forward) {
                    window.resetBaseline = [window.queueResets, window.historyResets]
                    Player.previous()
                    window.enter("back")
                } else {
                    Player.clearQueue()
                    window.enter("stopping")
                }
            } else if (window.phase === "stopping") {
                if (Player.playing || queueRows.count) return
                window.lyricMode = true
                // The normalized first timestamp includes the parser's file offset.
                window.lyricShift = 7000
                window.lyricIndex = 0
                window.lyricPosition = 6.999
                window.injectLyrics()
                window.enter("lyrics-wait")
            } else if (window.phase === "lyrics-wait") {
                if (window.phaseTicks < 6) return
                const note = window.child(column, "immersiveLyricsPrelude")
                window.check(lyricFeed.waiting && lyricFeed.current === -1 && column.count === 0 && note && note.visible,
                    "timed prelude showed lyrics before the real timestamp despite index zero")
                column.resume()
                window.check(lyricFeed.waiting && column.count === 0, "manual follow invented synchronization")
                window.captureLyrics("lyrics-wait", "lyrics-start")
            } else if (window.phase === "lyrics-start") {
                window.lyricPosition = 7
                window.injectLyrics()
                window.enter("lyrics-started")
            } else if (window.phase === "lyrics-started") {
                if (!window.centered() || window.phaseTicks < 6) return
                window.check(!lyricFeed.waiting && column.count === 14 && column.itemAtIndex(0).active,
                    "the first verse did not appear at its normalized timestamp")
                window.lyricPosition = 1
                window.injectLyrics()
                window.enter("lyrics-rewind")
            } else if (window.phase === "lyrics-rewind") {
                if (window.phaseTicks < 4) return
                window.check(lyricFeed.waiting && column.count === 0 && window.child(column, "immersiveLyricsPrelude").visible,
                    "seeking before the start exposed original or translated lyrics")
                window.lyricTiming = "unsynced"
                window.lyricIndex = -1
                window.injectLyrics()
                window.enter("lyrics-plain")
            } else if (window.phase === "lyrics-plain") {
                if (window.phaseTicks < 4) return
                window.check(!lyricFeed.waiting && !lyricFeed.synced && column.count === 14,
                    "plain lyrics lost their normal reading view")
                window.lyricTiming = "line"
                window.lyricShift = 9000
                window.lyricIndex = 0
                window.injectLyrics()
                window.enter("lyrics-document")
            } else if (window.phase === "lyrics-document") {
                if (window.phaseTicks < 4) return
                window.check(lyricFeed.waiting && column.count === 0, "a replacement timed document leaked its text")
                window.lyricTiming = "word"
                window.lyricShift = 0
                window.lyricIndex = 4
                window.lyricPosition = 12.3
                window.injectLyrics()
                window.enter("lyrics-a")
            } else if (window.phase === "lyrics-a") {
                if (!window.centered() || window.phaseTicks < 12) return
                window.check(!Player.playing && Player.paused, "lyric visual fixture requires idle output and a paused clock")
                window.check(column.itemAtIndex(3) && column.itemAtIndex(5), "active lyric has no surrounding context")
                const words = window.lyricWords()
                window.check(words && words.timed && words.text === "A verse with room to breathe 4", "real word segments were not composed")
                window.check(Math.abs(lyricFeed.nowMs - 12300) < 1, "paused fixture clock did not reach the feed")
                window.firstProgress = words.progress(words.groups[0], lyricFeed.nowMs)
                window.check(window.firstProgress > 0 && window.firstProgress < 1, "first word is not partially sung")
                window.lyricSpot = words.spot(1)
                window.captureLyrics("lyrics-word-a", "lyrics-change")
            } else if (window.phase === "lyrics-change") {
                window.lyricPosition = 13.9
                window.injectLyrics()
                window.enter("lyrics-b")
            } else if (window.phase === "lyrics-b") {
                if (window.phaseTicks < 8) return
                const words = window.lyricWords(), spot = words.spot(1)
                window.check(words.progress(words.groups[0], lyricFeed.nowMs) === 1
                    && words.progress(words.groups[1], lyricFeed.nowMs) > 0, "explicit word intervals did not progress")
                window.check(spot.x === window.lyricSpot.x && spot.y === window.lyricSpot.y, "word highlighting moved the text layout")
                window.captureLyrics("lyrics-word-b", "lyrics-line-change")
            } else if (window.phase === "lyrics-line-change") {
                window.lyricTiming = "line"
                window.injectLyrics()
                window.enter("lyrics-line")
            } else if (window.phase === "lyrics-line") {
                if (!window.centered() || window.phaseTicks < 8) return
                const line = window.child(column.itemAtIndex(4), "immersiveLyricText")
                window.check(!line.wordByWord && !window.lyricWords(), "line-only timing invented word progress")
                window.captureLyrics("lyrics-line", "lyrics-advance")
            } else if (window.phase === "lyrics-advance") {
                window.lyricIndex = 5
                window.lyricPosition = 15.4
                window.injectLyrics()
                window.enter("lyrics-follow")
            } else if (window.phase === "lyrics-follow") {
                if (column.gliding) window.sawGlide = true
                if (!window.centered() || window.phaseTicks < 8) return
                window.check(window.sawGlide, "adjacent lyric change did not glide")
                window.captureLyrics("lyrics-follow", "done")
            } else if (window.phase === "done") {
                console.log("SMOKE focus evidence " + JSON.stringify({native_models:true, native_transport:true,
                    preserved_images:window.trackIds.length, transition_model_resets:window.transitionResets,
                    reset_scope:"Native transport changes queue/history; reset timing is not asserted during animation",
                    next_frames:window.nextSamples,
                    back_frames:window.backSamples, lyrics_context:true, lyrics_centered:true, words_timed:true,
                    line_timing_no_words:true, lyric_glide:window.sawGlide, timed_prelude:true, prelude_seek:true,
                    plain_reading:true, document_replacement:true, translated_prelude:true, normalized_first_ms:7000,
                    injected_document:true, injected_clock:true}))
                console.log("SMOKE focus panels completed")
                Qt.quit()
            }
        }
    }
}
