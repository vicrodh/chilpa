import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import "../../qml/Components"
import "../../../chilpa-qt-kit/qml/Theme"

TestCase {
    name: "TrackListPlace"
    when: windowShown
    visible: true
    width: 1000
    height: 400
    TrackListModel { id: tracks }
    TrackList { id: list; width: 1000; height: 300; model: tracks; rich: false }
    readonly property Item view: findChild(list, "trackListView")

    function fill(count) {
        tracks.clear()
        for (let i = 1; i <= count; ++i)
            tracks.append({ trackId: i, rowKind: 0, number: i, title: "Track " + i, artist: "Band",
                album: "Album", sourceName: "", duration: "3:00", plays: 0, quality: "FLAC", tier: 2,
                available: true, playing: false, selected: false, headerLabel: "", headerMeta: "", art: "" })
        view.forceLayout()
    }
    function init() {
        Prefs.animatedIndicator = false
        tracks.kind = "listenlist"; tracks.albumId = 1; tracks.key = ""
        fill(80)
        view.contentY = view.originY
    }
    // What a reset does to the view: the rows are read again from the top.
    function readAgain(count) {
        list.keepPlace()
        fill(count)
        view.contentY = view.originY
        list.restorePlace()
    }

    function test_the_same_list_read_again_stays_where_it_was() {
        view.positionViewAtIndex(50, ListView.Beginning)
        view.contentY += 7
        const at = view.contentY - view.originY
        verify(at > 0)
        readAgain(80)
        compare(view.contentY - view.originY, at, "the same row at the top, as far into it")
    }

    function test_the_end_can_be_reached_and_kept() {
        view.positionViewAtEnd()
        const at = view.contentY - view.originY
        readAgain(80)
        compare(view.contentY - view.originY, at)
    }

    function test_a_shorter_list_ends_at_its_end() {
        view.positionViewAtEnd()
        readAgain(20)
        verify(view.atYEnd, "what is left is shown from its end")
        verify(view.contentY >= view.originY)
    }

    function test_another_list_starts_at_its_top() {
        view.positionViewAtIndex(50, ListView.Beginning)
        list.keepPlace()
        tracks.albumId = 2
        fill(80)
        view.contentY = view.originY
        list.restorePlace()
        compare(view.contentY, view.originY)
        view.positionViewAtIndex(50, ListView.Beginning)
        list.keepPlace()
        tracks.key = "maiden"
        fill(80)
        view.contentY = view.originY
        list.restorePlace()
        compare(view.contentY, view.originY, "a search narrows to another list")
    }

    // The playing row out of sight lets the pulse go: a pooled row and a
    // view's current one stay alive off the screen (QBZ's recycleActive).
    function test_a_playing_row_out_of_sight_lets_the_pulse_go_data() {
        return [{ tag: "a row in the pool", row: 5 }, { tag: "the first row", row: 0 }]
    }
    function test_a_playing_row_out_of_sight_lets_the_pulse_go(data) {
        Prefs.animatedIndicator = true
        tracks.setProperty(data.row, "playing", true)
        tryCompare(Pulse, "held", 1, 1000, "on the screen it holds the pulse")
        view.positionViewAtEnd()
        tryCompare(Pulse, "held", 0, 1000, "off the screen it lets the pulse go")
        view.positionViewAtBeginning()
        tryCompare(Pulse, "held", 1, 1000, "back on the screen it moves again")
    }

    function test_a_clipped_cached_row_stops_moving_data() {
        return [{ tag: "above", above: true }, { tag: "below", above: false }]
    }
    function test_a_clipped_cached_row_stops_moving(data) {
        const index = 30
        Prefs.animatedIndicator = true
        tracks.setProperty(index, "playing", true)
        view.positionViewAtIndex(index, ListView.Center)
        tryCompare(Pulse, "held", 1)
        const row = view.itemAtIndex(index)
        verify(row !== null)
        view.contentY = data.above ? row.y + row.height + 1 : row.y - view.height - 1
        verify(view.itemAtIndex(index) === row && row.visible, "the cached row is clipped, not pooled or hidden")
        tryCompare(Pulse, "held", 0, 1000, "a clipped cached mark must not schedule frames")
        view.contentY += data.above ? -2 : 2
        tryCompare(Pulse, "held", 1, 1000, "even a partially visible mark resumes")
    }
}
