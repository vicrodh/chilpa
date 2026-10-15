import QtQuick
import QtQuick.Window
import QtTest
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Components"
import "../../qml/Patterns"
import "../../qml/Theme"

TestCase {
    id: test
    name: "ImmersiveFocusPanels"
    when: windowShown
    visible: true
    width: 1100
    height: 760
    property var deck: null
    property var lyrics: null
    TrackListModel { id: queueRows }
    TrackListModel { id: historyRows }
    QtObject {
        id: nativeRows
        readonly property int count: queueRows.count
        signal modelReset()
        signal dataChanged()
        function playingRow() { return queueRows.playingRow() }
        function index(row, column) { return row }
        function data(index, role) { return role === 269 ? queueRows.get(index).trackId : role === 271 ? queueRows.get(index).art : null }
        function play(row) { queueRows.play(row) }
    }
    LyricsFeed { id: lyricFeed; live: false }
    Component { id: deckMaker; CoverFlowDeck { width: test.width; height: 460; ahead: queueRows; behind: historyRows; artSize: 300; currentPath: "/one"; shaders: false } }
    Component { id: lyricsMaker; LyricsFocusPanel { width: test.width; height: test.height; feed: lyricFeed } }
    function art(id) {
        return "data:image/svg+xml," + encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><rect width="64" height="64" fill="hsl(' + id * 70 + ',70%,50%)"/></svg>')
    }
    function rows(first) {
        queueRows.clear()
        for (let i = first; i <= 8; ++i) queueRows.append({trackId:i, art:art(i), playing:i === first})
        historyRows.clear()
        for (let i = first - 1; i > 0; --i) historyRows.append({trackId:i, art:art(i), playing:false})
    }
    function cover(id) { return deck.covers().find(c => c.occupied && c.identity === "track:" + id) }
    function init() {
        Motion.reduced = false
        Player.lyricsJson = ""
        Player.lyricsSynced = false
        Player.lyricsLine = -1
        Player.position = 0
        Player.path = "/test.wav"
        Player.paused = true
        rows(1)
    }
    function cleanup() { Motion.reduced = false; if (deck) deck.destroy(); if (lyrics) lyrics.destroy(); deck = null; lyrics = null; wait(0) }
    function makeDeck() { deck = deckMaker.createObject(test); tryVerify(() => cover(2) !== undefined && cover(2).ready); return deck }
    function test_cover_instances_and_decode_survive_the_next_track_and_model_reset() {
        makeDeck()
        const first = cover(1), next = cover(2)
        const tile = findChild(next, "flowArtwork"), decode = tile.decode, source = tile.source.toString()
        rows(2); deck.currentPath = "/two"; deck.currentArt = art(2)
        wait(60)
        compare(cover(2), next)
        compare(cover(1), first)
        verify(next.offset > 0 && next.offset < 1, "the next cover slides from its existing pose")
        compare(tile.decode, decode, "the animation transforms a single decode")
        compare(tile.source.toString(), source)
        tryCompare(next, "offset", 0)
        compare(first.destination, -1)
        compare(deck.covers().length, Stage.flowNeighbours * 2 + 3)
    }
    function test_previous_and_rapid_next_keep_the_existing_images() {
        rows(3); makeDeck()
        const previous = cover(2)
        rows(2); deck.currentPath = "/two"
        wait(60)
        compare(cover(2), previous)
        verify(previous.offset < 0 && previous.offset > -1, "the previous cover slides from the left")
        rows(3); deck.currentPath = "/three"
        wait(20)
        const next = cover(4)
        rows(4); deck.currentPath = "/four"
        wait(20)
        compare(cover(4), next)
        tryCompare(next, "offset", 0)
    }
    function test_hidden_and_reduced_motion_stop_the_fan() {
        makeDeck()
        const next = cover(2)
        deck.live = false
        rows(2); deck.currentPath = "/two"
        wait(40)
        compare(next.destination, 1, "hidden models do not update the images")
        Motion.reduced = true
        deck.live = true
        tryCompare(next, "destination", 0)
        compare(next.offset, 0)
    }
    function test_native_model_roles_use_the_same_bounded_pool() {
        makeDeck()
        compare(Stage.flowTrackRole, 269)
        compare(Stage.flowArtRole, 271)
        deck.ahead = nativeRows
        deck.reconcile()
        compare(cover(2).destination, 1)
        compare(decodeURIComponent(cover(2).artwork.toString()), decodeURIComponent(art(2)))
    }
    function lyricDocument(timing) {
        return JSON.stringify({document:{timing:timing,lines:Array.from({length:14}, (_, i) => ({text:"A verse with room to breathe " + i,start_ms:i * 3000,end_ms:(i + 1) * 3000,
            segments: timing === "word" ? [{text:"A verse",start_ms:i * 3000,end_ms:i * 3000 + 1400,space_after:true},{text:"with room to breathe " + i,start_ms:i * 3000 + 1400,end_ms:(i + 1) * 3000}] : []}))}})
    }
    function makeLyrics(timing) {
        Player.lyricsJson = lyricDocument(timing)
        Player.lyricsSynced = true
        Player.lyricsLine = 4
        lyrics = lyricsMaker.createObject(test)
        const column = findChild(lyrics, "immersiveLyricColumn")
        tryVerify(() => column.itemAtIndex(4) !== null)
        return column
    }
    function test_context_lines_follow_then_allow_manual_reading_and_return() {
        const column = makeLyrics("line")
        const row = column.itemAtIndex(4)
        verify(row.active)
        verify(!findChild(row, "immersiveLyricText").wordByWord, "line timings never invent word progress")
        verify(column.itemAtIndex(3) !== null && column.itemAtIndex(5) !== null)
        Player.lyricsLine = 5
        tryVerify(() => column.gliding)
        tryVerify(() => !column.gliding)
        const before = column.contentY
        column.browse()
        Player.lyricsLine = 6
        wait(40)
        compare(column.contentY, before)
        verify(findChild(lyrics, "immersiveLyricsFollow").visible)
        column.resume()
        tryVerify(() => Math.abs(column.contentY - before) > 10 && !column.gliding)
        const active = column.itemAtIndex(6)
        const center = active.y - column.contentY + active.height / 2
        verify(Math.abs(center - column.anchor) < 2, "the active line returns to its anchor")
    }
    function test_words_need_real_segments_and_hidden_lyrics_stop_scrolling() {
        const column = makeLyrics("word")
        verify(findChild(column.itemAtIndex(4), "immersiveLyricText").wordByWord)
        lyrics.visible = false
        Player.lyricsLine = 5
        wait(40)
        verify(!column.gliding)
        lyrics.visible = true
        Motion.reduced = true
        Player.lyricsLine = 6
        wait(20)
        verify(!column.gliding)
    }
    function delayedDocument(timing, start) {
        return JSON.stringify({document:{timing:timing, lines:[{text:"A delayed first verse",start_ms:start},{text:"The next verse",start_ms:start + 3000}],
            layers:[{kind:"translation",language:"es",lines:[{line:0,text:"A translation that must wait too"}]}]},translation:0})
    }
    function delayedLyrics(timing, start) {
        Player.paused = true
        Player.lyricsJson = delayedDocument(timing, start)
        Player.lyricsSynced = timing !== "unsynced"
        Player.lyricsLine = 0
        lyrics = lyricsMaker.createObject(test)
        return findChild(lyrics, "immersiveLyricColumn")
    }
    function prelude(column, waiting) {
        compare(lyricFeed.waiting, waiting)
        compare(findChild(column, "immersiveLyricsPrelude").visible, waiting)
        if (waiting) {
            compare(column.count, 0, "neither original nor translated rows exist before the first real timestamp")
            compare(lyricFeed.current, -1)
            verify(!findChild(lyrics, "immersiveLyricsFollow").visible)
        }
    }
    function test_timed_prelude_uses_the_clock_despite_a_clamped_index_and_manual_follow() {
        // A 10-second LRC line with +3000 ms offset reaches the UI at 7000 ms.
        const column = delayedLyrics("line", 7000)
        Player.position = 6.999
        prelude(column, true)
        column.follow = false
        prelude(column, true)
        column.resume()
        prelude(column, true)
        Player.position = 7
        prelude(column, false)
        tryCompare(column, "count", 2)
        tryVerify(() => column.itemAtIndex(0) !== null)
        verify(column.itemAtIndex(0).active)
        Player.position = 2
        prelude(column, true)
        Player.position = 7.001
        prelude(column, false)
    }
    function test_document_replacement_translation_and_native_sync_arrival_keep_the_prelude() {
        const column = delayedLyrics("word", 10000)
        Player.lyricsSynced = false
        Player.position = 11
        prelude(column, true)
        Player.lyricsSynced = true
        Player.lyricsLine = -1
        prelude(column, true)
        Player.lyricsLine = 0
        prelude(column, false)
        Player.lyricsJson = delayedDocument("line", 15000)
        prelude(column, true)
        const changed = JSON.parse(Player.lyricsJson)
        changed.document.layers[0].lines[0].text += " with a late replacement"
        Player.lyricsJson = JSON.stringify(changed)
        prelude(column, true)
        Player.path = "/next-track.wav"
        Player.position = 0
        Player.lyricsJson = delayedDocument("syllable", 4000)
        prelude(column, true)
        Player.position = 4
        prelude(column, false)
    }
    function test_plain_lyrics_keep_normal_reading_without_starting_sync() {
        const column = delayedLyrics("unsynced", 20000)
        prelude(column, false)
        tryCompare(column, "count", 2)
        compare(lyricFeed.current, -1)
        column.resume()
        verify(!lyricFeed.synced)
        compare(lyricFeed.current, -1)
        compare(column.count, 2)
    }

}
