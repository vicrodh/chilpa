import QtQuick
import QtQuick.Window
import QtTest
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

TestCase {
    id: test
    name: "Keyboard"
    when: windowShown
    visible: true
    width: 600
    height: 700

    Column {
        id: column
        y: 10
        spacing: 8
        TextField { id: name; objectName: "name"; width: 200 }
        TextField { id: secret; objectName: "secret"; width: 200; secret: true }
        Toggle { id: toggle; objectName: "toggle" }
        SelectField { id: select; objectName: "select"; width: 200; options: ["One", "Two", "Three"]; values: ["1", "2", "3"]; value: "1"; onPicked: (v) => value = v }
        LevelSlider { id: level; objectName: "level"; value: 40; onMoved: (v) => value = v }
        PillButton { id: button; objectName: "button"; text: "Go" }
        SegmentedControl { id: segments; objectName: "segments"; options: [{ text: "A" }, { text: "B" }]; onActivated: (i) => current = i }
        SearchField { id: search; objectName: "search"; width: 200 }
    }
    Item { id: hider; y: 460; width: 200; height: 30; TextField { id: hidden; objectName: "hidden"; width: 200 } }
    Item { id: blank; y: 500; width: 600; height: 200 }
    FocusRelease { id: release }

    SignalSpy { id: toggled; target: toggle; signalName: "toggled" }
    SignalSpy { id: clicked; target: button; signalName: "clicked" }
    SignalSpy { id: finished; target: name; signalName: "editingFinished" }
    SignalSpy { id: escaped; target: search; signalName: "escaped" }

    function holder() {
        let item = test.Window.window.activeFocusItem
        while (item && item.objectName === "")
            item = item.parent
        return item ? item.objectName : ""
    }

    function test_tab_goes_through_the_controls_in_the_order_they_are_drawn() {
        name.focusInput()
        const order = []
        for (let i = 0; i < 7; i++) {
            keyClick(Qt.Key_Tab)
            order.push(holder())
        }
        compare(order, ["secret", "toggle", "select", "level", "button", "segments", "search"])
    }

    function test_each_control_answers_its_keys_and_keeps_space_from_the_window() {
        toggle.forceActiveFocus()
        keyClick(Qt.Key_Space)
        compare(toggled.count, 1)
        select.forceActiveFocus()
        keyClick(Qt.Key_Down)
        compare(select.value, "2")
        level.forceActiveFocus()
        keyClick(Qt.Key_Right)
        compare(level.value, 41)
        keyClick(Qt.Key_End)
        compare(level.value, 100)
        button.forceActiveFocus()
        keyClick(Qt.Key_Return)
        compare(clicked.count, 1)
        segments.forceActiveFocus()
        keyClick(Qt.Key_Right)
        compare(segments.current, 1)
    }

    function test_disabled_select_options_are_skipped_by_the_keyboard() {
        select.value = "1"
        select.enabledOptions = [true,false,true]
        select.forceActiveFocus()
        keyClick(Qt.Key_Down)
        compare(select.value,"3")
        keyClick(Qt.Key_Up)
        compare(select.value,"1")
        select.enabledOptions = [false,false,false]
        keyClick(Qt.Key_Down)
        compare(select.value,"1")
        select.enabledOptions = []
    }

    function test_a_press_outside_the_field_takes_the_keyboard_and_commits() {
        finished.clear()
        name.focusInput()
        keyClick(Qt.Key_A)
        verify(name.editing)
        mouseClick(blank, 20, 20)
        verify(!name.editing, "the field let go")
        compare(finished.count, 1, "and said it was done")
        name.focusInput()
        mouseClick(name, 10, name.height / 2)
        verify(name.editing, "a press on the field itself keeps it")
    }

    function test_a_field_hidden_while_it_has_the_keyboard_lets_go_for_good() {
        hidden.focusInput()
        verify(hidden.editing)
        hider.visible = false
        // The window gives the keys back to its root, a focus scope: it must
        // not hand them to the field nobody sees (the console's trap).
        test.Window.window.contentItem.forceActiveFocus()
        verify(!hidden.editing, "a hidden field holds no keys")
        hider.visible = true
        verify(!hidden.editing, "and does not take them back when it shows again")
    }

    function test_escape_empties_a_search_and_gives_the_keyboard_back() {
        search.focusInput()
        keyClick(Qt.Key_B)
        compare(search.text, "b")
        keyClick(Qt.Key_Escape)
        compare(search.text, "")
        compare(escaped.count, 1)
        verify(!search.editing)
    }
}
