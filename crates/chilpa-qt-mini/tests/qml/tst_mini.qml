import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.mini
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Components"
import "../../qml/Patterns"
import "../../qml/Screens"
import "../../qml/Theme"

// The mini player's card: its shape per surface, what drags the window
// and what keeps its own gestures, the capsule's fold, the keys, the queue
// rows, the backdrop and the lyrics' empty state.
TestCase {
    id: test
    name: "Mini"
    when: windowShown
    visible: true
    width: 400
    height: 600

    QtObject {
        id: host
        property int moves: 0
        property bool pinnedOnTop: false
        function startSystemMove() { moves += 1 }
    }
    MiniShell {
        id: card
        width: Card.windowSpan - Card.gutter * 2
        height: (card.surfaceId === 0 ? Card.heightMicro : card.surfaceId === 1 ? Card.heightCompact : Card.heightExpanded) - Card.gutter * 2
        hostWindow: host
    }
    MiniWindow { id: win }

    function init() {
        host.moves = 0
        Mini.open = true
        Mini.backgroundBlur = false
        Mini.surface = 2
        Mini.exits = 0
        Player.path = "/test.wav"
        Player.playing = true
        Player.paused = false
        Player.toggles = 0
        Player.volume = 50
        Player.sought = -1
        ThemeModel.ambient = false
        mouseMove(test, 395, 590)
        waitForRendering(card)
    }

    function test_the_card_follows_the_surface() {
        Mini.surface = 0
        compare(card.footerHeight, Card.footerMicro)
        compare(card.radius, Card.radiusCondensed)
        compare(card.footerMode, 2)
        Mini.surface = 1
        compare(card.footerHeight, Card.footerCompact)
        compare(card.footerMode, 1)
        Mini.surface = 4
        compare(card.footerHeight, Card.footerFull)
        compare(card.radius, Card.radiusExpanded)
        compare(card.footerMode, 0)
    }

    function test_the_free_area_drags_the_window_data() {
        return [{ tag: "micro", surface: 0, y: 8 }, { tag: "compact", surface: 1, y: 45 }, { tag: "artwork", surface: 2, y: 100 }]
    }
    function test_the_free_area_drags_the_window(data) {
        Mini.surface = data.surface
        waitForRendering(card)
        mousePress(card, 100, data.y)
        compare(host.moves, 1)
        mouseRelease(card, 100, data.y)
    }

    function test_the_controls_keep_their_gestures() {
        const play = findChild(card, "miniPlay")
        verify(play !== null)
        mouseClick(play, play.width / 2, play.height / 2)
        compare(Player.toggles, 1)
        compare(host.moves, 0)
        mouseDrag(card, 100, card.height - Card.footerFull + Card.seekHeight[0] / 2, 40, 0)
        verify(Player.sought >= 0, "the seek bar seeks")
        compare(host.moves, 0)
        for (const surface of [3, 4]) {
            Mini.surface = surface
            waitForRendering(card)
            mouseDrag(card, 100, 150, 0, -50)
            compare(host.moves, 0, "a scrolling surface never drags the window")
            mousePress(card, 3, card.height - 3)
            compare(host.moves, 1, "the footer's edge still drags")
            mouseRelease(card, 3, card.height - 3)
            host.moves = 0
        }
    }

    function test_the_capsule_unfolds_on_hover_and_folds_when_the_pointer_leaves() {
        const capsule = findChild(card, "miniWindowControls")
        verify(capsule !== null, "the trigger stays mounted outside hover")
        tryCompare(capsule, "expanded", false)
        const trigger = findChild(capsule, "miniMenuTrigger")
        mouseMove(trigger, 11, 11)
        tryCompare(capsule, "expanded", true)
        verify(capsule.width > Card.capsuleTall)
        mouseMove(card, 2, 2)
        tryCompare(capsule, "expanded", false)
    }

    function test_keys_reach_the_surfaces_the_player_and_the_exit() {
        card.run("mini.queue")
        compare(Mini.surface, 3)
        card.run("playback.toggle")
        compare(Player.toggles, 1)
        card.run("playback.volumeUp")
        compare(Player.volume, 55)
        card.run("ui.escape")
        compare(Mini.exits, 1)
    }

    function test_the_queue_resumes_the_playing_row_and_plays_the_rest() {
        Mini.surface = 3
        waitForRendering(card)
        const list = findChild(card, "miniQueue")
        verify(list !== null)
        list.model.append({ title: "Now", artist: "Band", duration: "3:00", playing: true })
        list.model.append({ title: "Then", artist: "Band", duration: "4:00", playing: false })
        waitForRendering(card)
        Player.paused = true
        mouseClick(list, 100, Card.queueTop + Card.queueRow / 2)
        compare(Player.toggles, 1, "a paused playing row resumes")
        Player.paused = false
        mouseClick(list, 100, Card.queueTop + Card.queueRow / 2)
        compare(Player.toggles, 1, "a playing row is never restarted")
        mouseClick(list, 100, Card.queueTop + Card.queueRow * 1.5)
        compare(list.model.lastAction, "play")
        compare(list.model.lastRow, 1)
    }

    function test_the_backdrop_follows_the_app_and_the_blur_and_leaves_with_the_window() {
        ThemeModel.ambient = true
        verify(card.useAppBackground)
        verify(card.backdropMounted)
        Mini.backgroundBlur = true
        verify(!card.useAppBackground, "the card's own blur overrides the app's background")
        verify(card.backdropOn)
        Mini.backgroundBlur = false
        Mini.open = false
        verify(!card.backdropMounted)
        Mini.open = true
        ThemeModel.ambient = false
        verify(!card.backdropOn, "no background means the opaque theme is back")
    }

    function test_closed_artwork_surfaces_release_and_reopen_data() {
        return [{ tag: "compact", surface: 1 }, { tag: "artwork", surface: 2 }]
    }
    function test_closed_artwork_surfaces_release_and_reopen(data) {
        Mini.surface = data.surface
        tryVerify(() => findChild(card, "miniMetadata") !== null)
        Mini.open = false
        tryCompare(card, "backdropMounted", false)
        tryVerify(() => findChild(card, "miniMetadata") === null, 1000, "hidden mini must release its cover surface")
        Player.title = "A new track"
        Mini.open = true
        tryVerify(() => findChild(card, "miniMetadata") !== null)
    }

    // Closing the window from the desktop leaves the mini player; once it
    // has been left, the close is the app quitting and goes through.
    function test_closing_the_window_leaves_the_mini_player_and_a_quit_goes_through() {
        let accepted = null
        win.closing.connect(function (close) { accepted = close.accepted })
        win.show()
        tryVerify(() => win.visible)
        win.close()
        compare(accepted, false, "the window stays: the mini player was left instead")
        compare(Mini.exits, 1)
        Mini.open = false
        accepted = null
        win.close()
        compare(accepted, true, "left already, the window closes")
        tryVerify(() => !win.visible)
    }

    function test_lyrics_without_lines_say_so() {
        Mini.surface = 4
        Player.lyricsJson = ""
        waitForRendering(card)
        const empty = findChild(card, "miniLyricsEmpty")
        verify(empty !== null && empty.visible)
        Player.lyricsJson = JSON.stringify({ status: "found", document: { timing: "line", lines: [{ text: "la", start_ms: 0 }] } })
        tryVerify(() => !empty.visible)
    }
}
