import QtQuick
import QtTest
import app.chilpa
import app.chilpa.controls
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

    // Results of every source mix under the box: the library's own wear
    // Chilpa's mark, a source's its icon.
    function test_in_the_flyout_the_library_s_own_wear_chilpa_s_mark() {
        model.clear()
        model.append({kind: 4, title: "Own song", detail: "", art: "", duration: "3:00", sourceIcon: "", sourceName: ""})
        model.append({kind: 4, title: "Their song", detail: "", art: "", duration: "3:00", sourceIcon: "broadcast", sourceName: "Fixture"})
        flyout.open()
        tryCompare(flyout, "opened", true)
        const marks = () => { const out = []; const walk = node => { if (node.objectName === "hitOrigin" && node.visible) out.push(node); for (const child of node.children || []) walk(child) }; walk(flyout.contentItem); return out }
        tryVerify(() => marks().length === 2)
        const [own, theirs] = marks().sort((a, b) => a.mapToItem(null, 0, 0).y - b.mapToItem(null, 0, 0).y)
        verify(own.sourceInfo.chilpa === true)
        compare(theirs.sourceInfo.icon, "broadcast")
        tryCompare(findChild(own, "sourceImage"), "status", Image.Ready, 2000, "the mark is drawn")
        flyout.close()
        model.clear()
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
