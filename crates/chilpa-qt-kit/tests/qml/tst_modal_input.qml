import QtQuick
import QtQuick.Window
import QtQuick.Controls.Basic as Controls
import QtTest
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

TestCase {
    id: test
    name: "ModalInput"
    when: windowShown
    visible: true
    width: 600
    height: 400
    property bool windowEnabled: true
    property int backgroundClicks: 0
    property int backgroundTaps: 0
    property int backgroundWheels: 0
    property int backgroundKeys: 0
    property int backgroundDrags: 0
    property int menuChoices: 0
    property int disabledFrames: 0
    property int insideClicks: 0
    property int insideMouseClicks: 0

    TapHandler { onTapped: test.backgroundTaps++ }
    DragHandler { target: null; onActiveChanged: if (active) test.backgroundDrags++ }
    Shortcut { sequence: "Ctrl+A"; enabled: !InputLayers.blocksShortcuts(test.Window.window); onActivated: test.backgroundKeys++ }
    MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons; onClicked: test.backgroundClicks++; onWheel: wheel => { test.backgroundWheels++; wheel.accepted = true } }
    TextInput { id: backgroundField; y: 320; width: 100; height: 30; text: "Before" }
    Connections {
        target: test.Window.window
        function onAfterAnimating() { if (!test.Window.window.contentItem.enabled) test.disabledFrames++ }
    }
    QtObject {
        id: feed
        property var bars: []
        property var waveform: []
        property var energy: []
        property int releases: 0
        function setCompact(active, mode) { if (!active) releases++ }
    }
    CompactVisualizer { id: band; y: 360; width: 200; height: 24; active: true; feed: feed }
    Component { id: temporaryModal; Modal { width: 100; height: 100 } }
    Modal { id: nested; width: 100; height: 80 }

    Modal {
        id: dialog
        width: 240
        height: 260
        enter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 80 } }
        exit: Transition { NumberAnimation { property: "opacity"; from: 1; to: 0; duration: 80 } }
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
        Flickable {
            id: scrolling
            x: 0; y: 90; width: 100; height: 80
            contentHeight: 600
            boundsBehavior: Flickable.StopAtBounds
            Rectangle { width: 100; height: 600 }
        }
        Menu { id: context; entries: [{text:"Action"}]; onChosen: test.menuChoices++ }
        PillButton { id: menuButton; y: 180; text: "Menu"; onClicked: context.open() }
        TextInput { id: field; x: 110; y: 90; width: 80; height: 40; text: "Edit" }
    }

    function cleanup() {
        context.close(); nested.close(); dialog.close()
        tryCompare(dialog, "visible", false)
        tryCompare(nested, "visible", false)
        compare(InputLayers.layers.length, 0)
        backgroundClicks = 0; backgroundTaps = 0; insideClicks = 0; insideMouseClicks = 0
        backgroundWheels = 0; backgroundKeys = 0; backgroundDrags = 0
        scrolling.contentY = 0
        menuChoices = 0
        compare(disabledFrames, 0)
    }
    function untouched() {
        compare(backgroundClicks, 0)
        compare(backgroundTaps, 0)
        compare(backgroundWheels, 0)
        compare(backgroundKeys, 0)
        compare(backgroundDrags, 0)
    }
    function open() { dialog.open(); tryVerify(() => dialog.opened) }

    function test_transition_and_focus_changes_keep_shortcuts_blocked() {
        dialog.open()
        verify(dialog.visible)
        verify(!dialog.opened)
        verify(InputLayers.blocks(test.Window.window))
        test.forceActiveFocus()
        keyClick(Qt.Key_A, Qt.ControlModifier)
        tryVerify(() => dialog.opened)
        dialog.close()
        verify(dialog.visible)
        verify(InputLayers.blocks(test.Window.window))
        keyClick(Qt.Key_A, Qt.ControlModifier)
        untouched()
        tryCompare(dialog, "visible", false)
        keyClick(Qt.Key_A, Qt.ControlModifier)
        compare(backgroundKeys, 1)
    }
    function test_nested_and_destroyed_popups_release_only_their_own_layer() {
        open()
        nested.open()
        tryCompare(nested, "opened", true)
        compare(InputLayers.layers.length, 2)
        keyClick(Qt.Key_Escape)
        tryCompare(nested, "visible", false)
        verify(dialog.visible)
        compare(InputLayers.layers.length, 1)
        const temporary = createTemporaryObject(temporaryModal, test)
        temporary.open()
        tryCompare(temporary, "opened", true)
        compare(InputLayers.layers.length, 2)
        temporary.destroy()
        tryVerify(() => InputLayers.layers.length === 1)
        verify(InputLayers.blocks(test.Window.window))
    }
    function test_wheel_and_secondary_buttons_do_not_reach_the_page() {
        open()
        mouseWheel(inside, 40, 40, 0, -120)
        mouseWheel(test, 10, 10, 0, -120)
        mouseClick(inside, 40, 40, Qt.RightButton)
        mouseClick(inside, 40, 40, Qt.MiddleButton)
        mouseClick(inside, 40, 40, Qt.BackButton)
        untouched()
    }
    function test_touch_and_scroll_gestures_stay_inside() {
        open()
        touchEvent(inside).press(0, inside, 40, 40).commit()
        touchEvent(inside).release(0, inside, 40, 40).commit()
        wait(0)
        compare(insideClicks, 1)
        mouseDrag(scrolling, 50, 60, 0, -45)
        tryVerify(() => scrolling.contentY > 0)
        untouched()
    }
    function test_opening_cancels_a_background_press() {
        mousePress(test, 10, 10)
        dialog.open()
        mouseRelease(test, 10, 10)
        untouched()
    }
    function test_nested_menu_preserves_owner_focus_window_and_visualizer() {
        backgroundField.forceActiveFocus()
        const active = test.Window.window.active
        const releases = feed.releases
        open()
        verify(dialog.activeFocus)
        mouseClick(menuButton, 10, 10)
        tryCompare(context, "opened", true)
        const options = findChild(context, "menuOptions")
        tryVerify(() => options.itemAtIndex(0) !== null)
        mouseClick(options.itemAtIndex(0), 10, 10)
        tryCompare(context, "visible", false)
        compare(menuChoices, 1)
        verify(dialog.opened && dialog.enabled && dialog.activeFocus)
        compare(test.Window.window.active, active)
        compare(feed.releases, releases)
        verify(band.capturing)
        dialog.close()
        tryCompare(dialog, "visible", false)
        verify(backgroundField.activeFocus)
        compare(feed.releases, releases)
        untouched()
    }
    function test_opening_restores_the_window_enabled_binding() {
        test.Window.window.contentItem.enabled = Qt.binding(() => test.windowEnabled)
        open()
        verify(test.Window.window.contentItem.enabled)
        windowEnabled = false
        compare(test.Window.window.contentItem.enabled, false)
        windowEnabled = true
        compare(test.Window.window.contentItem.enabled, true)
    }
    function test_fields_keep_typing_and_escape_closes_the_modal() {
        open()
        mouseClick(field, 10, 10)
        verify(field.activeFocus)
        keyClick(Qt.Key_A, Qt.ControlModifier)
        keyClick(Qt.Key_X)
        compare(field.text, "x")
        untouched()
        keyClick(Qt.Key_Escape)
        tryCompare(dialog, "visible", false)
    }

    function test_inside_press_stays_in_the_modal() {
        dialog.open()
        tryVerify(() => dialog.opened)
        mouseClick(inside, inside.width / 2, inside.height / 2)
        compare(insideClicks, 1)
        compare(backgroundClicks, 0)
        compare(backgroundTaps, 0)
        verify(dialog.opened)
    }

    function test_outside_dismissal_does_not_click_the_page() {
        dialog.open()
        tryVerify(() => dialog.opened)
        mouseClick(test, 10, 10)
        tryVerify(() => !dialog.opened)
        compare(backgroundClicks, 0)
        compare(backgroundTaps, 0)
    }

    function test_a_mouse_area_inside_receives_its_click() {
        dialog.open()
        tryVerify(() => dialog.opened)
        mouseClick(mouseInside, mouseInside.width / 2, mouseInside.height / 2)
        compare(insideMouseClicks, 1)
        compare(backgroundClicks, 0)
        compare(backgroundTaps, 0)
        verify(dialog.opened)
    }
}
