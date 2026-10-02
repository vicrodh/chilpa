import QtQuick
import QtTest
import "../../qml/Components"
import "../../qml/Theme"

TestCase {
    name: "PluginIcon"
    when: windowShown
    visible: true
    width: 960
    height: 600

    NavItem { id: entry; width: 230; text: "Guest"; glyph: Icons.letterG }

    function test_fallback_letter_exists_and_stays_visible_without_an_image() {
        entry.iconSource = ""
        verify(Icons.letterG.length > 0)
        verify(Icons.letterX.length > 0)
        tryCompare(findChild(entry, "navGlyph"), "visible", true)
        compare(findChild(entry, "navImage").visible, false)
        entry.collapsed = true
        compare(findChild(entry, "navGlyph").x, (entry.width - Metrics.iconNav) / 2)
        entry.collapsed = false
    }

    function test_a_valid_plugin_image_replaces_the_letter_and_removal_restores_it() {
        entry.iconSource = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgYGD4DwABBAEAX+XDSwAAAABJRU5ErkJggg=="
        tryCompare(findChild(entry, "navImage"), "status", Image.Ready)
        compare(findChild(entry, "navGlyph").visible, false)
        entry.iconSource = ""
        tryCompare(findChild(entry, "navGlyph"), "visible", true)
    }
}
