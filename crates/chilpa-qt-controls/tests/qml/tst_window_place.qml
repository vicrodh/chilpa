import QtQuick
import QtTest
import com.blitzfc.chilpa
import com.blitzfc.chilpa.controls
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
}
