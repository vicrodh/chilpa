import QtQuick
import QtTest
import "../../qml/Components"

TestCase {
    name: "MetadataReplace"
    when: windowShown
    visible: true
    width: 700
    height: 600

    Item { id: host; anchors.fill: parent }
    MetadataReplace {
        id: dialog
        parent: host
        fields: [{ key: "album:title", label: "Album" }, { key: "track:title", label: "Title" }]
    }
    SignalSpy { id: asked; target: dialog; signalName: "replaceAsked" }

    function part(name) { return findChild(dialog.contentItem, name) }

    function test_a_text_is_replaced_in_the_chosen_field_with_the_chosen_rules() {
        asked.clear()
        dialog.open()
        tryVerify(() => dialog.opened)
        compare(dialog.field, "album:title", "the first field until another is chosen")
        const apply = part("metadataReplaceApply")
        verify(!apply.enabled, "nothing to find, nothing to do")
        part("metadataReplaceField").picked("track:title")
        part("metadataReplaceFind").text = "Live"
        part("metadataReplaceWith").text = "Vivo"
        part("metadataReplaceWhole").toggled(true)
        part("metadataReplaceCase").toggled(true)
        verify(apply.enabled)
        apply.clicked()
        compare(asked.count, 1)
        compare(Array.from(asked.signalArguments[0]), ["track:title", "Live", "Vivo", true, true])
        tryVerify(() => !dialog.opened, 1000, "it closes once asked")
    }
}
