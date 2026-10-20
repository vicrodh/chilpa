import QtQuick
import QtTest
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// The gear beside a list of records opens one switch per column, in the
// order they stand in the row, and says what is chosen.
TestCase {
    id: test
    name: "AlbumColumns"
    when: windowShown
    visible: true
    width: 600; height: 500

    property var chosen: []
    AlbumColumnsGear { id: gear; shown: ["year", "tracks", "duration", "rating", "source"]; onChosen: (columns) => test.chosen = columns }

    function test_a_switch_turns_its_column_on_and_off_in_the_rows_order() {
        mouseClick(gear)
        const modal = findChild(gear, "albumColumns")
        tryVerify(() => modal.opened)
        waitForRendering(modal.contentItem)
        const genre = findChild(modal.contentItem, "column-genre")
        compare(genre.checked, false, "genre starts off")
        mouseClick(genre)
        compare(chosen, ["year", "tracks", "duration", "genre", "rating", "source"], "a column turned on takes its place in the row")
        gear.shown = chosen
        mouseClick(findChild(modal.contentItem, "column-tracks"))
        compare(chosen, ["year", "duration", "genre", "rating", "source"])
        modal.close()
    }
}
