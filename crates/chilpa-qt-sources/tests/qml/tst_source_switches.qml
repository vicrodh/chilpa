import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import "../../qml/Patterns"

// One switch per source for Library, Albums and Tracks: a source's switch
// moves both its libraries and never leaves it kept out; the library's own
// files are only shown or kept out; a source turned off stays listed.
TestCase {
    name: "SourceSwitches"
    when: windowShown
    visible: true
    width: 400; height: 300

    SourceSwitches { id: switches; active: true; width: 360 }

    function init() {
        Sources.asked = []
        Library.asked = []
        Library.sourceKeys = ["local:default", "plugin:fixture.one"]
        Library.sourceLabels = ["", "First source"]
        Library.sourceCounts = ["120", "30"]
        Library.sourceIcons = ["", ""]
        Library.excludedSources = []
        Sources.show({albumLibraries: [{id: "fixture.one", name: "First source", enabled: true, icon: ""}, {id: "fixture.two", name: "Second source", enabled: false, icon: ""}],
            trackLibraries: [{id: "fixture.one", name: "First source", enabled: true, icon: ""}, {id: "fixture.two", name: "Second source", enabled: false, icon: ""}]})
        // The rows are built again on every change: find them once settled.
        waitForRendering(switches)
    }

    function test_a_source_moves_both_its_libraries_with_one_switch() {
        const first = findChild(switches, "sourceSwitch-plugin:fixture.one")
        verify(first)
        compare(first.checked, true)
        mouseClick(first)
        compare(Sources.asked, [["album_library", "fixture.one", "false"], ["track_library", "fixture.one", "false"]])
        compare(Library.asked, [], "a source not kept out is not touched in the filter")
    }

    function test_a_source_turned_off_stays_listed_and_turns_on_again() {
        const second = findChild(switches, "sourceSwitch-plugin:fixture.two")
        verify(second, "listed though it shows nothing now")
        compare(second.checked, false)
        mouseClick(second)
        compare(Sources.asked, [["album_library", "fixture.two", "true"], ["track_library", "fixture.two", "true"]])
    }

    function test_a_kept_out_source_is_let_back_in_and_the_library_is_only_shown_or_kept_out() {
        Library.excludedSources = ["plugin:fixture.one"]
        waitForRendering(switches)
        const first = findChild(switches, "sourceSwitch-plugin:fixture.one")
        tryCompare(first, "checked", false)
        mouseClick(first)
        compare(Library.asked, [["source", "plugin:fixture.one"]])
        Library.asked = []
        Library.excludedSources = []
        waitForRendering(switches)
        mouseClick(findChild(switches, "sourceSwitch-local:default"))
        compare(Library.asked, [["source", "local:default"]])
        compare(Sources.asked.filter(call => call[1] === "local:default"), [])
    }
}
