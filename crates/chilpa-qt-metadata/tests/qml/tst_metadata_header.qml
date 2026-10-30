import QtQuick
import QtTest
import app.chilpa
import app.chilpa.metadata
import "../../qml/Patterns"

TestCase {
    name: "MetadataHeader"
    when: windowShown
    visible: true
    width: 900
    height: 300

    MetadataHeader {
        id: header
        width: 860
        doc: ({ state: "ready", dirty: false, error: "", notice: "", album: [{ name: "title", value: "Low Orbit" }], tracks: [{}, {}],
                save: { mode: "sidecar", hosted: false, files_allowed: true, files_refused: "", xml_allowed: false }, layers: { present: [] } })
    }

    function test_a_record_of_files_chooses_where_a_save_goes() {
        const choice = findChild(header, "metadataSaveTo")
        const device = findChild(header, "metadataOnThisDevice")
        verify(choice.visible, "the sidecar or the files")
        verify(!device.visible)
    }

    function test_a_record_on_a_server_says_its_changes_stay_on_this_device() {
        header.doc = { state: "ready", dirty: false, error: "", notice: "", album: [{ name: "title", value: "Low Orbit" }], tracks: [{}],
                       save: { mode: "sidecar", hosted: true, files_allowed: false, files_refused: "a record on a server keeps its changes on this device", xml_allowed: false },
                       layers: { present: [] } }
        const choice = findChild(header, "metadataSaveTo")
        const device = findChild(header, "metadataOnThisDevice")
        verify(!choice.visible, "there is nothing to choose")
        verify(device.visible)
        compare(device.text, "On this device")
    }
}
