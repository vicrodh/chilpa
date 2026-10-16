import QtQuick
import QtTest
import app.chilpa
import "../../qml/Components"
import "../../../chilpa-qt-kit/qml/Theme"

TestCase {
    name: "TrackWindow"
    when: windowShown
    visible: true
    width: 1000; height: 400
    TrackListModel { id: tracks }
    Flickable {
        id: page
        width: parent.width; height: parent.height
        contentHeight: column.height
        Column {
            id: column
            width: page.width
            Item { width: parent.width; height: Metrics.heroArt }
            TrackList { id: list; width: parent.width; height: implicitHeight; fitContent: true; viewport: page; model: tracks; rich: false }
        }
    }
    function init() {
        page.contentY = 0; list.rich = false
        tracks.clear()
        for (let i = 0; i < 247; i++)
            tracks.append({ trackId: i + 1, rowKind: i === 80 ? 1 : i === 160 ? 2 : 0, number: i + 1,
                title: "Track " + i, artist: "Band", album: "Record", sourceName: "", sourceInfo: "{}", composer: "",
                duration: "3:00", plays: 0, quality: "FLAC", tier: 2, available: true, playing: false, selected: false,
                headerLabel: "Disc 2", headerMeta: "80 songs", art: "" })
        tracks.lastAction = ""; tracks.lastRow = -1
        wait(50)
    }
    function view() { return findChild(list, "trackListView") }
    function built() {
        let total = 0
        for (let i = 0; i < tracks.count; i++) {
            const row = view().itemAtIndex(i)
            if (row && findChild(row, "trackRowBody")) total++
        }
        return total
    }
    function contentBuilds() {
        let total = 0
        for (let i = 0; i < tracks.count; i++)
            total += view().itemAtIndex(i)?.contentBuilds || 0
        return total
    }
    function test_initial_layout_never_builds_visuals_for_every_slot() {
        tryVerify(() => contentBuilds() > 0)
        verify(contentBuilds() > 0 && contentBuilds() < 60, "initial placement must not create and discard every row's contents: " + contentBuilds())
    }
    function test_long_records_keep_exact_slots_and_bound_visual_rows() {
        for (const rich of [false, true]) {
            list.rich = rich
            wait(50)
            const rowHeight = rich ? Metrics.rowRich : Metrics.rowCompact
            compare(view().contentHeight, 245 * rowHeight + Metrics.rowGroup + Metrics.rowRecord)
            verify(built() > 0 && built() < 40, "247 tracks build only a nearby band of visual rows")
            verify(view().itemAtIndex(246), "the last slot retains its geometry")
            compare(findChild(view().itemAtIndex(246), "trackRowBody"), null)
        }
    }
    function test_scrolling_builds_the_new_band_and_releases_the_old_one() {
        const height = view().contentHeight
        page.contentY = column.height - page.height
        wait(50)
        verify(findChild(view().itemAtIndex(246), "trackRowBody"), "the last track is ready to draw")
        compare(findChild(view().itemAtIndex(0), "trackRowBody"), null)
        compare(view().contentHeight, height, "windowing cannot change scroll geometry")
        verify(built() < 40)
        const row = view().itemAtIndex(246)
        const at = row.mapToItem(page, row.cols.titleX + 2, row.height / 2)
        mouseClick(page, at.x, at.y)
        compare(tracks.lastAction, "select")
        compare(tracks.lastRow, 246)
        mouseDoubleClickSequence(page, at.x, at.y)
        compare(tracks.lastAction, "play")
    }
    function test_a_disc_keeps_its_actions_after_scrolling() {
        const row = view().itemAtIndex(80)
        page.contentY = list.y + row.y
        wait(50)
        const action = row.groupActionArea
        verify(action && action.visible)
        const at = action.mapToItem(page, Metrics.button30 / 2, Metrics.button30 / 2)
        mouseClick(page, at.x, at.y)
        compare(tracks.lastAction, "play")
        compare(tracks.lastRow, 80)
    }
}
