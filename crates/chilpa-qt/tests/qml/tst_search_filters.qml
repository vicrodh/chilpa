import QtQuick
import QtTest
import app.chilpa
import "../../qml/Patterns"

TestCase {
    name: "SearchFilters"
    when: windowShown
    visible: true
    width: 900
    height: 700

    SearchModel { id: model }
    SearchFlyout { id: flyout; search: model; room: 600 }

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
}
