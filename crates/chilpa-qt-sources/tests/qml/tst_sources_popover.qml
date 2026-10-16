import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import "../../qml/Patterns"

// The sources popover: two columns side by side, the column toggles only
// where the screen has columns, each toggle asking the screen to turn its
// column on or off.
TestCase {
    id: test
    name: "SourcesPopover"
    when: windowShown
    visible: true
    width: 900
    height: 700

    property var asked: []
    SourcesPopover { id: withPanes; panes: ["year", "genre"]; onPaneToggled: (column) => test.asked.push(column) }
    SourcesPopover { id: without }
    property var stars: []
    SourcesPopover { id: forTracks; ratings: test.stars; onRatingsChosen: (chosen) => test.stars = chosen }

    function init() { test.asked = [] }

    function test_a_toggle_asks_for_its_column() {
        withPanes.open()
        tryVerify(() => withPanes.opened)
        const label = findChild(withPanes.contentItem, "paneToggle-label")
        verify(label !== null)
        verify(!label.checked, "label is off by default")
        verify(findChild(withPanes.contentItem, "paneToggle-year").checked, "year is on")
        mouseClick(label)
        compare(test.asked.join(","), "label")
        withPanes.close()
    }
    function test_artist_and_album_are_available_in_the_compact_selector() {
        withPanes.open()
        tryVerify(() => withPanes.opened)
        for (const name of ["artist", "album"]) {
            const toggle = findChild(withPanes.contentItem, "paneToggle-" + name)
            verify(toggle !== null && toggle.compact)
            mouseClick(toggle)
        }
        compare(test.asked.join(","), "artist,album")
        withPanes.close()
    }

    function test_no_toggles_where_the_screen_has_no_columns() {
        without.open()
        tryVerify(() => without.opened)
        const label = findChild(without.contentItem, "paneToggle-label")
        verify(label === null || !label.visible || !label.parent.parent.visible, "the toggles stay out of sight")
        without.close()
    }

    // The Tracks screen keeps tracks by their own stars: a toggle a row,
    // five stars down to none, and Reset lets them all go.
    function test_the_tracks_screen_chooses_the_stars_it_keeps() {
        verify(findChild(withPanes.contentItem, "ratingToggle-5") === null || !findChild(withPanes.contentItem, "ratingToggle-5").parent.parent.visible, "only where the screen asks")
        forTracks.open()
        tryVerify(() => forTracks.opened)
        mouseClick(findChild(forTracks.contentItem, "ratingToggle-5"))
        mouseClick(findChild(forTracks.contentItem, "ratingToggle-0"))
        compare(test.stars, [5, 0])
        verify(findChild(forTracks.contentItem, "ratingToggle-5").checked)
        mouseClick(findChild(forTracks.contentItem, "ratingToggle-5"))
        compare(test.stars, [0])
        mouseClick(findChild(forTracks.contentItem, "ratingsReset"))
        compare(test.stars, [])
        forTracks.close()
    }

    function test_it_grows_across_not_down() {
        withPanes.open()
        tryVerify(() => withPanes.opened)
        verify(withPanes.width >= 600, "wide: " + withPanes.width)
        withPanes.close()
    }
}
