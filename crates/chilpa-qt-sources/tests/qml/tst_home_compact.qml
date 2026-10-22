import QtQuick
import QtTest
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Patterns"

// The Home's compact grid, as QBZ's: four across and three down, two pages
// at most; a track row plays from the list, a record row opens the record.
TestCase {
    id: test
    name: "HomeCompact"
    when: windowShown
    visible: true
    width: 1100
    height: 800

    TrackListModel { id: most; kind: "mostPlayed" }
    AlbumGridModel { id: records; kind: "recentlyAdded" }
    HomeTrackGrid { id: tracks; width: 1000; title: "Most played"; showPlays: true; model: most }
    HomeAlbumGrid { id: albums; y: 400; width: 1000; title: "Recently played"; model: records }
    SectionsCustomizer {
        id: customizer
        grouped: true
        sections: [{id: "library.mostPlayed", title: "Most played", compact: true}, {id: "library.recentPlaylists", title: "Playlists"}]
    }
    SignalSpy { id: laid; target: customizer; signalName: "laidOut" }

    function rowsOf(item) {
        let found = []
        const walk = node => { if (node.hasOwnProperty("subtitle") && node.hasOwnProperty("count")) found.push(node); for (const child of node.children || []) walk(child) }
        walk(item)
        return found.filter(row => row.visible)
    }

    function init() {
        most.clear()
        for (let i = 0; i < 30; i++)
            most.append({ trackId: i + 1, rowKind: 0, title: "Track " + i, artist: "Band", plays: 30 - i, available: true, playing: false, art: "" })
    }

    function test_four_across_three_down_and_two_pages_at_most() {
        tryVerify(() => rowsOf(tracks).length === 24, 1000, "two pages of twelve")
        compare(tracks.pageCount, 2)
        const rows = rowsOf(tracks)
        const first = rows.find(row => row.title === "Track 0").parent
        const fifth = rows.find(row => row.title === "Track 4").parent
        const thirteenth = rows.find(row => row.title === "Track 12").parent
        compare(first.height, Metrics.slimRow)
        compare(fifth.y, Metrics.slimRow + Metrics.gap8, "the fifth starts the second row")
        compare(thirteenth.x, findChild(tracks, "slimGridPages").width, "the thirteenth starts the second page")
    }

    function test_most_played_says_its_plays_and_a_click_plays_from_the_list() {
        tryVerify(() => rowsOf(tracks).length === 24)
        const row = rowsOf(tracks).find(row => row.title === "Track 2")
        compare(row.count, "28")
        verify(findChild(row, "slimRowCount").visible)
        row.activated()
        compare(most.lastAction, "play")
        compare(most.lastRow, 2)
    }

    function test_a_record_row_opens_the_record_and_says_no_count() {
        tryVerify(() => rowsOf(albums).length === 2)
        const row = rowsOf(albums)[0]
        verify(!findChild(row, "slimRowCount").visible)
        row.activated()
        compare(Navigation.page, "album")
    }

    function test_only_a_section_that_can_be_compact_offers_the_choice() {
        customizer.open()
        tryVerify(() => customizer.opened)
        tryVerify(() => findChild(customizer.contentItem, "activeMode_library.mostPlayed") !== null)
        const mode = findChild(customizer.contentItem, "activeMode_library.mostPlayed")
        verify(mode.visible)
        compare(mode.value, "compact", "compact until Cards is chosen")
        tryVerify(() => findChild(customizer.contentItem, "activeMode_library.recentPlaylists") !== null)
        verify(!findChild(customizer.contentItem, "activeMode_library.recentPlaylists").visible)
        mode.picked("cards")
        compare(laid.count, 1)
        compare(laid.signalArguments[0][1], "cards")
        customizer.close()
    }
}
