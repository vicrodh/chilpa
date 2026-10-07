import QtQuick
import QtTest
import app.chilpa
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
}
