import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import "../../qml/Patterns"

// The search box and the results page: without the preview, a pause in
// typing opens the page; with it, Enter with no row chosen does.
TestCase {
    name: "SearchBoxPage"
    when: windowShown
    visible: true
    width: 600
    height: 400

    SearchBox { id: box; width: 300; room: 300 }

    function field() {
        const walk = node => { if (node.hasOwnProperty("placeholder") && node.hasOwnProperty("shortcut")) return node; for (const child of node.children || []) { const found = walk(child); if (found) return found } return null }
        return walk(box)
    }
    function cleanup() {
        field().text = ""
        SettingsModel.keep("navigation.searchPreview", "true")
        Prefs.changed()
        Navigation.went = []
    }

    function test_without_the_preview_a_pause_in_typing_opens_the_page() {
        SettingsModel.keep("navigation.searchPreview", "false")
        Prefs.changed()
        verify(!box.preview)
        field().text = "mora vale"
        tryCompare(Navigation, "went", ["search", "mora vale"], 2000)
    }

    function test_with_the_preview_enter_on_no_row_opens_the_page() {
        verify(box.preview)
        field().text = "mora"
        field().accepted()
        compare(Navigation.went, ["search", "mora"])
    }
}
