import QtQuick
import QtTest
import com.blitzfc.chilpa
import "../../qml/Screens"

TestCase {
    name: "SourceScreen"
    when: windowShown
    visible: true
    width: 900
    height: 500

    SourceScreen { id: screen; width: 640; height: 500 }

    readonly property var station: ({ id: "station:1", title: "One FM", subtitle: "Somewhere", image: "", kind: "item", live: true, playable: true, whyNot: "", saved: false, sounding: false, resolving: false })
    readonly property var folder: ({ id: "countries", title: "By country", subtitle: "", image: "", kind: "folder", live: false, playable: true, whyNot: "", saved: false, sounding: false, resolving: false })

    function init() {
        Sources.asked = []
        Sources.show({ crumbs: [], entries: [folder, station], error: "", loading: false, more: false })
        wait(30)
    }

    function last() { return Sources.asked[Sources.asked.length - 1] }
    function row(index) { return findChild(screen, "sourceList").itemAtIndex(index) }

    function test_opening_asks_for_the_source_of_the_route() {
        Sources.asked = []
        const fresh = createTemporaryObject(Qt.createComponent("../../qml/Screens/SourceScreen.qml"), this)
        compare(Sources.asked[0], ["open", "radio", ""])
    }

    function test_a_folder_is_entered_and_a_station_played_by_the_same_click() {
        mouseClick(row(0), 60, 10)
        compare(last(), ["enter", "countries", ""])
        mouseClick(row(1), 60, 10)
        compare(last(), ["enter", "station:1", ""])
    }

    function test_the_heart_saves_and_does_not_play() {
        mouseClick(row(1), row(1).width - 22, row(1).height / 2)
        compare(last(), ["save", "station:1", ""])
    }

    function test_a_crumb_goes_back_to_its_step() {
        Sources.show({ crumbs: ["By country", "Mexico"] })
        wait(30)
        const crumbs = Array.from(findChild(screen, "sourceCrumbs").children).filter(c => c.text !== undefined)
        compare(crumbs.length, 3)
        mouseClick(crumbs[1])
        compare(last(), ["crumb", "1", ""])
        mouseClick(crumbs[0])
        compare(last(), ["crumb", "0", ""])
    }

    function test_an_error_is_said_and_can_be_retried() {
        Sources.show({ entries: [], error: "the directory did not answer" })
        wait(30)
        compare(findChild(screen, "sourceStatus").text, "the directory did not answer")
    }
}
