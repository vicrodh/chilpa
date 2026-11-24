import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
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
        return {installed: true, plugin: "com.test.import", sources: [{id: "com.test.cloud", name: "Cloud", writable: true, icon: ""}], folders: [], choices: {}, error: "", pending: pending,
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
    function test_the_rail_names_the_steps_and_marks_the_ones_behind() {
        const rail = findChild(page, "importSteps")
        compare(rail.document.steps.map(s => s.label), ["Origin", "Destinations", "Matches", "Done"])
        compare(rail.document.step, 1)
        PlaylistImport.show(document({phase: "preview", preview: {name: "List", total: 3}}, false))
        compare(rail.document.step, 2)
        compare(rail.document.steps[0].state, "complete")
    }
    function test_back_carries_its_arrow() {
        PlaylistImport.show(document({phase: "preview", preview: {name: "List", total: 3}}, false))
        const back = findChild(page, "importBack")
        verify(back.visible)
        compare(back.glyph, Lucide.circleArrowLeft)
        verify(!back.glyphAfter)
    }
    // A page at rest asks the plugin nothing; a command or work under way does.
    function test_the_status_is_asked_only_while_something_runs() {
        const poll = findChild(page, "importPoll")
        verify(!poll.running, "nothing runs")
        PlaylistImport.show(document({phase: "input"}, true))
        verify(poll.running, "a command is in flight")
        PlaylistImport.show(document({phase: "matching", busy: true}, false))
        verify(poll.running, "the plugin works")
        PlaylistImport.show(document({phase: "ready"}, false))
        verify(!poll.running)
    }
    // Every place has the name the steps show, an account by its own.
    function test_places_name_chilpa_the_library_and_each_source() {
        compare(page.places.mixed.name, "Chilpa playlist")
        compare(page.places.local.name, "Your library")
        compare(page.places["com.test.cloud"].name, "Cloud")
        verify(page.places["com.test.cloud"].writable)
    }
}
