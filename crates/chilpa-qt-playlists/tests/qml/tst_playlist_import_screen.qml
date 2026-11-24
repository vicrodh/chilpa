import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import "../../qml/Screens"

TestCase {
    id: test
    name: "PlaylistImportScreen"
    when: windowShown
    visible: true
    width: 1000
    height: 700
    PlaylistImportScreen { id: page; anchors.fill: parent }
    function document(state, pending) {
        return {installed: true, plugin: "com.test.import", sources: [], folders: [], choices: {}, error: "", pending: pending,
            options: {inputs: [{id: "url", name: "Playlist link", hint: "", kind: "text"},
                {id: "list", name: "Service", hint: "", kind: "choices"}]}, state: state}
    }
    function init() {
        PlaylistImport.asked = []
        PlaylistImport.show(document({phase: "input"}, false))
        page.step = 0
        page.sent = ""
    }
    // Reading a list takes a while; the page says so with a turning mark
    // until the preview is in, and only for a read.
    function test_reading_a_list_shows_a_mark_until_its_preview_is_in() {
        const reading = findChild(page, "importReading")
        verify(reading !== null)
        verify(!reading.visible, "nothing is read yet")
        findChild(page, "importPrimary").clicked()
        compare(PlaylistImport.asked[0].action, "preview")
        PlaylistImport.show(document({phase: "input"}, true))
        verify(reading.visible, "the read is in flight")
        PlaylistImport.show(document({phase: "fetching", busy: true}, false))
        verify(reading.visible, "the plugin reads it")
        PlaylistImport.show(document({phase: "preview", busy: false, preview: {name: "List", total: 3}}, false))
        compare(page.step, 1)
        verify(!reading.visible, "the preview is in")
    }
    function test_finding_a_services_lists_is_no_read() {
        const reading = findChild(page, "importReading")
        page.send("choices", {text: "someone"})
        PlaylistImport.show(document({phase: "input"}, true))
        verify(!reading.visible)
    }
}
