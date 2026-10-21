import QtQuick
import QtQuick.Window
import QtTest
import app.chilpa.kit
import "../../qml/Primitives"
import "../../qml/Components"
import "../../qml/Theme"

TestCase {
    id: test
    name: "PendingFavoriteFeedback"
    when: windowShown
    visible: true
    width: 200; height: 100
    FavoriteButton { id: button; clock: Pulse; tip: "Favorite"; glyph: Icons.heartFilled }
    SignalSpy { id: clicks; target: button; signalName: "clicked" }
    Component {
        id: pendingWindow
        Window { visible: true; width: 100; height: 100; BusyMark { clock: Pulse } }
    }
    function cleanup() { button.busy = false; button.visible = true; Motion.reduced = false; clicks.clear() }
    function test_the_filled_heart_always_selects_the_filled_font() {
        const icon = button.children[1]
        tryCompare(icon, "text", Icons.heartFilled)
        compare(icon.font.family, Icons.filled)
    }
    function test_waiting_blocks_repeated_clicks_and_releases_the_clock() {
        compare(Pulse.held, 0)
        button.busy = true
        compare(Pulse.held, 1)
        const spinner = button.progressMark
        const before = spinner.phase
        tryVerify(() => spinner.phase !== before)
        mouseClick(button)
        compare(clicks.count, 0)
        button.busy = false
        compare(Pulse.held, 0)
        mouseClick(button)
        compare(clicks.count, 1)
    }
    function test_the_same_pending_button_stops_when_hidden_and_resumes_when_shown() {
        button.busy = true
        compare(Pulse.held, 1)
        button.visible = false
        compare(Pulse.held, 0)
        button.visible = true
        compare(Pulse.held, 1)
    }
    function test_reduced_motion_keeps_a_static_progress_mark_without_a_clock() {
        Motion.reduced = true
        button.busy = true
        compare(Pulse.held, 0)
        verify(button.progressMark !== null)
    }
    function test_hidden_and_minimized_windows_release_the_shared_clock() {
        const window = createTemporaryObject(pendingWindow, test)
        verify(window)
        tryCompare(Pulse, "held", 1)
        window.visible = false
        compare(Pulse.held, 0)
        window.visible = true
        compare(Pulse.held, 1)
        window.visibility = Window.Minimized
        compare(Pulse.held, 0)
        window.visible = false
    }
}
