import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.query
import "../../../chilpa-qt/qml/Screens"
import "../../../chilpa-qt-sources/qml/Patterns"

// Text typed in a column's header narrows the library as a condition does,
// so it stands on the crumb line: with the columns shown, with Refine open
// and with the area under the line hidden. Its x clears the header and its
// box; "Clear all" in Refine, Reset in the sources popover and the empty
// result's offer clear it too.
TestCase {
    id: test
    name: "ColumnSearchCrumbs"
    when: windowShown
    visible: true
    width: 1200
    height: 800
    Component { id: screenFactory; LibraryScreen { width: 1100; height: 760 } }
    Component { id: popoverFactory; SourcesPopover { panes: ["year", "artist"] } }

    function init() {
        I18n.choose("en")
        Library.queryText = ""
        Library.queries = ["", "", "", "", "", "", "", "", ""]
        Library.busy = false
        SettingsModel.preferences = ({})
        SettingsModel.panesOpen = true
        SettingsModel.keep("ui.library.focus", "false")
        SettingsModel.keep("ui.library.columns", JSON.stringify(["year", "genre", "artist", "album"]))
    }
    function all(root, name) {
        const out = []
        const walk = (item) => { for (const child of item.children) { if (child.objectName === name) out.push(child); walk(child) } }
        walk(root)
        return out
    }
    function shown(item) {
        for (let at = item; at; at = at.parent)
            if (!at.visible || at.opacity === 0) return false
        return item.width > 0
    }
    function crumbs(root) { return all(root, "columnSearchCrumb").filter(shown) }
    // The crumb's x, at its right end, held as a hand holds it.
    function remove(crumb) { mouseClick(crumb, crumb.width - 12, crumb.height / 2, Qt.LeftButton, Qt.NoModifier, 50) }

    function test_a_header_search_stands_on_the_crumb_line_and_its_x_clears_it_and_its_box() {
        const page = createTemporaryObject(screenFactory, test)
        tryVerify(() => all(page, "paneSearch").length > 0, 2000)
        const box = all(page, "paneSearch").find(input => input.parent.parent.column === "artist")
        verify(box, "the artist column's box")
        box.forceActiveFocus()
        for (const key of [Qt.Key_A, Qt.Key_N, Qt.Key_N, Qt.Key_E]) keyClick(key)
        compare(Library.queries[5], "anne", "typing in the header searches the column")
        tryVerify(() => crumbs(page).length === 1, 2000, "the header search stands on the crumb line")
        const crumb = crumbs(page)[0]
        compare(crumb.name, "Artist")
        compare(crumb.value, "“anne”")
        remove(crumb)
        compare(Library.queries[5], "", "the x clears the header")
        compare(box.text, "", "and the column's box with it")
        tryVerify(() => crumbs(page).length === 0, 2000)
    }

    function test_the_crumb_stays_with_refine_open_and_with_the_area_hidden() {
        SettingsModel.keep("ui.library.focus", "true")
        const page = createTemporaryObject(screenFactory, test)
        Library.searchColumn("artist", "anne")
        Library.searchColumn("genre", "rock")
        Library.searchColumn("rating", "4")
        tryVerify(() => crumbs(page).length === 2, 2000, "one crumb per header that narrows the library; the rating's narrows only its column")
        SettingsModel.panesOpen = false
        tryVerify(() => crumbs(page).length === 2, 2000, "with the area under the line hidden")
    }

    function test_clear_all_in_refine_clears_the_header_searches_too() {
        SettingsModel.keep("ui.library.focus", "true")
        const page = createTemporaryObject(screenFactory, test)
        Library.queryText = "genre:jazz"
        Library.searchColumn("artist", "anne")
        tryVerify(() => all(page, "refineClear").some(shown), 2000, "Refine's Clear all")
        mouseClick(all(page, "refineClear").find(shown))
        compare(Library.queries[5], "", "Clear all clears what a header narrows")
        compare(Library.queryText, "")
    }

    function test_reset_in_the_sources_popover_clears_the_header_searches() {
        const popover = createTemporaryObject(popoverFactory, test)
        popover.open()
        tryVerify(() => popover.opened)
        const reset = findChild(popover.contentItem, "ratingsReset")
        verify(!reset.enabled, "nothing to reset")
        Library.searchColumn("artist", "anne")
        verify(reset.enabled, "a header search is something to reset")
        mouseClick(reset)
        compare(Library.queries[5], "")
        popover.close()
    }

    function test_an_empty_result_offers_to_clear_a_header_search() {
        const page = createTemporaryObject(screenFactory, test)
        Library.searchColumn("artist", "zzz")
        tryVerify(() => all(page, "zeroSearchOffer").some(shown), 2000, "with no records, the header search is offered away")
        mouseClick(all(page, "zeroSearchOffer").find(shown))
        compare(Library.queries[5], "")
    }
}
