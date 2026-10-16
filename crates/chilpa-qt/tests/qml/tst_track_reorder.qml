import QtQuick
import QtTest
import app.chilpa
import "../../qml/Components"
import "../../../chilpa-qt-kit/qml/Theme"

TestCase {
    name: "TrackReorder"
    when: windowShown
    visible: true
    width: 1000
    height: 400
    TrackListModel { id: tracks }
    TrackList { id: list; width: 1000; height: 250; model: tracks; rich: false; reorderable: true }
    TrackList { id: plain; y: 260; width: 1000; height: 120; model: tracks; rich: false }
    // A playlist's page: the list grows with its rows inside a page that
    // scrolls as a whole.
    Flickable {
        id: page
        x: 0; y: 0; z: -1
        width: 1000; height: 100
        visible: false
        contentHeight: paged.implicitHeight + 400
        TrackList { id: paged; width: 1000; height: implicitHeight; fitContent: true; model: tracks; rich: false; reorderable: true }
    }
    function init() {
        page.visible = false; page.contentY = 0
        list.visible = true; plain.visible = true
        list.rich = false
        tracks.clear()
        for (let i = 1; i <= 3; ++i)
            tracks.append({ trackId: 40 + i, rowKind: 0, number: i, title: "Track " + i, artist: "Band",
                album: "Album", sourceName: "Remote source", duration: "3:00", plays: 0, quality: "FLAC", tier: 2,
                available: true, playing: false, selected: false, headerLabel: "", headerMeta: "", art: "" })
        tracks.lastAction = ""; tracks.lastRow = -1; tracks.steps = 0; tracks.selectedCount = 0; tracks.movedTo = -1
        wait(30)
    }
    function row(of, at) { return findChild(of, "trackListView").itemAtIndex(at) }
    // The middle of an arrow of a row, in the list's coordinates: side by
    // side in a compact row, one over the other in a rich one.
    function arrow(at, which) {
        const item = row(list, at)
        return list.rich
            ? item.mapToItem(list, Metrics.rowPadRich + Metrics.colMoveRich / 2, item.height / 2 + (which - 0.5) * Metrics.moveCell)
            : item.mapToItem(list, Metrics.rowPadCompact + Metrics.moveCellCompact * (which + 0.5), item.height / 2)
    }
    function check_arrows() {
        const up = arrow(1, 0), down = arrow(1, 1)
        mouseMove(list, up.x, up.y)
        mouseClick(list, up.x, up.y)
        compare(tracks.lastAction, "up"); compare(tracks.lastRow, 1)
        mouseClick(list, down.x, down.y)
        compare(tracks.lastAction, "down"); compare(tracks.lastRow, 1)
        compare(tracks.steps, 2)
    }
    function test_album_name_opens_album_without_playing_or_selecting() {
        for (const rich of [false, true]) {
            list.rich = rich
            wait(30)
            const item = row(list, 0)
            compare(item.sourceName, "Remote source")
            verify(item.cols.sourceWidth > 0)
            const at = item.mapToItem(list, item.cols.albumX + 2, item.height / 2)
            mouseClick(list, at.x, at.y)
            compare(tracks.lastAction, "album")
            compare(tracks.lastRow, 0)
            mouseClick(list, at.x, at.y, Qt.LeftButton, Qt.ControlModifier)
            compare(tracks.lastAction, "select")
        }
    }
    function test_artist_name_opens_artist_without_playing_or_selecting() {
        for (const rich of [false, true]) {
            list.rich = rich
            wait(30)
            const item = row(list, 0)
            const label = item.artistLabel
            const at = label.mapToItem(list, 2, label.height / 2)
            mouseClick(list, at.x, at.y)
            compare(tracks.lastAction, "artist:0")
            compare(tracks.lastRow, 0)
        }
    }
    function test_compact_arrows_sit_side_by_side_before_the_number() { check_arrows() }
    function test_rich_arrows_are_stacked_before_the_number() {
        list.rich = true
        wait(30)
        check_arrows()
        const number = row(list, 1).cols
        // The number's own cell is not an arrow.
        const item = row(list, 2)
        const at = item.mapToItem(list, number.indexX + number.index / 2, item.height / 2)
        mouseClick(list, at.x, at.y)
        compare(tracks.lastAction, "select")
    }
    function test_the_number_stays_and_the_album_column_pays_for_the_arrows() {
        const moved = row(list, 0).cols, still = row(plain, 0).cols
        verify(moved.reorder && !still.reorder)
        compare(moved.move, Metrics.colMoveCompact)
        verify(moved.move < Metrics.rowAction * 2, "more compact than two full cells")
        compare(moved.index, still.index)
        compare(moved.titleW, still.titleW)
        compare(moved.timeX, still.timeX)
        compare(still.albumW - moved.albumW, moved.move)
    }
    function test_a_quick_second_press_is_one_more_step_and_never_a_play() {
        const down = arrow(0, 1)
        mouseDoubleClickSequence(list, down.x, down.y)
        compare(tracks.steps, 2)
        verify(tracks.lastAction !== "play")
    }
    function test_a_list_in_another_order_has_no_arrows() {
        const item = row(plain, 1)
        const at = item.mapToItem(plain, Metrics.rowPadCompact + Metrics.moveCellCompact / 2, item.height / 2)
        mouseClick(plain, at.x, at.y)
        compare(tracks.steps, 0)
        compare(tracks.lastAction, "select")
    }
    function drag(count) {
        tracks.selectedCount = count
        tracks.setProperty(0, "selected", count > 1)
        const ghost = findChild(list, "dragGhost")
        const from = row(list, 0), to = row(list, 2)
        const start = from.mapToItem(list, 300, from.height / 2), end = to.mapToItem(list, 300, to.height / 2)
        verify(!ghost.visible)
        mousePress(list, start.x, start.y)
        mouseMove(list, start.x, start.y + 12)
        mouseMove(list, end.x, end.y)
        verify(ghost.visible, "the ghost follows the drag")
        const spot = ghost.mapToItem(list, 0, 0)
        verify(Math.abs(spot.x - end.x) < 40 && Math.abs(spot.y - end.y) < 40, "next to the pointer")
        compare(ghost.count, count)
        const line = findChild(list, "dropLine")
        verify(line.visible, "a line marks where it would land")
        const under = to.mapToItem(list, 0, to.height)
        verify(Math.abs(line.y + line.height / 2 - under.y) < 1, "under the target, coming from above")
        compare(ghost.title, "Track 1")
        mouseRelease(list, end.x, end.y)
        verify(!ghost.visible)
        verify(!findChild(list, "dropLine").visible)
        compare(tracks.lastAction, "move"); compare(tracks.lastRow, 0); compare(tracks.movedTo, 2)
    }
    function test_dragging_a_row_does_not_scroll_the_page_it_is_in() {
        list.visible = false; plain.visible = false; page.visible = true
        wait(30)
        const from = row(paged, 0), to = row(paged, 2)
        const start = from.mapToItem(page, 300, from.height / 2), end = to.mapToItem(page, 300, to.height / 2)
        mousePress(page, start.x, start.y)
        for (let step = 1; step <= 8; ++step)
            mouseMove(page, start.x, start.y + (end.y - start.y) * step / 8)
        verify(findChild(paged, "dragGhost").visible, "the row is being dragged")
        compare(page.contentY, 0)
        verify(!page.moving)
        mouseRelease(page, end.x, end.y)
        compare(tracks.lastAction, "move"); compare(tracks.movedTo, 2)
    }
    function test_a_dragged_row_carries_its_ghost() { drag(1) }
    function test_a_dragged_selection_carries_a_group_ghost() { drag(3) }
}
