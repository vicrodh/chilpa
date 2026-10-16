import QtQuick
import QtTest
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// The date picker: a month of days that can be seen and picked, none after
// the latest, months to page through, and Today.
TestCase {
    id: test
    name: "DatePicker"
    when: windowShown
    visible: true
    width: 600
    height: 600

    Item {
        id: anchor
        x: 20
        y: 20
        width: 10
        height: 10

        DatePicker {
            id: picker
            selected: "2026-10-04"
            latest: "2026-10-09"
        }
    }
    SignalSpy { id: picked; target: picker; signalName: "picked" }

    function cells() {
        const grid = findChild(picker.contentItem, "pickerDays")
        return grid.contentItem.children.filter(cell => cell.day !== undefined)
    }

    // Opened, with its days placed: the grid lays them out on a polish.
    function show() {
        picker.open()
        tryCompare(picker, "opened", true)
        tryVerify(() => !isPolishScheduled(findChild(picker.contentItem, "pickerDays").contentItem))
    }

    function init() {
        picked.clear()
        show()
    }
    function cleanup() {
        picker.close()
    }

    function test_the_month_of_the_chosen_day_shows_its_days() {
        compare(picker.month, 9, "October")
        const days = cells()
        verify(days.length >= 35, "a month in whole weeks: " + days.length)
        const fourth = days.find(cell => cell.day === "2026-10-04")
        verify(fourth.width > 0 && fourth.height > 0, "a day is something to click: " + fourth.width + "x" + fourth.height)
        verify(fourth.chosen)
    }

    function test_a_day_is_picked_and_none_after_the_latest() {
        const days = cells()
        mouseClick(days.find(cell => cell.day === "2026-10-07"))
        compare(picked.count, 1)
        compare(picked.signalArguments[0][0], "2026-10-07")
        show()
        mouseClick(cells().find(cell => cell.day === "2026-10-20"))
        compare(picked.count, 1, "a day after today cannot be picked")
    }

    function test_escape_closes_it() {
        keyClick(Qt.Key_Escape)
        tryCompare(picker, "opened", false)
        compare(picked.count, 0)
    }

    function test_the_months_page_and_today_is_the_latest() {
        mouseClick(findChild(picker.contentItem, "pickerEarlier"))
        compare(picker.month, 8, "September")
        verify(findChild(picker.contentItem, "pickerLater").enabled)
        mouseClick(findChild(picker.contentItem, "pickerLater"))
        verify(!findChild(picker.contentItem, "pickerLater").enabled, "no month after today's")
        mouseClick(findChild(picker.contentItem, "pickerToday"))
        compare(picked.signalArguments[0][0], "2026-10-09")
    }
}
