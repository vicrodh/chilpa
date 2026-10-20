import QtQuick
import QtQuick.Window
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.immersive
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Components"
import "../../qml/Patterns"
import "../../qml/Screens"
import "../../qml/Theme"

// The immersive view: it shows and takes the keys when the bridge opens,
// Escape leaves it, the chrome hides after a rest and wakes on a move,
// the bar plays and seeks, the view menu walks the panels, the settle
// eases on the pulse and nothing moves while closed.
TestCase {
    id: test
    name: "Immersive"
    when: windowShown
    visible: true
    width: 900
    height: 600

    QtObject {
        id: controls
        property bool immersiveOpen: false
        property int asked: 0
        signal immersiveAsked()
        function pressed(event) {}
    }
    ImmersiveView {
        id: view
        anchors.fill: parent
        window: test.Window.window
        controls: controls
    }

    function init() {
        Immersive.open = false
        Immersive.viewMode = 1
        Immersive.mode = 0
        Immersive.splitPanel = 0
        Immersive.scene = 0
        Immersive.views = []
        Player.path = "/test.wav"
        Player.art = ""
        Player.playing = true
        Player.paused = false
        Player.toggles = 0
        Player.sought = -1
        Player.position = 9
        Player.duration = 30
        Immersive.open = true
        waitForRendering(view)
    }

    function test_opening_shows_the_view_and_tells_the_controls() {
        verify(view.visible)
        compare(controls.immersiveOpen, true)
        Immersive.open = false
        verify(!view.visible)
        compare(controls.immersiveOpen, false)
    }

    function test_closing_releases_content_and_reopening_keeps_the_selected_view() {
        Immersive.viewMode = 0
        Immersive.mode = 1
        Immersive.open = false
        compare(view.content, null, "I release the whole visual tree")
        tryVerify(() => findChild(view, "immersiveBar") === null)
        Player.art = "qrc:/qt/qml/app/chilpa/kit/assets/logo.png"
        compare(view.content, null, "a cover change cannot recreate hidden content")
        Immersive.open = true
        verify(view.content !== null)
        compare(Immersive.viewMode, 0)
        compare(Immersive.mode, 1, "I reopen the selected panel")
    }

    function test_escape_leaves_the_view() {
        view.forceActiveFocus()
        keyClick(Qt.Key_Escape)
        compare(Immersive.open, false)
    }

    function test_the_chrome_hides_after_a_rest_and_wakes_on_a_move() {
        verify(view.content.chromeVisible)
        view.content.chromeVisible = false
        mouseMove(view, 300, 300)
        mouseMove(view, 320, 310)
        verify(view.content.chromeVisible, "a real move wakes it")
    }

    function test_the_bar_plays_and_seeks() {
        const play = findChild(view, "immersivePlay")
        verify(play !== null)
        mouseClick(play, play.width / 2, play.height / 2)
        compare(Player.toggles, 1)
        const bar = findChild(view, "immersiveBar")
        verify(bar !== null)
        mouseClick(bar, bar.width / 2, Stage.tinyTop + Stage.tinyGrab / 2)
        verify(Player.sought >= 0, "the thin bar seeks")
        verify(Math.abs(Player.sought - 15) < 2, "to the middle: " + Player.sought)
    }

    function test_the_bars_menu_leaves_the_view_and_dies_with_it() {
        const more = findChild(view, "immersiveMore")
        verify(more !== null)
        mouseClick(more, more.width / 2, more.height / 2)
        const menu = findChild(view, "immersiveExitMenu")
        verify(menu !== null)
        tryVerify(() => menu.opened, 1000, "the menu opens")
        const row = findChild(view, "immersiveExitRow")
        verify(row !== null)
        verify(row.height > 0, "the row has a height to click: " + row.height)
        mouseClick(row, row.width / 2, row.height / 2)
        compare(Immersive.open, false, "Exit Immersive leaves")
        tryVerify(() => findChild(view, "immersiveExitMenu") === null, 1000, "the menu goes with the view")
        Immersive.open = true
        waitForRendering(view)
        const reopenedMore = findChild(view, "immersiveMore")
        const reopenedMenu = findChild(view, "immersiveExitMenu")
        mouseClick(reopenedMore, reopenedMore.width / 2, reopenedMore.height / 2)
        tryVerify(() => reopenedMenu.opened, 1000)
        Immersive.open = false
        tryVerify(() => findChild(view, "immersiveExitMenu") === null, 1000, "an Escape-style exit releases it too")
    }

    function test_the_corner_button_leaves_the_view_beside_the_bar() {
        const leave = findChild(view, "immersiveLeave")
        const bar = findChild(view, "immersiveBar")
        verify(leave !== null)
        compare(leave.x, Stage.edge, "at the left edge")
        const middle = leave.mapToItem(view, 0, leave.height / 2).y
        const barMiddle = bar.mapToItem(view, 0, bar.height / 2).y
        verify(Math.abs(middle - barMiddle) <= 1, "level with the bar: " + middle + " against " + barMiddle)
        view.content.chromeVisible = false
        verify(!leave.enabled, "it hides with the chrome")
        view.content.chromeVisible = true
        mouseClick(leave, leave.width / 2, leave.height / 2)
        compare(Immersive.open, false, "it leaves the view")
    }

    function test_the_header_walks_the_views() {
        const header = findChild(view, "immersiveHeader")
        verify(header !== null)
        header.cycle(1)
        compare(Immersive.views.length, 1)
        compare(Immersive.views[0][0], 1, "from split lyrics, next is the next split panel")
        compare(Immersive.views[0][2], 3)
    }

    function test_the_settle_eases_toward_the_frame_on_the_pulse() {
        const settle = settleMaker.createObject(test)
        settle.target = [1, 0.5]
        Pulse.ms += Pulse.period
        verify(settle.at(0) > 0.4 && settle.at(0) < 0.5, "a step of easeK: " + settle.at(0))
        Pulse.ms += Pulse.period
        verify(settle.at(0) > 0.6, "and closer on the next: " + settle.at(0))
        settle.live = false
        settle.target = [0, 0]
        Pulse.ms += Pulse.period
        verify(settle.at(0) > 0.6, "frozen, it holds")
        settle.destroy()
    }
    Component { id: settleMaker; StreamSettle {} }

    readonly property string translated: JSON.stringify({ status: "found", source: "Somewhere", translation: 0, document: { schema: 1, timing: "line", lines: [
        { start_ms: 1000, text: "One two three" }, { start_ms: 5000, text: "Four" } ], layers: [
        { kind: "translation", language: "es", lines: [{ line: 0, text: "Uno dos tres" }, { line: 1, text: "Cuatro" }] } ] } })

    function test_the_lyrics_offer_their_translation_where_they_show() {
        Player.lyricsJson = translated
        Player.lyricsSynced = true
        Player.lyricsLine = 1
        Immersive.setView(1, 0, 0)
        waitForRendering(view)
        const split = findChild(view, "immersiveSplitTranslate")
        verify(split !== null && split.visible, "the split's lyrics offer it at their bottom right")
        compare(findChild(view, "immersiveSplitLyrics").translation, null, "and wait to be asked")
        mouseClick(split)
        compare(findChild(view, "immersiveSplitLyrics").translation[1], "Cuatro")
        Immersive.setView(1, 0, 5)
        waitForRendering(view)
        const card = findChild(view, "immersiveCardTranslate")
        verify(card !== null && card.visible && card.on, "the card's line keeps the same choice")
        mouseClick(card)
        verify(!card.on)
        Player.lyricsJson = JSON.stringify({ status: "searching" })
        verify(!card.visible, "lyrics without a translation offer none")
    }

    function test_the_view_picker_moves_right_only_on_a_mac() {
        const header = findChild(view, "immersiveHeader")
        const trigger = findChild(header, "immersiveViewTrigger")
        header.pickerTrailing = false
        compare(trigger.x, 0)
        header.pickerTrailing = true
        // The capsule's open width is kept free, so the picker never slides.
        verify(trigger.x + trigger.width < header.width - 40 - 36 * 2, "clear of the capsule however wide it opens")
        compare(trigger.x, header.width - findChild(header, "immersiveViewTrigger").parent.children.find(c => c.openWide !== undefined).openWide - 12 - trigger.width)
        header.pickerTrailing = Qt.platform.os === "osx"
    }

    function test_the_focus_lyrics_say_when_there_are_none() {
        Player.lyricsJson = ""
        Immersive.setView(0, 4, 0)
        waitForRendering(view)
        const empty = findChild(view, "immersiveNoLyrics")
        verify(empty !== null && empty.visible)
    }
}
