import QtQuick
import QtTest
import "../../../chilpa-qt-kit/qml/Primitives"
import "../../../chilpa-qt-kit/qml/Theme"

TestCase {
    name: "RichSelect"
    when: windowShown
    visible: true
    width: 700; height: 600
    readonly property var startOptions: ["One", "Two", "Three", "Four", "Five", "Six"]
    readonly property var startValues: ["a", "b", "c", "d", "e", "f"]
    readonly property var startDetails: ["", "", "", "", "", "High resolution"]
    SelectField { id: field; x: 100; y: 30; options: startOptions; values: startValues; details: startDetails; value: "a" }
    SignalSpy { id: picked; target: field; signalName: "picked" }
    function cleanup() { findChild(field, "selectOptions").close(); picked.clear(); field.options = startOptions; field.values = startValues; field.details = startDetails }
    function test_filter_preserves_the_original_value_and_accepts_keyboard() {
        mouseClick(field, field.width / 2, field.height / 2)
        const menu = findChild(field, "selectOptions")
        tryCompare(menu, "opened", true)
        const search = findChild(menu, "optionSearch")
        verify(search.visible)
        search.text = "resolution"
        tryCompare(menu, "shown", [{entry:menu.entries[5],original:5}])
        search.accepted()
        compare(picked.count, 1)
        compare(picked.signalArguments[0][0], "f")
        verify(!menu.visible)
    }
    function test_a_long_select_takes_at_most_half_the_window_and_scrolls() {
        field.details = []
        field.values = Array.from({length: 60}, (_, i) => "v" + i)
        field.options = Array.from({length: 60}, (_, i) => "Option " + i)
        mouseClick(field, field.width / 2, field.height / 2)
        const menu = findChild(field, "selectOptions")
        tryCompare(menu, "opened", true)
        const list = findChild(menu, "menuOptions")
        const search = findChild(menu, "optionSearch")
        verify(search.visible, "a long select has its search")
        verify(menu.height <= height / 2 && search.height + list.height <= height / 2, "the select takes at most half the window: " + menu.height + ", " + (search.height + list.height) + " of " + height)
        verify(list.height >= Metrics.menuItem * 5, "five rows at least: " + list.height)
        verify(list.contentHeight > list.height, "the rest scrolls inside the list")
    }
    function test_five_options_stay_simple() {
        const original = field.options
        field.options = original.slice(0, 5)
        mouseClick(field, field.width / 2, field.height / 2)
        const menu = findChild(field, "selectOptions")
        tryCompare(menu, "opened", true)
        verify(!findChild(menu, "optionSearch").visible)
        field.options = original
    }
}
