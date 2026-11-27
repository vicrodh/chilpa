import QtQuick
import QtTest
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// Play on a list left halfway asks whether to go on or start over; a way
// out that is neither plays nothing.
TestCase {
    name: "ResumeOffer"
    when: windowShown
    visible: true
    width: 960
    height: 700
    ResumeOffer { id: dialog }
    function init() { QueueActivity.answers = []; QueueActivity.offer = "" }
    function cleanup() { dialog.close(); tryCompare(dialog, "visible", false) }
    function offered() {
        QueueActivity.offer = JSON.stringify({ title: "Allegro", number: 3, total: 8 })
        tryCompare(dialog, "opened", true)
        verify(findChild(dialog.contentItem, "resumeOfferText").text.indexOf("Allegro") >= 0)
    }

    function test_continue_goes_on_from_the_place() {
        offered()
        mouseClick(findChild(dialog.contentItem, "resumeContinue"))
        compare(QueueActivity.answers, ["continue"])
        tryCompare(dialog, "visible", false)
    }

    function test_start_over_plays_from_the_first_track() {
        offered()
        mouseClick(findChild(dialog.contentItem, "resumeRestart"))
        compare(QueueActivity.answers, ["restart"])
    }

    function test_closing_it_plays_nothing() {
        offered()
        dialog.close()
        tryCompare(dialog, "visible", false)
        compare(QueueActivity.answers, [""])
    }
}
