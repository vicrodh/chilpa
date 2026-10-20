import QtQuick
import QtTest
import app.chilpa
import app.chilpa.query
import "../../qml/Components"
import "../../qml/Patterns"
import "../../../chilpa-qt/qml/Screens"

TestCase {
    id: test
    name: "LibraryFocus"
    when: windowShown
    visible: true
    width: 1200
    height: 700
    property var panel
    Component { id: panelFactory; FocusPanel { width: 1100; height: 500 } }
    Component { id: screenFactory; LibraryScreen { width: 1100; height: 650 } }
    Component { id: switchFactory; FocusSwitch {} }
    Component { id: areaFactory; FocusArea { width: 1100; height: 400; columns: Component { Item { objectName: "testColumns" } } } }

    function row(field, values) {
        return Object.assign({ field: field, label: field, include: [], exclude: [], range: null, within: null, missing: false, minimum: null }, values || {})
    }
    function parsed(text, rows, any, advanced) {
        const data = Object.assign({}, QueryChips.focusOf)
        data[text] = { text: text, rows: rows, any: !!any, advanced: !!advanced }
        QueryChips.focusOf = data
    }
    function expectEdit(text) { QueryChips.editedText = text; QueryChips.lastEdit = ({}) }
    function addCategory(index) {
        mouseClick(findChild(panel, "focusAdd"))
        const menu = findChild(panel, "focusCategories")
        tryCompare(menu, "opened", true)
        menu.chosen(index)
        menu.close()
    }
    function initTestCase() { failOnWarning(/.*/) }
    function init() {
        Library.queryText = ""
        QueryChips.focusOf = ({})
        QueryChips.focusTesting = true
        SettingsModel.preferences = ({})
        SettingsModel.panesOpen = false
        QueryChips.facets = {
            genre: [{ key: "", label: "All", meta: "", all: true }, { key: "Jazz", label: "Jazz", meta: "12", all: false }, { key: "Pop", label: "Pop", meta: "3", all: false }],
            album: [{ key: "", label: "All", meta: "", all: true }, { key: "41", label: "Kind of Blue", meta: "1959", all: false }],
            year: [{ key: "", label: "All", meta: "", all: true }, { key: "1980", label: "1980", meta: "1", all: false }, { key: "1950", label: "1950", meta: "2", all: false }]
        }
        panel = createTemporaryObject(panelFactory, test)
        verify(panel)
    }
    function cleanup() { QueryChips.focusTesting = false; Library.queryText = "" }

    function test_switch_persists_mode_hides_on_second_click_and_releases_surface() {
        panel.destroy()
        const control = createTemporaryObject(switchFactory, test)
        const area = createTemporaryObject(areaFactory, test, { y: 80 })
        area.focusMode = Qt.binding(() => control.focusMode)
        area.visible = Qt.binding(() => SettingsModel.panesOpen)
        compare(area.item, null)
        mouseClick(control, control.width / 4, control.height / 2)
        tryVerify(() => area.item && area.item.objectName === "testColumns")
        mouseClick(control, control.width * 3 / 4, control.height / 2)
        tryVerify(() => area.item && area.item.objectName === "focusPanel")
        compare(SettingsModel.kept("ui.library.focus"), "true")
        const restored = createTemporaryObject(switchFactory, test, { y: 40 })
        compare(restored.focusMode, true)
        mouseClick(control, control.width * 3 / 4, control.height / 2)
        tryCompare(area, "item", null)
        compare(SettingsModel.panesOpen, false)
    }

    function test_values_click_alt_search_counts_and_closed_lifecycle() {
        const before = QueryChips.liveFacets
        compare(findChild(panel, "focusPicker").item, null)
        addCategory(0)
        const list = findChild(panel, "focusValues")
        verify(list)
        tryVerify(() => list.itemAtIndex(1) !== null)
        compare(QueryChips.liveFacets, before + 1)
        compare(list.model.get(1).meta, "12")
        expectEdit("genre:Jazz")
        mouseClick(list.itemAtIndex(1))
        compare(QueryChips.lastEdit, { kind: "value", field: "genre", value: "Jazz", state: "include" })
        compare(Library.queryText, "genre:Jazz")
        expectEdit("genre:Jazz -genre:Pop")
        mouseClick(list.itemAtIndex(2), 15, 10, Qt.LeftButton, Qt.AltModifier)
        compare(QueryChips.lastQuery, "genre:Jazz")
        compare(QueryChips.lastEdit.state, "exclude")
        compare(Library.queryText, "genre:Jazz -genre:Pop")
        findChild(panel, "focusSearch").text = "jazz"
        tryCompare(list.model, "count", 2)
        waitForRendering(panel)
        mouseClick(findChild(panel, "focusCloseValues"))
        compare(panel.category, "")
        tryCompare(QueryChips, "liveFacets", before)
        addCategory(3)
        const albums = findChild(panel, "focusValues")
        tryVerify(() => albums.itemAtIndex(1) !== null)
        expectEdit('album:"Kind of Blue"')
        mouseClick(albums.itemAtIndex(1))
        compare(QueryChips.lastEdit.value, "Kind of Blue", "album names never become numeric ids")
        panel.visible = false
        tryCompare(QueryChips, "liveFacets", before)
    }

    function test_chips_toggle_and_remove_against_current_query() {
        parsed("genre:Jazz", [row("genre", { include: ["Jazz"] })])
        parsed("-genre:Jazz", [row("genre", { exclude: ["Jazz"] })])
        Library.queryText = "genre:Jazz"
        tryVerify(() => findChild(panel, "focusValueToggle") !== null)
        expectEdit("-genre:Jazz")
        mouseClick(findChild(panel, "focusValueToggle"))
        compare(QueryChips.lastEdit, { kind: "value", field: "genre", value: "Jazz", state: "exclude" })
        compare(Library.queryText, "-genre:Jazz")
        expectEdit("")
        mouseClick(findChild(panel, "focusValueRemove"))
        compare(QueryChips.lastEdit.state, "none")
        compare(QueryChips.lastQuery, "-genre:Jazz")
        compare(Library.queryText, "")
    }

    function test_year_range_uses_library_bounds_and_both_handles() {
        addCategory(1)
        const slider = findChild(panel, "focusYearRange")
        verify(slider)
        tryCompare(slider, "from", 1950)
        compare(slider.to, 1980)
        expectEdit("year:1960..1979")
        slider.setValues(1960, 1979)
        slider.first.moved()
        compare(QueryChips.lastEdit, { kind: "range", field: "year", from: 1960, to: 1979 })
        compare(Library.queryText, "year:1960..1979")
        parsed("year:1960..1979", [row("year", { range: [1960, 1979] })])
        tryVerify(() => findChild(panel, "focusYearRange") !== null)
        const second = findChild(panel, "focusYearRange")
        verify(Number.isFinite(second.parent.height))
        verify(second.parent.height >= second.height, "the active row gives both handles room")
        tryCompare(second.first, "value", 1960)
        tryCompare(second.second, "value", 1979)
        second.setValues(1960, 1970)
        expectEdit("year:1960..1970")
        second.second.moved()
        compare(QueryChips.lastEdit.to, 1970)
        compare(Library.queryText, "year:1960..1970")
    }

    function test_added_rating_missing_and_top_level_join() {
        parsed("added:1m", [row("added", { within: 30 })])
        expectEdit("added:1m")
        addCategory(7)
        compare(Library.queryText, "added:1m")
        const added = findChild(panel, "focusAdded")
        verify(added)
        expectEdit("added:1w")
        added.picked("7")
        compare(QueryChips.lastEdit, { kind: "within", field: "added", days: 7 })
        compare(Library.queryText, "added:1w")
        parsed("rating:3..", [row("rating", { minimum: 3 })])
        expectEdit("rating:3..")
        addCategory(6)
        expectEdit("rating:4..")
        findChild(panel, "focusRating").picked("4")
        compare(QueryChips.lastEdit, { kind: "minimum", field: "rating", stars: 4 })
        expectEdit("rating:4.. genre:none")
        addCategory(8)
        compare(QueryChips.lastEdit, { kind: "missing", field: "genre", on: true })
        const join = findChild(panel, "focusJoin")
        compare(join.visible, false)
        mouseClick(findChild(panel, "focusAdvanced"))
        compare(join.visible, true)
        waitForRendering(panel)
        QueryChips.joinedText = "rating:4.. | genre:none"
        mouseClick(join, join.width * 3 / 4, join.height / 2)
        compare(QueryChips.lastJoin, ["rating:4.. genre:none", true])
        compare(Library.queryText, "rating:4.. | genre:none")
        QueryChips.joinedText = "rating:4.. genre:none"
        mouseClick(join, join.width / 4, join.height / 2)
        compare(QueryChips.lastJoin[1], false)
    }

    function test_drag_commits_only_on_release() {
        addCategory(1)
        const slider = findChild(panel, "focusYearRange")
        waitForRendering(panel)
        expectEdit("year:1960..1980")
        const handle = slider.first.handle
        const start = handle.mapToItem(slider, handle.width / 2, handle.height / 2)
        mousePress(slider, start.x, start.y)
        mouseMove(slider, slider.width / 3, start.y)
        compare(QueryChips.lastEdit, ({}), "dragging keeps the control alive")
        const from = Math.round(slider.first.value)
        mouseRelease(slider, slider.width / 3, start.y)
        compare(QueryChips.lastEdit, { kind: "range", field: "year", from: from, to: 1980 })
    }

    function test_a_custom_span_from_filter_keeps_its_label() {
        parsed("added:2w", [row("added", { within: 14 })])
        Library.queryText = "added:2w"
        const added = findChild(panel, "focusAdded")
        compare(added.options[added.current], "Last 14 days")
    }

    function test_advanced_query_stays_as_text() {
        const text = "(genre:Jazz | artist:Monk) year:1960.."
        parsed(text, [], false, true)
        Library.queryText = text
        compare(findChild(panel, "focusAdvancedQuery").text, text)
        compare(findChild(panel, "focusAdd").enabled, false)
        compare(findChild(panel, "focusPicker").item, null)
    }

    function test_library_screen_builds_both_surfaces() {
        panel.destroy()
        SettingsModel.keep("ui.library.focus", "true")
        SettingsModel.panesOpen = true
        const screen = createTemporaryObject(screenFactory, test)
        verify(screen)
        tryVerify(() => findChild(screen, "focusPanel") !== null)
        const control = findChild(screen, "focusSwitch")
        mouseClick(control, control.width / 4, control.height / 2)
        tryVerify(() => findChild(screen, "focusPanel") === null)
    }
}
