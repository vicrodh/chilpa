import QtQuick
import QtTest
import app.chilpa
import app.chilpa.query
import "../../qml/Components"
import "../../qml/Patterns"
import "../../../chilpa-qt/qml/Screens"

// The area under the crumb line: the switch keeps its choice, a second
// click hides it and only the surface shown is alive; the Library screen
// builds both.
TestCase {
    id: test
    name: "RefineArea"
    when: windowShown
    visible: true
    width: 1200
    height: 700
    Component { id: screenFactory; LibraryScreen { width: 1100; height: 650 } }
    Component { id: switchFactory; RefineSwitch {} }
    Component { id: areaFactory; RefineArea { width: 1100; height: 400; columns: Component { Item { objectName: "testColumns" } } } }

    function initTestCase() { failOnWarning(/.*/) }
    function init() {
        Library.queryText = ""
        SettingsModel.preferences = ({})
        SettingsModel.panesOpen = false
    }

    function test_switch_persists_mode_hides_on_second_click_and_releases_surface() {
        const control = createTemporaryObject(switchFactory, test)
        const area = createTemporaryObject(areaFactory, test, { y: 80 })
        area.refineMode = Qt.binding(() => control.refineMode)
        area.visible = Qt.binding(() => SettingsModel.panesOpen)
        compare(area.item, null)
        mouseClick(control, control.width / 4, control.height / 2)
        tryVerify(() => area.item && area.item.objectName === "testColumns")
        mouseClick(control, control.width * 3 / 4, control.height / 2)
        tryVerify(() => area.item && area.item.objectName === "refinePanel")
        compare(SettingsModel.kept("ui.library.focus"), "true")
        const restored = createTemporaryObject(switchFactory, test, { y: 40 })
        compare(restored.refineMode, true)
        mouseClick(control, control.width * 3 / 4, control.height / 2)
        tryCompare(area, "item", null)
        compare(SettingsModel.panesOpen, false)
    }

    function test_library_screen_builds_both_surfaces() {
        SettingsModel.keep("ui.library.focus", "true")
        SettingsModel.panesOpen = true
        const screen = createTemporaryObject(screenFactory, test)
        verify(screen)
        tryVerify(() => findChild(screen, "refinePanel") !== null)
        const control = findChild(screen, "refineSwitch")
        mouseClick(control, control.width / 4, control.height / 2)
        tryVerify(() => findChild(screen, "refinePanel") === null)
    }

    function test_a_narrow_library_stacks_the_panel_and_quiets_the_switch() {
        SettingsModel.keep("ui.library.focus", "true")
        SettingsModel.panesOpen = true
        const screen = createTemporaryObject(screenFactory, test, { width: 680 })
        tryVerify(() => findChild(screen, "refinePanel") !== null)
        verify(findChild(screen, "refinePanel").narrow, "under 640 of content the labels go above the values")
        verify(findChild(screen, "refineSwitch").narrow, "and the switch loses its words")
        screen.width = 1100
        tryVerify(() => !findChild(screen, "refinePanel").narrow)
    }
}
