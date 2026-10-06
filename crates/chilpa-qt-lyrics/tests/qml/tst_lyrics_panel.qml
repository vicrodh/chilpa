import QtQuick
import QtTest
import com.blitzfc.chilpa
import "../../../chilpa-qt/qml/Patterns"
import "../../../chilpa-qt-kit/qml/Theme"

TestCase {
    name: "LyricsPanel"
    when: windowShown
    visible: true
    width: 420
    height: 600

    LyricsPanel { id: panel; width: 400; height: 580 }

    // Two lines timed by word, the first with a word split in syllables.
    readonly property string words: JSON.stringify({ status: "found", source: "LRCLIB", translation: null, document: { schema: 1, timing: "syllable", lines: [
        { start_ms: 1000, end_ms: 3000, text: "Hello world", segments: [
            { text: "Hel", start_ms: 1000, end_ms: 1500, space_after: false },
            { text: "lo", start_ms: 1500, end_ms: 2000, space_after: true },
            { text: "world", start_ms: 2000, end_ms: 3000, space_after: false } ] },
        { start_ms: 4000, text: "Again" } ] } })
    readonly property string lines: JSON.stringify({ status: "found", source: "File", translation: null, document: { schema: 1, timing: "line", lines: [
        { start_ms: 1000, text: "One two three" }, { start_ms: 5000, text: "Four" } ] } })
    // The same two lines with a romanization and, second, a translation.
    readonly property string translated: JSON.stringify({ status: "found", source: "Somewhere", translation: 1, document: { schema: 1, timing: "line", lines: [
        { start_ms: 1000, text: "One two three" }, { start_ms: 5000, text: "Four" } ], layers: [
        { kind: "romanization", language: "und-Latn", lines: [{ line: 0, text: "wan tu suri" }] },
        { kind: "translation", language: "es", lines: [{ line: 0, text: "Uno dos tres" }, { line: 1, text: "Cuatro" }] } ] } })
    readonly property string plain: JSON.stringify({ status: "found", source: "tags", translation: null, document: { schema: 1, timing: "unsynced", lines: [
        { text: "No times here" }, { text: "Nor here" } ] } })

    function show(json, synced, line) {
        Player.lyricsJson = json
        const said = json ? JSON.parse(json) : null
        Player.lyrics = said && said.document ? said.document.lines.map(l => l.text) : []
        Player.lyricsSynced = synced
        Player.lyricsLine = line
    }

    function options() {
        return findChild(panel, "lyricsOptions")
    }

    function row(index) {
        const list = findChild(panel, "lyricsList")
        list.forceLayout()
        return list.itemAtIndex(index)
    }

    function init() {
        Player.playing = true
        Player.paused = false
        Player.position = 0
        Player.sought = -1
        Player.title = "Fixture track"
        Player.artist = "Fixture artist"
        SettingsModel.preferences = ({})
        options().reset()
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

    function test_a_line_breaks_in_the_same_places_sung_or_not() {
        const parts = "Realize you are living in the golden years and never look back again".split(" ")
        const long = JSON.stringify({ status: "found", source: "Somewhere", translation: null, document: { schema: 1, timing: "word", lines: [
            { start_ms: 1000, end_ms: 1000 + parts.length * 500, text: parts.join(" "),
              segments: parts.map((w, i) => ({ text: w, start_ms: 1000 + i * 500, end_ms: 1500 + i * 500, space_after: i + 1 < parts.length })) },
            { start_ms: 60000, text: "Next" } ] } })
        for (const size of ["m", "xl"]) {
            options().choose("size", size)
            show(long, true, 1)
            const plain = row(0).height
            verify(plain > row(1).height, "long enough to wrap")
            Player.lyricsLine = 0
            const sung = findChild(panel, "sungLine")
            verify(sung, "now it is the line being sung")
            compare(row(0).height, plain, "lit, it takes the room it took: no word moved to another row")
            compare(sung.spot(0).x, 0)
            const last = sung.spot(parts.length - 1)
            verify(last.y > 0, "the last word sits on a row below the first")
            verify(last.x < sung.width, "and inside the line")
        }
    }

    // Nine lines timed as wholes, a second apart.
    readonly property string song: JSON.stringify({ status: "found", source: "File", translation: null, document: { schema: 1, timing: "line",
        lines: ["One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine"].map((text, at) => ({ start_ms: at * 1000, text: text })) } })

    function test_the_lines_fade_around_the_sung_one_and_harder_ahead() {
        show(song, true, 4)
        compare(row(4).lit, 1, "the sung line whole")
        verify(row(4).sung && !row(4).past)
        verify(row(3).past && !row(5).past)
        verify(row(3).lit < 1 && row(2).lit < row(3).lit, "what went by fades with its distance")
        verify(row(5).lit < row(3).lit, "what is to come fades harder than what was sung")
        verify(row(6).lit < row(5).lit)
        tryCompare(findChild(row(5), "words"), "opacity", row(5).lit)
        Player.lyricsLine = 5
        compare(row(5).lit, 1, "its turn came")
        verify(findChild(row(4), "words").opacity > row(4).lit, "the line that went by fades out, it does not snap")
        tryCompare(findChild(row(4), "words"), "opacity", row(4).lit)
    }

    function test_lyrics_that_do_not_follow_the_music_fade_around_nothing() {
        show(plain, false, -1)
        compare(row(0).lit, row(1).lit)
        verify(!row(0).sung && !row(0).past)
        show(song, true, -1)
        compare(row(3).lit, 1, "before the first line is sung every line is whole")
    }

    function test_the_sung_line_sits_in_the_middle() {
        const list = findChild(panel, "lyricsList")
        show(song, true, 0)
        const middle = (item) => item.mapToItem(list, 0, item.height / 2).y
        tryVerify(() => Math.abs(middle(row(0)) - list.height / 2) < 2, 1000, "even the first line: there is room above it")
        Player.lyricsLine = 8
        tryVerify(() => Math.abs(middle(row(8)) - list.height / 2) < 2, 1000, "and the last")
        options().choose("follow", false)
        const at = list.contentY
        Player.lyricsLine = 2
        wait(Motion.lyricScroll + 60)
        compare(list.contentY, at, "not followed, it stays where the reader left it")
    }

    function test_plain_lists_still_show_without_the_document() {
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

    function test_the_header_says_what_plays() {
        compare(panel.playing, "Fixture track — Fixture artist")
        Player.artist = ""
        compare(panel.playing, "Fixture track", "a track nobody signs is only its title")
        Player.title = ""
        compare(panel.playing, "")
    }

    function test_it_says_it_is_looking_and_then_that_there_are_none() {
        const state = findChild(panel, "lyricsState")
        show("", false, -1)
        Player.playing = false
        verify(state.visible)
        compare(state.text, "Lyrics show here while a track plays.")
        Player.playing = true
        show(JSON.stringify({ status: "searching" }), false, -1)
        compare(state.text, "Looking for lyrics…")
        show(JSON.stringify({ status: "none" }), false, -1)
        compare(state.text, "No lyrics for this track.")
        show(lines, true, 0)
        verify(!state.visible, "with lyrics there is nothing else to say")
    }

    function test_the_source_is_said_at_the_foot() {
        const source = findChild(panel, "lyricsSource")
        verify(!source.visible, "nothing to credit without lyrics")
        show(words, true, 0)
        verify(source.visible)
        compare(findChild(panel, "lyricsSourceName").text, "LRCLIB")
        show(JSON.stringify({ status: "none" }), false, -1)
        verify(!source.visible)
    }

    function test_a_translation_is_offered_only_when_there_is_one() {
        const button = findChild(panel, "translateButton")
        show(lines, true, 0)
        verify(!button.visible, "lyrics without a translation offer none")
        show(translated, true, 0)
        verify(button.visible)
        compare(row(1).translation, "", "and it waits to be asked for")
        mouseClick(button)
        verify(panel.translating)
        compare(row(0).translation, "Uno dos tres", "the translation, not the romanization")
        compare(row(1).translation, "Cuatro")
        verify(findChild(row(1), "translation").visible)
        mouseClick(button)
        compare(row(1).translation, "")
    }

    function test_the_translation_is_put_away_with_its_track() {
        show(translated, true, 0)
        mouseClick(findChild(panel, "translateButton"))
        verify(panel.translating)
        show(JSON.stringify({ status: "searching" }), false, -1)
        verify(!panel.translating, "the next track starts as it was written")
    }

    function test_a_click_on_a_line_goes_there() {
        show(lines, true, 0)
        mouseClick(row(1))
        compare(Player.sought, 5, "the second line starts at five seconds")
        show(plain, false, -1)
        Player.sought = -1
        mouseClick(row(1))
        compare(Player.sought, -1, "a line without a time goes nowhere")
    }

    function test_the_options_open_from_the_header() {
        const button = findChild(panel, "lyricsOptionsButton")
        verify(!options().opened)
        mouseClick(button)
        tryVerify(() => options().opened)
        options().close()
        tryVerify(() => !options().opened)
    }

    function test_the_size_is_chosen_and_kept() {
        show(lines, true, 0)
        compare(panel.size, Type.lyric, "medium until something else is chosen")
        options().choose("size", "xl")
        compare(panel.size, Type.lyricXLarge)
        compare(SettingsModel.preferences["ui.lyrics.size"], "xl")
        compare(row(1).size, Type.lyricXLarge)
        options().choose("size", "s")
        compare(panel.size, Type.lyricSmall)
        options().reset()
        compare(panel.size, Type.lyric)
        compare(SettingsModel.preferences["ui.lyrics.size"], "m")
    }

    function test_following_the_sung_line_can_be_turned_off() {
        show(lines, true, 0)
        const list = findChild(panel, "lyricsList")
        verify(list.following, "it follows until told not to")
        options().choose("follow", false)
        verify(!list.following)
        compare(SettingsModel.preferences["ui.lyrics.follow"], "false")
    }

    function test_the_lite_fill_lights_the_line_whole() {
        show(words, true, 0)
        verify(findChild(panel, "sungLine"))
        options().choose("lite", true)
        verify(!findChild(panel, "sungLine"), "no word by word, and no clock to run it")
        verify(row(0).sung)
    }

    function test_capitals_color_and_dimming_reach_the_lines() {
        show(lines, true, 0)
        verify(!row(1).uppercase)
        options().choose("uppercase", true)
        verify(row(1).uppercase)
        options().choose("color", "accent")
        verify(Qt.colorEqual(row(0).sungColor, Theme.color.accent))
        compare(row(1).dimming, "strong", "strong dimming until chosen otherwise")
        verify(row(1).lit < 1)
        options().choose("dimming", "off")
        compare(row(1).lit, 1, "no dimming, every line whole")
        options().choose("translation", "fr")
        compare(SettingsModel.preferences["ui.lyrics.translation"], "fr", "the language goes where the facade reads it")
    }

    function test_the_lyrics_are_copied_and_it_is_said() {
        show(lines, true, 0)
        compare(options().text, "One two three\nFour")
        const notice = findChild(panel, "lyricsNotice")
        verify(!notice.visible)
        options().copy()
        verify(notice.visible)
        compare(notice.text, "Lyrics copied")
        show("", false, -1)
        verify(!options().canCopy, "nothing to copy without lyrics")
    }
}
