import QtQuick
import QtTest
import app.chilpa
import "../../qml/Patterns"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

TestCase {
    name: "SettingsFocus"
    when: windowShown
    property var objects: []
    QtObject { id: row }
    QtObject {
        id: sampleViewport
        property int count: 2
        property int positioned: -1
        function positionViewAtIndex(index, mode) { positioned = index }
        function itemAtIndex(index) { return row }
    }
    QtObject {
        id: sampleModel
        function index(row, column) { return row }
        function data(index, role) { return ["library.rescan", "library.folders"][index] }
    }
    QtObject { id: sampleHighlight; property var target: null }
    Component {
        id: seeker
        SettingsFocus { section: "appearance"; viewport: sampleViewport; model: sampleModel; highlight: sampleHighlight }
    }
    function create(section) {
        const object = seeker.createObject(this, {section: section})
        verify(object !== null)
        objects.push(object)
        return object
    }
    function init() {
        Navigation.screen = "settings"
        Navigation.section = "appearance"
        sampleViewport.positioned = -1
        sampleHighlight.target = null
        Visit.keep("settings.focus", null)
    }
    function cleanup() {
        for (const object of objects) object.destroy()
        objects = []
        Visit.keep("settings.focus", null)
    }
    function test_search_target_survives_the_destination_mount() {
        const previous = create("appearance")
        previous.go({section: "library", key: "library.folders"})
        compare(previous.section, "appearance")
        compare(sampleViewport.positioned, -1)
        previous.leaving = true
        const destination = create("library")
        tryCompare(destination, "row", 1)
        compare(sampleViewport.positioned, 1)
        compare(sampleHighlight.target, row)
        compare(Visit.recall("settings.focus", null), null)
    }
    function test_an_interrupted_navigation_clears_its_search_target() {
        const previous = create("appearance")
        previous.go({section: "library", key: "library.folders"})
        Navigation.screen = "home"
        Navigation.section = ""
        previous.leaving = true
        compare(Visit.recall("settings.focus", null), null)
        const destination = create("library")
        compare(destination.key, "")
        compare(destination.row, -1)
    }
}
