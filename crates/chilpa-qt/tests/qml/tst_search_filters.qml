import QtQuick
import QtTest
import app.chilpa
import "../../qml/Patterns"
import "../../qml/Components"

TestCase {
    name: "SearchFilters"
    when: windowShown
    visible: true
    width: 900
    height: 700

    SearchModel { id: model }
    SearchFlyout { id: flyout; search: model; room: 600 }
    SearchHit { id: hit; y: 600; width: 600; height: 48; index: 1; kind: 4; title: "Lion"; detail: "May'n"; art: ""; duration: "4:58"; sourceName: "A source"; sourceIcon: "" }

    function test_explicit_results_are_hidden_from_their_chip_and_shown_again() {
        flyout.open()
        tryCompare(flyout, "opened", true)
        const chip = findChild(flyout.contentItem, "hideExplicit")
        verify(chip !== null)
        compare(model.hideExplicit, false, "nothing is hidden by default")
        mouseClick(chip)
        compare(model.hideExplicit, true)
        verify(chip.active)
        mouseClick(chip)
        compare(model.hideExplicit, false)
        flyout.close()
    }

    function test_a_result_ends_with_its_time_its_source_and_its_menu() {
        const place = (item) => item.mapToItem(hit, 0, 0).x
        const texts = []
        const walk = (item) => { for (const child of item.children) { if (child.text === "4:58") texts.push(child); walk(child) } }
        walk(hit)
        const source = findChild(hit, "sourceImage").parent
        const menu = findChild(hit, "hitMenu")
        verify(texts.length === 1 && source !== null && menu !== null)
        verify(place(texts[0]) < place(source), "the time before the source")
        verify(place(source) < place(menu), "the source before the menu, which ends the row")
    }
}
