import QtQuick
import QtQuick.Controls.Basic
import QtTest
import app.chilpa
import "../../qml/Components"

TestCase {
    id: test
    name: "NerdyAlbums"
    when: windowShown
    visible: true
    width: 1200; height: 650
    AlbumGridModel { id: records }
    AlbumGrid { id: grid; width: 1050; height: 400; model: records; list: true; technical: true }
    SignalSpy { id: opened; target: grid; signalName: "open" }
    TrackListModel { id: toolbarTracks }
    LibraryToolbar { id: toolbar; y: 420; width: 1050; tracks: toolbarTracks }
    function initTestCase() {
        AlbumActions.technicalOf = {1: {tracks: 12, codec: "FLAC", rate: 96000, bits: 24, channels: 2, bytes: 104857600, artwork: "/cover.jpg"},
            2: {tracks: 8, codec: null, rate: null, bits: null, year: null, mixed_fields: ["codec", "rate"]}}
        for (let at = 0; at < 1000; ++at) records.append({first: at, span: 1, group: "", revision: 0})
        records.albums = 1000
    }
    function init() {
        grid.width = 1050
        grid.technical = true
        grid.scrollView.contentX = 0
        grid.scrollView.positionViewAtBeginning()
        mouseMove(test, 1150, 600)
        tryVerify(() => grid.scrollView.itemAtIndex(0) !== null)
    }
    function test_clicking_the_album_row_opens_its_album_in_both_list_modes() {
        function albumRow(item) {
            if (!item) return null
            if (typeof item.technicalFacts !== "undefined" && item.albumId === 1) return item
            for (const child of item.children || []) {
                const found = albumRow(child)
                if (found) return found
            }
            return null
        }
        AlbumActions.selectedCount = 0
        for (const technical of [true, false]) {
            grid.technical = technical
            let row = null
            tryVerify(() => { row = albumRow(grid.scrollView.itemAtIndex(0)); return row !== null })
            compare(row.technical, technical)
            opened.clear()
            row.clicked(0)
            tryCompare(opened, "count", 1)
            compare(opened.signalArguments[0][0], 1)
            compare(opened.signalArguments[0][1], "Record 0")
        }
    }
    function test_switcher_keeps_the_requested_glasses_icon() {
        const switcher = findChild(toolbar, "libraryViewSwitcher")
        verify(switcher.options[2].glyph !== "")
        switcher.activated(2)
        compare(SettingsModel.libraryView, "nerdy")
    }
    function test_rows_are_albums_and_do_not_create_cover_images() {
        const row = grid.scrollView.itemAtIndex(0)
        tryVerify(() => findChild(row, "technicalCell_tracks") !== null)
        compare(findChild(row, "technicalCell_album").text, "Record 0")
        compare(findChild(row, "technicalCell_tracks").text, "12")
        compare(findChild(row, "technicalCell_artwork").text, "Yes")
        function images(item) {
            if (String(item).indexOf("QQuickImage") >= 0) return 1
            return item.children ? item.children.reduce((sum, child) => sum + images(child), 0) : 0
        }
        compare(images(row), 0)
    }
    function test_mixed_and_unknown_album_values_are_explicit() {
        const row = grid.scrollView.itemAtIndex(1)
        tryVerify(() => row && findChild(row, "technicalCell_codec") !== null)
        compare(findChild(row, "technicalCell_codec").text, "Mixed")
        compare(findChild(row, "technicalCell_rate").text, "Mixed")
        compare(findChild(row, "technicalCell_bits").text, "—")
        compare(findChild(row, "technicalCell_year").text, "—")
    }
    function assertPaintedThumb(bar) {
        tryVerify(() => bar.contentItem.width > 0 && bar.contentItem.height > 0)
        verify(bar.contentItem.opacity > 0)
        waitForRendering(bar)
        const thumb = bar.contentItem
        const center = thumb.mapToItem(bar, thumb.width / 2, thumb.height / 2)
        const trackX = bar.sideways ? (thumb.x + thumb.width + bar.width) / 2 : bar.width / 2
        const trackY = bar.sideways ? bar.height / 2 : (thumb.y + thumb.height + bar.height) / 2
        const picture = grabImage(bar)
        const painted = picture.pixel(Math.floor(center.x), Math.floor(center.y))
        verify(painted.a > 0)
        verify(!Qt.colorEqual(painted, picture.pixel(Math.floor(trackX), Math.floor(trackY))),
            "the thumb paints pixels distinct from its track")
    }
    function test_scrollbars_stay_in_free_gutters_and_horizontal_is_visible() {
        const vertical = findChild(grid, "albumVerticalScrollBar"), horizontal = findChild(grid, "albumHorizontalScrollBar")
        tryCompare(vertical, "parent", grid)
        tryCompare(horizontal, "parent", grid)
        tryCompare(horizontal, "visible", true)
        const view = grid.scrollView
        verify(vertical.x >= view.x + view.width, "the vertical thumb is outside clipped rows")
        verify(vertical.x + vertical.width <= grid.width)
        verify(horizontal.y >= view.y + view.height, "the horizontal thumb has its own bottom strip")
        verify(horizontal.y + horizontal.height <= grid.height)
        mouseMove(view, 300, 100)
        tryCompare(vertical, "visible", true)
        assertPaintedThumb(vertical)
        assertPaintedThumb(horizontal)
        view.positionViewAtIndex(999, ListView.End)
        tryVerify(() => view.contentY > 0)
        verify(vertical.x >= view.x + view.width)
        view.contentX = view.contentWidth - view.width
        tryVerify(() => view.contentX > 0)
        compare(horizontal.visible, true)
        verify(horizontal.position > 0)
        verify(vertical.x >= view.x + view.width)
    }
    function test_horizontal_thumb_drag_scrolls_album_columns() {
        const horizontal = findChild(grid, "albumHorizontalScrollBar")
        const view = grid.scrollView
        tryCompare(horizontal, "visible", true)
        tryCompare(view, "contentX", 0)
        const thumbCenter = horizontal.contentItem.x + horizontal.contentItem.width / 2
        const y = horizontal.height / 2
        mousePress(horizontal, thumbCenter, y, Qt.LeftButton)
        tryCompare(horizontal, "pressed", true)
        mouseMove(horizontal, thumbCenter + 120, y, 50)
        tryVerify(() => view.contentX > 0)
        compare(horizontal.visible, true)
        mouseRelease(horizontal, thumbCenter + 120, y, Qt.LeftButton)
        tryCompare(horizontal, "pressed", false)
        verify(view.contentX > 0)
        compare(horizontal.visible, true)
    }
    function test_leaving_nerdy_resets_horizontal_scroll_without_losing_albums() {
        const view = grid.scrollView
        const horizontal = findChild(grid, "albumHorizontalScrollBar")
        const albums = records.albums
        const rows = view.count
        view.contentX = view.contentWidth - view.width
        tryVerify(() => view.contentX > 0)
        grid.technical = false
        tryCompare(view, "contentX", 0)
        tryVerify(() => view.contentWidth <= view.width)
        tryVerify(() => !view.headerItem)
        compare(records.albums, albums)
        compare(view.count, rows)
        grid.technical = true
        tryVerify(() => view.headerItem !== null)
        tryCompare(horizontal, "visible", true)
        verify(horizontal.size > 0 && horizontal.size < 1)
        compare(view.contentX, 0)
        compare(records.albums, albums)
        compare(view.count, rows)
    }
    function test_no_horizontal_overflow_hides_the_bar_and_modes_restore() {
        grid.width = 1900
        const horizontal = findChild(grid, "albumHorizontalScrollBar")
        tryCompare(horizontal, "visible", false)
        compare(grid.scrollView.contentWidth, grid.scrollView.width)
        tryCompare(grid.scrollView, "contentX", 0)
        mouseDrag(grid.scrollView, 600, 120, -250, 0, Qt.LeftButton)
        tryCompare(grid.scrollView, "contentX", 0)
        compare(horizontal.visible, false)
        grid.technical = false
        tryCompare(horizontal, "visible", false)
        verify(!grid.scrollView.headerItem)
        grid.technical = true
        tryVerify(() => grid.scrollView.headerItem !== null)
    }
}
