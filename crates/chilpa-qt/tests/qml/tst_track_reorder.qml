import QtQuick
import QtTest
import com.blitzfc.chilpa
import "../../qml/Components"
import "../../qml/Theme"

TestCase {
    name: "TrackReorder"
    when: windowShown
    visible: true
    width: 1000
    height: 400
    TrackListModel { id: tracks }
    TrackList { id: list; width: 1000; height: 250; model: tracks; rich: false; reorderable: true }
    TrackList { id: plain; y: 260; width: 1000; height: 120; model: tracks; rich: false }
    function init() {
        list.rich = false
        tracks.clear()
        for (let i = 1; i <= 3; ++i)
            tracks.append({ trackId: 40 + i, rowKind: 0, number: i, title: "Track " + i, artist: "Band",
                album: "Album", duration: "3:00", plays: 0, quality: "FLAC", tier: 2,
                available: true, playing: false, selected: false, headerLabel: "", headerMeta: "", art: "" })
        tracks.lastAction = ""; tracks.lastRow = -1; tracks.steps = 0
        wait(30)
    }
    function row(of, at) { return findChild(of, "trackListView").itemAtIndex(at) }
    // The middle of an arrow's cell of a row, in the list's coordinates.
    function arrow(at, which) {
        const item = row(list, at)
        return item.mapToItem(list, Metrics.rowPadCompact + Metrics.rowAction * (which + 0.5), item.height / 2)
    }
    function test_arrows_take_the_number_place_and_move_the_row() {
        const up = arrow(1, 0), down = arrow(1, 1)
        mouseMove(list, up.x, up.y)
        mouseClick(list, up.x, up.y)
        compare(tracks.lastAction, "up"); compare(tracks.lastRow, 1)
        mouseClick(list, down.x, down.y)
        compare(tracks.lastAction, "down"); compare(tracks.lastRow, 1)
        compare(tracks.steps, 2)
    }
    function test_the_arrows_stay_clear_of_the_time_and_cost_the_album_column() {
        const moved = row(list, 0).cols, still = row(plain, 0).cols
        verify(moved.reorder && !still.reorder)
        verify(moved.index >= Metrics.colMove, "both arrows fit where the number was")
        compare(moved.titleW, still.titleW)
        compare(moved.timeX, still.timeX)
        compare(still.albumW - moved.albumW, moved.index - still.index)
    }
    function test_a_quick_second_press_is_one_more_step_and_never_a_play() {
        const down = arrow(0, 1)
        mouseDoubleClickSequence(list, down.x, down.y)
        compare(tracks.steps, 2)
        verify(tracks.lastAction !== "play")
    }
    function test_a_list_in_another_order_has_no_arrows() {
        const item = row(plain, 1)
        const at = item.mapToItem(plain, Metrics.rowPadCompact + Metrics.rowAction / 2, item.height / 2)
        mouseClick(plain, at.x, at.y)
        compare(tracks.steps, 0)
        compare(tracks.lastAction, "select")
    }
}
