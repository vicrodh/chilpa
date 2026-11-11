import QtQuick
import QtTest
import "../../qml/Components"

// A field laid over by a sidecar says what is under it and gives it back on
// a click; a field the server does not keep cannot be changed while saving
// to the server.
TestCase {
    id: test
    name: "MetadataField"
    when: windowShown
    visible: true
    width: 600; height: 200
    MetadataField { id: field; width: test.width; hosted: true }
    SignalSpy { id: edits; target: field; signalName: "edited" }

    function init() { edits.clear() }

    function test_a_click_on_what_lays_it_over_takes_it_off() {
        field.field = { name: "title", value: "Mine", overridden: true, beneath: "Theirs", editable: true }
        const layer = findChild(field, "metadataLaidOver")
        verify(layer.visible)
        mouseClick(layer)
        compare(edits.count, 1)
        compare([edits.signalArguments[0][0], edits.signalArguments[0][1]], ["title", "Theirs"], "the server's value, typed back")
        field.field = { name: "title", value: "Theirs", overridden: false, beneath: "", editable: true }
        verify(!layer.visible, "nothing laid over it any more")
    }

    function test_a_field_the_server_does_not_keep_is_not_typed_in() {
        field.field = { name: "catalog_number", value: "CAT-1", editable: false, invalid: "" }
        const input = findChild(field, "metadataNotKept")
        verify(input.enabled, "it says why on hover")
        field.field = { name: "compilation", value: "false", editable: false }
        const toggles = []
        const walk = (item) => { if ("checked" in item && "toggled" in item) toggles.push(item); for (const c of item.children || []) walk(c) }
        walk(field)
        compare(toggles.length, 1)
        verify(!toggles[0].enabled, "the compilation switch cannot be flipped either")
    }
}
