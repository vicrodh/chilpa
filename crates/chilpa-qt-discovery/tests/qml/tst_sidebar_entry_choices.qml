import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import "../../qml/Components"

TestCase {
    name: "SidebarEntryChoices"
    when: windowShown
    width: 480
    height: 600
    visible: true
    property string sent: ""
    SidebarEntryChoices { id: choices; width: 400; onChanged: (value) => sent = value }

    function init() {
        sent = ""
        choices.value = "home,library,artists,albums,tracks,genres,composers,compositions,folders,openMedia,deviceSync"
    }

    function test_moving_an_entry_sends_the_whole_layout() {
        mouseClick(findChild(choices, "sidebarEntryDown-home"))
        compare(sent, "library,home,artists,albums,tracks,genres,composers,compositions,folders,openMedia,deviceSync")
    }

    function test_turning_one_off_sends_it_with_a_dash() {
        findChild(choices, "sidebarEntryToggle-composers").toggled(false)
        compare(sent, "home,library,artists,albums,tracks,genres,-composers,compositions,folders,openMedia,deviceSync")
    }

    function test_the_last_entry_shown_cannot_be_turned_off() {
        choices.value = "-home,-library,-artists,-albums,tracks,-genres,-composers,-compositions,-folders,-openMedia,-deviceSync"
        compare(findChild(choices, "sidebarEntryToggle-tracks").enabled, false)
        compare(findChild(choices, "sidebarEntryToggle-home").enabled, true)
    }

    function test_the_ends_cannot_move_past_the_edge() {
        compare(findChild(choices, "sidebarEntryUp-home").enabled, false)
        compare(findChild(choices, "sidebarEntryDown-deviceSync").enabled, false)
    }
}
