import QtQuick
import QtTest
import "../../../chilpa-qt/qml/Primitives"

TestCase {
    name: "RichSelect"
    when: windowShown
    visible: true
    width: 700; height: 600
    SelectField { id: field; x: 100; y: 30; options: ["One", "Two", "Three", "Four", "Five", "Six"]; values: ["a", "b", "c", "d", "e", "f"]; details: ["", "", "", "", "", "High resolution"]; value: "a" }
    SignalSpy { id: picked; target: field; signalName: "picked" }
    function cleanup() { findChild(field, "selectOptions").close(); picked.clear() }
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
