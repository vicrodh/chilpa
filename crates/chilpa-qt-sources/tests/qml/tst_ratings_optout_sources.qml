import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import "../../qml/Patterns"

// With ratings off the filters of the Library and Tracks screens offer no
// stars: no rating column to turn on, no stars to keep, and the filter
// button does not say a hidden choice is in force. What was chosen stays
// chosen. Removing a server says nothing of stars, with ratings on or off.
TestCase {
    id: test
    name: "RatingsOptOutSources"
    when: windowShown
    visible: true
    width: 900
    height: 700

    property var asked: []
    property var stars: [5]
    SourcesPopover { id: withPanes; panes: ["year", "rating"]; onPaneToggled: (column) => test.asked.push(column) }
    SourcesPopover { id: forTracks; ratings: test.stars; onRatingsChosen: (chosen) => test.stars = chosen }
    ServerRemoveDialog { id: removal; message: "{\"name\":\"Home\",\"tracks\":3,\"albums\":1,\"ratings\":5,\"plays\":2}" }

    function init() { test.asked = []; test.stars = [5] }
    function cleanup() { Prefs.ratings = true; withPanes.close(); forTracks.close() }
    function shown(item) {
        for (let at = item; at; at = at.parent) if (!at.visible) return false
        return true
    }

    function test_no_rating_column_is_offered_and_a_chosen_one_stays() {
        withPanes.open()
        tryVerify(() => withPanes.opened)
        const rating = findChild(withPanes.contentItem, "paneToggle-rating")
        verify(shown(rating) && rating.checked)
        Prefs.ratings = false
        verify(!shown(rating), "no rating column to turn on")
        tryVerify(() => { const year = findChild(withPanes.contentItem, "paneToggle-year"); return shown(year) && year.parent.y > 0 }, 1000, "the other columns stay, placed again once their rows are rebuilt")
        mouseClick(findChild(withPanes.contentItem, "paneToggle-year"))
        compare(test.asked, ["year"], "only the column asked for changes")
        Prefs.ratings = true
        tryVerify(() => { const again = findChild(withPanes.contentItem, "paneToggle-rating"); return shown(again) && again.checked }, 1000, "the rating column is offered again, still chosen")
    }

    function test_the_tracks_screen_offers_no_stars_and_keeps_the_ones_chosen() {
        forTracks.open()
        tryVerify(() => forTracks.opened)
        const five = findChild(forTracks.contentItem, "ratingToggle-5")
        verify(shown(five) && forTracks.starred)
        Prefs.ratings = false
        verify(!shown(five), "no stars to keep")
        verify(!forTracks.starred, "a hidden choice does not mark the filter")
        mouseClick(findChild(forTracks.contentItem, "ratingsReset"))
        compare(test.stars, [5], "Reset leaves the hidden choice as it was")
        Prefs.ratings = true
        verify(shown(five) && forTracks.starred)
    }

    // A facts document that still counts stars is not said either.
    function test_removing_a_server_says_nothing_of_stars_with_ratings_on_or_off() {
        for (const on of [true, false]) {
            Prefs.ratings = on
            verify(removal.said.indexOf("rating") < 0 && removal.said.indexOf("5") < 0, removal.said)
            verify(removal.said.indexOf("Your 2 plays of them are removed too.") >= 0, removal.said)
        }
    }
}
