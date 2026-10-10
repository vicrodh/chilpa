import QtQuick
import QtTest
import "../../qml/Theme"
import "../../qml/Components"

// A sort in use carries a caret for its direction, and choosing it again
// turns it around; an order without a direction, or a choice that is not
// an order, keeps its check.
TestCase {
    name: "SortButton"
    when: windowShown
    visible: true
    width: 400
    height: 400

    SortButton {
        id: sort
        options: [{ value: "artist", text: "Artist" }, { value: "added", text: "Date added", descending: true }, { value: "default", text: "Default", fixed: true }]
        onPicked: (next) => value = next
    }
    SortButton {
        id: grouping
        directed: false
        value: "letter"
        options: [{ value: "", text: "No grouping" }, { value: "letter", text: "By letter" }]
        onPicked: (next) => value = next
    }

    function test_choosing_the_order_in_use_again_turns_it_around() {
        sort.value = "artist"
        const menu = findChild(sort, "sortMenu")
        compare(menu.entries[0].glyph, Icons.chevronUp, "its own way, up")
        menu.chosen(0)
        compare(sort.value, "artist-desc")
        compare(menu.entries[0].glyph, Icons.chevronDown)
        menu.chosen(0)
        compare(sort.value, "artist")
        menu.chosen(1)
        compare(sort.value, "added", "another order comes its own way")
        verify(sort.descending, "the newest first is its own way")
        menu.chosen(1)
        compare(sort.value, "added-asc")
        verify(!sort.descending)
        menu.chosen(2)
        menu.chosen(2)
        compare(sort.value, "default")
        verify(menu.entries[2].checked, "an order with no direction keeps its check")
    }

    function test_choices_that_are_not_orders_keep_their_check() {
        const menu = findChild(grouping, "sortMenu")
        verify(menu.entries[1].checked)
        menu.chosen(1)
        compare(grouping.value, "letter")
    }
}
