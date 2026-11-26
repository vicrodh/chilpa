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
        doc: test.files
    }

    QtObject {
        id: test
        readonly property var files: ({ state: "ready", dirty: false, error: "", notice: "", album: [{ name: "title", value: "Low Orbit" }], tracks: [{}, {}],
                                        save: { mode: "sidecar", hosted: false, files_allowed: true, files_refused: "", xml_allowed: false }, layers: { present: [] } })
        function hosted(mode, allowed, refused) {
            return { state: "ready", dirty: true, error: "", notice: "", album: [{ name: "title", value: "Low Orbit" }], tracks: [{}],
                     save: { mode: mode, hosted: true, files_allowed: false, files_refused: "a record on a server has no files here to write to", xml_allowed: false,
                             server: { name: "Living room", allowed: allowed, refused: refused } },
                     layers: { present: [] } }
        }
    }

    function init() {
        header.doc = test.files
        MetadataEditor.modes = []
    }

    SignalSpy { id: dropAsked; target: header; signalName: "dropSidecarAsked" }

    function test_a_sidecar_on_this_machine_can_be_deleted_from_the_head() {
        const button = findChild(header, "metadataDropSidecar")
        verify(!button.visible, "nothing to delete without a sidecar")
        const kept = JSON.parse(JSON.stringify(test.files))
        kept.save.sidecar_kept = true
        header.doc = kept
        verify(button.visible && button.enabled)
        dropAsked.clear()
        button.clicked()
        compare(dropAsked.count, 1, "the screen asks before anything goes")
        const saving = JSON.parse(JSON.stringify(kept))
        saving.state = "saving"
        header.doc = saving
        verify(!button.enabled)
    }

    function test_a_record_of_files_chooses_where_a_save_goes() {
        verify(findChild(header, "metadataSaveTo").visible, "the sidecar or the files")
        verify(!findChild(header, "metadataSaveToServer").visible)
        verify(!findChild(header, "metadataSaveToSays").visible)
    }

    function test_a_record_on_a_server_saves_to_the_server_and_says_so() {
        header.doc = test.hosted("server", true, "")
        const choice = findChild(header, "metadataSaveToServer")
        verify(choice.visible)
        verify(!findChild(header, "metadataSaveTo").visible, "no file to write to")
        compare(choice.current, 0)
        compare(findChild(header, "metadataSaveToSays").text, "Saved to Living room and read back from it.")
        choice.activated(1)
        compare(MetadataEditor.modes, ["sidecar"], "the sidecar only when chosen")
    }

    function test_a_server_that_cannot_take_the_changes_leaves_the_choice_to_the_listener() {
        header.doc = test.hosted("", false, "only an administrator of this server can change its metadata")
        const choice = findChild(header, "metadataSaveToServer")
        compare(choice.current, -1, "nothing is chosen for the listener")
        verify(choice.options[0].disabled, "the server is shown, and cannot be chosen")
        compare(choice.options[0].tip, "only an administrator of this server can change its metadata")
        verify(findChild(header, "metadataSaveToSays").text.indexOf("only an administrator") >= 0, "and why is said")
        verify(waitForRendering(choice), "the replacement options must settle before a pointer click")
        mouseClick(choice, 8, choice.height / 2)
        compare(MetadataEditor.modes, [], "a click on the server chooses nothing")
        choice.forceActiveFocus()
        keyClick(Qt.Key_Right)
        compare(MetadataEditor.modes, ["sidecar"], "the keyboard steps over it to the sidecar")
    }

    function test_the_sidecar_says_it_stays_on_this_device() {
        header.doc = test.hosted("sidecar", true, "")
        compare(findChild(header, "metadataSaveToServer").current, 1)
        compare(findChild(header, "metadataSaveToSays").text, "Kept on this device, over what the server says, until you take it off.")
    }

    function test_a_protected_library_greys_out_what_it_refuses() {
        const doc = JSON.parse(JSON.stringify(test.files))
        doc.dirty = true
        doc.save = { mode: "", hosted: false, files_allowed: false, files_refused: "Protect my library is on.", xml_allowed: false,
                     protected: true, sidecar_allowed: false }
        header.doc = doc
        const where = findChild(header, "metadataSaveTo")
        compare(where.options[0].disabled, true, "no sidecar without the child")
        compare(where.options[1].disabled, true, "no files")
        compare(findChild(header, "metadataSave").enabled, false)
        doc.save.sidecar_allowed = true
        header.doc = JSON.parse(JSON.stringify(doc))
        compare(findChild(header, "metadataSaveTo").options[0].disabled, false)
        compare(findChild(header, "metadataSave").enabled, true)
    }

    function test_a_protected_record_with_an_xml_still_shows_it_disabled() {
        const doc = JSON.parse(JSON.stringify(test.files))
        doc.save = { mode: "", hosted: false, files_allowed: false, files_refused: "Protect my library is on.", xml_allowed: true,
                     xml_refused: "Protect my library is on.", protected: true, sidecar_allowed: true }
        header.doc = doc
        const options = findChild(header, "metadataSaveTo").options
        compare(options.length, 3, "the XML option is shown")
        compare(options[2].disabled, true, "and greyed")
        compare(options[2].tip, "Protect my library is on.")
    }

    function test_a_protected_server_record_says_only_the_protection_when_sidecars_are_allowed() {
        const doc = test.hosted("", true, "Protect my library is on.")
        doc.save.protected = true
        doc.save.sidecar_allowed = true
        header.doc = doc
        compare(findChild(header, "metadataSaveToSays").text, "Protect my library is on.")
        compare(findChild(header, "metadataSaveToServer").options[1].disabled, false, "the sidecar stays open")
    }
}
