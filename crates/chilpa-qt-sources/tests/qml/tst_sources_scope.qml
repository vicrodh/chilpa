import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import "../../qml/Patterns"

// Sources in the Library's crumb line (F8): the sources on by name, "All
// music" with every one on, and a popover with one switch each that moves
// the same libraries as the settings popover.
TestCase {
    id: test
    name: "SourcesScope"
    when: windowShown
    visible: true
    width: 500; height: 400

    SourcesScope { id: scope; x: 10; y: 10 }

    function init() {
        Sources.asked = []
        Library.asked = []
        Library.sourceKeys = ["local:default", "plugin:fixture.one"]
        Library.sourceLabels = ["", "First source"]
        Library.sourceCounts = ["120", "30"]
        Library.sourceIcons = ["", ""]
        Library.excludedSources = []
        Sources.show({albumLibraries: [{id: "fixture.one", name: "First source", enabled: true, icon: ""}, {id: "fixture.two", name: "Second source", enabled: false, icon: ""}],
            trackLibraries: []})
    }
    function rows() {
        const out = []
        const walk = (item) => { for (const child of item.children) { if (child.objectName === "sourcesScopeRow") out.push(child); walk(child) } }
        walk(findChild(scope, "sourcesScopePopup").contentItem)
        return out
    }

    function test_the_label_names_the_sources_on() {
        tryCompare(findChild(scope, "sourcesLabel"), "text", "Local · First")
        Sources.show({albumLibraries: [{id: "fixture.one", name: "First source", enabled: true, icon: ""}], trackLibraries: []})
        tryCompare(findChild(scope, "sourcesLabel"), "text", "All music")
    }

    function test_the_popover_switches_a_source() {
        mouseClick(findChild(scope, "sourcesScope"))
        tryVerify(() => scope.open, 1000)
        tryVerify(() => rows().length === 3, 2000)
        const second = findChild(findChild(scope, "sourcesScopePopup").contentItem, "sourcesScope-plugin:fixture.two")
        compare(second.checked, false)
        mouseClick(second)
        compare(Sources.asked, [["album_library", "fixture.two", "true"]])
        // The rows are built again after a switch: find them once settled.
        waitForRendering(findChild(scope, "sourcesScopePopup").contentItem)
        const local = findChild(findChild(scope, "sourcesScopePopup").contentItem, "sourcesScope-local:default")
        mouseClick(local)
        compare(Library.asked.length, 1, "the library's own files are kept out through the filter")
        keyClick(Qt.Key_Escape)
        tryVerify(() => !scope.open, 1000)
    }
}
