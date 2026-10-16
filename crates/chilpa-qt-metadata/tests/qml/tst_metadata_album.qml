import QtQuick
import QtTest
import app.chilpa
import app.chilpa.metadata
import "../../qml/Patterns"

TestCase {
    name: "MetadataAlbum"
    when: windowShown
    visible: true
    width: 700
    height: 1200

    MetadataAlbum {
        id: form
        width: 600
        height: 1200
        doc: ({ album: [{ name: "title", value: "Low Orbit" }, { name: "label", value: "Hornet Records" }] })
    }

    function fieldsNamed(name) {
        const found = []
        const walk = (item) => {
            if (item.field && item.field.name === name && item.visible) found.push(item)
            for (const child of item.children || []) walk(child)
        }
        walk(form)
        return found
    }

    function test_the_record_has_a_field_for_its_label() {
        const label = fieldsNamed("label")
        compare(label.length, 1, "one field for the label")
        compare(label[0].field.value, "Hornet Records")
    }
}
