import QtQuick
import QtTest
import app.chilpa
import app.chilpa.controls
import "../../qml/Patterns"
import "../../qml/Theme"

// The window goes back to where it was left and says where it is once it
// settles (design/04 Window).
TestCase {
    id: test
    name: "WindowPlace"
    when: windowShown
    visible: true
    width: 400
    height: 300

    Window { id: other; width: 700; height: 500 }
    Component { id: maker; WindowControls { window: other } }
    property var made: null

    function open(left, top, placed) {
        WindowPlace.left = left
        WindowPlace.top = top
        WindowPlace.placed = placed
        other.x = 5
        other.y = 5
        made = maker.createObject(test)
        other.show()
        WindowPlace.kept = []
    }
    function cleanup() {
        if (made)
            made.destroy()
        made = null
        Controls.trayClose = false
        other.hide()
        other.width = 700
        other.height = 500
    }

    function test_it_goes_back_to_where_it_was() {
        open(40, 30, true)
        compare(other.x, 40)
        compare(other.y, 30)
    }

    function test_a_place_on_no_screen_is_not_gone_back_to() {
        open(50000, 30, true)
        compare(other.x, 5)
        compare(other.y, 5)
    }

    function test_a_window_never_placed_is_left_to_the_system() {
        open(40, 30, false)
        compare(other.x, 5)
    }

    function test_it_says_its_frame_once_after_it_settles() {
        open(0, 0, false)
        other.width = 720
        other.width = 740
        other.height = 520
        compare(WindowPlace.kept.length, 0, "not while it is still moving")
        tryVerify(() => WindowPlace.kept.length > 0)
        wait(Sizes.placeSettle)
        compare(WindowPlace.kept.length, 1)
        const said = WindowPlace.kept[0]
        compare(said[0], "floating")
        compare(said[1], 740)
        compare(said[2], 520)
        compare(said[5], true, "this platform tells a window where it is")
    }

    function test_closing_says_it_at_once() {
        open(0, 0, false)
        other.width = 760
        other.close()
        compare(WindowPlace.kept.length, 1)
        compare(WindowPlace.kept[0][1], 760)
        wait(Sizes.placeSettle)
        compare(WindowPlace.kept.length, 1, "and not again when the wait ends")
    }

    // With a tray to go to, closing hides the window instead; the desktop's
    // own Quit still closes it.
    property var accepted: null
    function test_closing_hides_to_the_tray_when_that_is_wanted() {
        open(0, 0, false)
        Controls.trayClose = true
        other.closing.connect(function (close) { test.accepted = close.accepted })
        other.close()
        compare(test.accepted, false, "the close was refused")
        verify(!other.visible, "and the window hid")
        compare(WindowPlace.kept.length, 1, "its place was kept all the same")

        other.show()
        test.accepted = null
        made.quitting = true
        other.close()
        compare(test.accepted, true, "a quit is not hidden from")
    }

    // Quit asked from inside Chilpa goes the desktop's way: the window
    // closes for real instead of hiding to the tray.
    function test_quit_from_inside_closes_through_the_tray() {
        open(0, 0, false)
        Controls.trayClose = true
        other.closing.connect(function (close) { test.accepted = close.accepted })
        test.accepted = null
        made.run("nav.quit")
        compare(test.accepted, true, "the window closed")
    }

    // The tray icon's click shows a hidden window and hides one in sight.
    function test_the_tray_toggles_the_window() {
        open(0, 0, false)
        verify(other.visible)
        Player.windowAsked(2)
        verify(!other.visible)
        Player.windowAsked(2)
        verify(other.visible)
        Player.windowAsked(0)
        verify(other.visible)
    }
}
