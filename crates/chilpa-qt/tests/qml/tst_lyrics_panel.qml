import QtQuick
import QtTest
import com.blitzfc.chilpa
import "../../qml/Patterns"
import "../../qml/Theme"

TestCase {
    name: "LyricsPanel"
    when: windowShown
    visible: true
    width: 420
    height: 600

    LyricsPanel { id: panel; width: 400; height: 580 }

    // Two lines timed by word, the first with a word split in syllables.
    readonly property string words: JSON.stringify({ source: "LRCLIB", document: { schema: 1, timing: "syllable", lines: [
        { start_ms: 1000, end_ms: 3000, text: "Hello world", segments: [
            { text: "Hel", start_ms: 1000, end_ms: 1500, space_after: false },
            { text: "lo", start_ms: 1500, end_ms: 2000, space_after: true },
            { text: "world", start_ms: 2000, end_ms: 3000, space_after: false } ] },
        { start_ms: 4000, text: "Again" } ] } })
    readonly property string lines: JSON.stringify({ source: "File", document: { schema: 1, timing: "line", lines: [
        { start_ms: 1000, text: "One two three" }, { start_ms: 5000, text: "Four" } ] } })

    function show(json, synced, line) {
        Player.lyricsJson = json
        Player.lyrics = json ? JSON.parse(json).document.lines.map(l => l.text) : []
        Player.lyricsSynced = synced
        Player.lyricsLine = line
    }

    function init() {
        Player.playing = true
        Player.paused = false
        Player.position = 0
        show("", false, -1)
    }

    function test_only_the_sung_line_is_drawn_word_by_word() {
        show(words, true, 0)
        Player.position = 1.25
        const sung = findChild(panel, "sungLine")
        verify(sung, "the line being sung is drawn word by word")
        compare(sung.groups.length, 2, "two syllables make one word")
        compare(sung.groups[0].text, "Hello")
    }

    function test_a_word_fills_by_its_letters_as_it_is_sung() {
        show(words, true, 0)
        const sung = findChild(panel, "sungLine")
        const hello = sung.groups[0]
        compare(sung.progress(hello, 900), 0, "not yet")
        fuzzyCompare(sung.progress(hello, 1250), 0.3, 1e-9, "half of the three letters of Hel")
        fuzzyCompare(sung.progress(hello, 1500), 0.6, 1e-9, "Hel sung, lo starting")
        compare(sung.progress(hello, 2500), 1)
        fuzzyCompare(sung.progress(sung.groups[1], 2500), 0.5, 1e-9)
    }

    function test_a_line_timed_as_a_whole_lights_up_word_after_word() {
        show(lines, true, 0)
        const sung = findChild(panel, "sungLine")
        verify(sung)
        verify(!sung.timed, "nothing pretends to know when each word is sung")
        compare(sung.groups.map(g => g.text), ["One", "two", "three"])
    }

    function test_the_clock_moves_between_positions_but_not_far() {
        show(words, true, 0)
        Player.position = 1
        wait(120)
        verify(panel.nowMs > 1000, "it moved on its own")
        verify(panel.nowMs <= 1000 + Motion.lyricCarry, "but never further than the next position could correct")
        Player.paused = true
        const held = panel.nowMs
        wait(80)
        compare(panel.nowMs, held, "a paused track does not move")
    }

    function test_the_source_is_said_and_plain_lists_still_show() {
        show(words, true, 0)
        compare(panel.lyrics.source, "LRCLIB")
        show("", false, -1)
        Player.lyrics = ["Just", "text"]
        verify(!findChild(panel, "sungLine"), "without the document every line is plain")
        compare(findChild(panel, "lyricsList").count, 2)
    }

    function test_the_end_of_a_line_that_states_none() {
        show(words, true, 0)
        compare(panel.lineEnd(0), 3000, "stated")
        compare(panel.lineEnd(1), 4000 + Motion.lyricLastLine, "the last one, a few seconds")
        show(lines, true, 0)
        compare(panel.lineEnd(0), 5000, "until the next")
    }
}
