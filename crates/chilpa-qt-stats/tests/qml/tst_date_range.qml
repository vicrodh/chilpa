import QtQuick
import QtTest
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Components"

// A span of days: each end said as a date or as open, an end picked past
// the other taking it along, and the way back to every day.
TestCase {
    name: "DateRange"
    when: windowShown
    visible: true
    width: 600
    height: 400

    Component { id: spanOf; DateRange { latest: "2026-10-09" } }
    SignalSpy { id: chosen; signalName: "chosen" }

    function init() { chosen.clear() }

    function test_an_open_span_says_so_and_offers_no_way_back() {
        const range = createTemporaryObject(spanOf, this)
        compare(findChild(range, "rangeFrom").text, "First day")
        compare(findChild(range, "rangeTo").text, "Last day")
        verify(!findChild(range, "rangeClear").visible)
        compare(findChild(range, "rangeToPicker").latest, "2026-10-09")
    }

    function test_an_end_picked_past_the_other_takes_it_along() {
        const range = createTemporaryObject(spanOf, this, { from: "2026-10-01", to: "2026-10-05" })
        chosen.target = range
        findChild(range, "rangeFromPicker").picked("2026-10-07")
        compare(chosen.signalArguments[0][0], "2026-10-07")
        compare(chosen.signalArguments[0][1], "2026-10-07", "the last day moves with it")
        findChild(range, "rangeToPicker").picked("2026-09-20")
        compare(chosen.signalArguments[1][0], "2026-09-20", "the first day moves with it")
        compare(chosen.signalArguments[1][1], "2026-09-20")
        findChild(range, "rangeToPicker").picked("2026-10-08")
        compare(chosen.signalArguments[2][0], "2026-10-01", "an end in order keeps the other")
        compare(chosen.signalArguments[2][1], "2026-10-08")
    }

    function test_the_way_back_opens_both_ends() {
        const range = createTemporaryObject(spanOf, this, { from: "2026-10-01" })
        chosen.target = range
        verify(findChild(range, "rangeClear").visible)
        verify(findChild(range, "rangeFrom").text !== "First day", "the day itself")
        mouseClick(findChild(range, "rangeClear"))
        compare(chosen.count, 1)
        compare([chosen.signalArguments[0][0], chosen.signalArguments[0][1]], ["", ""])
    }
}
