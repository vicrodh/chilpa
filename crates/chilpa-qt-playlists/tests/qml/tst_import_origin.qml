import QtQuick
import QtTest
import app.chilpa
import "../../qml/Patterns"

// The origin step offers the importer's own origins, asks for what the chosen
// one needs, and says so while the list is read.
TestCase {
    id: test
    name: "ImportOrigin"
    when: windowShown
    visible: true
    width: 900
    height: 600
    readonly property var inputs: [
        {id: "url", name: "Playlist link", hint: "A public playlist link", kind: "text"},
        {id: "file", name: "Playlist file", hint: "M3U or XSPF", kind: "file"},
        {id: "list", name: "Service", hint: "A user name", kind: "choices", private_token: true},
        {id: "station", name: "Stations", hint: "A user name", kind: "station", choices: [{id: 0, name: "Library"}, {id: 1, name: "Mix"}]}]
    ImportOriginStep { id: step; width: 760; inputs: test.inputs }
    function init() {
        step.width = 760
        step.chosen = ""
        step.file = ""
        step.reading = false
        findChild(step, "importOriginText").text = ""
    }
    function test_the_origins_are_the_importers_own() {
        const kinds = findChild(step, "importOrigins")
        verify(kinds.visible)
        compare(kinds.options.map(o => o.text), ["Playlist link", "Playlist file", "Service", "Stations"])
        compare(kinds.current, 0)
        verify(!findChild(step, "importOriginSelect").visible)
    }
    function test_too_many_to_fit_become_a_select() {
        step.width = 200
        verify(!findChild(step, "importOrigins").visible)
        verify(findChild(step, "importOriginSelect").visible)
    }
    function test_a_text_origin_is_ready_with_text_and_says_what_it_reads() {
        verify(!step.ready)
        findChild(step, "importOriginText").text = "https://example.org/list"
        verify(step.ready)
        compare(step.payload().input, "url")
        compare(step.payload().text, "https://example.org/list")
    }
    function test_a_file_origin_waits_for_its_file() {
        findChild(step, "importOrigins").activated(1)
        verify(!findChild(step, "importOriginText").visible)
        verify(findChild(step, "importChooseFile").visible)
        verify(!step.ready)
        step.file = "file:///music/list.m3u"
        verify(step.ready)
        compare(step.payload().file, "file:///music/list.m3u")
    }
    function test_a_station_is_chosen_by_its_index() {
        findChild(step, "importOrigins").activated(3)
        const stations = findChild(step, "importStation")
        verify(stations.visible)
        stations.activated(1)
        compare(step.payload().station, 1)
    }
    function test_finding_a_services_lists_sends_the_text_and_token() {
        findChild(step, "importOrigins").activated(2)
        findChild(step, "importOriginText").text = "someone"
        let asked = null
        const take = payload => asked = payload
        step.findChoices.connect(take)
        findChild(step, "importFindChoices").clicked()
        step.findChoices.disconnect(take)
        compare(asked.text, "someone")
    }
    function test_the_wait_shows_only_while_reading() {
        verify(!findChild(step, "importReading").visible)
        step.reading = true
        verify(findChild(step, "importReading").visible)
    }
}
