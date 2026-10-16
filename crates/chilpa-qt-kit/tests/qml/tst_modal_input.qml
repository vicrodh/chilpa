import QtQuick
import QtTest
import "../../qml/Primitives"

TestCase {
    id: test
    name: "ModalInput"
    when: windowShown
    visible: true
    width: 600
    height: 400
    property int backgroundClicks: 0
    property int insideClicks: 0
    property int insideMouseClicks: 0

    MouseArea { anchors.fill: parent; onClicked: test.backgroundClicks++ }
    Modal {
        id: dialog
        width: 240
        height: 160
        Rectangle {
            id: inside
            width: 120
            height: 70
            TapHandler { onTapped: test.insideClicks++ }
        }
        Rectangle {
            id: mouseInside
            x: 130
            width: 70
            height: 70
            MouseArea { anchors.fill: parent; onClicked: test.insideMouseClicks++ }
        }
    }

    function cleanup() { dialog.close(); backgroundClicks = 0; insideClicks = 0; insideMouseClicks = 0 }

    function test_inside_press_stays_in_the_modal() {
        dialog.open()
        tryVerify(() => dialog.opened)
        mouseClick(inside, inside.width / 2, inside.height / 2)
        compare(insideClicks, 1)
        compare(backgroundClicks, 0)
        verify(dialog.opened)
    }

    function test_outside_dismissal_does_not_click_the_page() {
        dialog.open()
        tryVerify(() => dialog.opened)
        mouseClick(test, 10, 10)
        tryVerify(() => !dialog.opened)
        compare(backgroundClicks, 0)
    }

    function test_a_mouse_area_inside_receives_its_click() {
        dialog.open()
        tryVerify(() => dialog.opened)
        mouseClick(mouseInside, mouseInside.width / 2, mouseInside.height / 2)
        compare(insideMouseClicks, 1)
        compare(backgroundClicks, 0)
        verify(dialog.opened)
    }
}
