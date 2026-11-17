import QtQuick
import QtTest
import app.chilpa
import "qrc:/qt/qml/app/chilpa/sources/qml/Screens"

TestCase {
    id: test
    name: "SourceListingViewport"
    when: windowShown
    visible: true
    width: 1000; height: 500
    Component { id: listing; SourceListing { width: test.width; height: test.height; source: "fixture native:folder" } }
    property var page
    property var grid
    function findItem(root, matches) {
        if (matches(root)) return root
        for (const child of root.children || []) {
            const found = findItem(child, matches)
            if (found) return found
        }
        return null
    }
    function init() {
        failOnWarning(/Binding loop detected|Cannot assign|is not a function|ReferenceError|TypeError/)
        page = createTemporaryObject(listing, test)
        grid = findItem(page, item => item.windowCount !== undefined)
        verify(grid)
        compare(grid.viewport, page)
        tryVerify(() => findChild(grid, "albumAnimatedCover") !== null)
    }
    function appendTracks(model) {
        for (let i = 0; i < 50; ++i)
            model.append({ trackId:i+1, rowKind:0, number:i+1, title:"Track", artist:"Band", album:"Record",
                duration:"3:00", plays:0, quality:"FLAC", tier:0, playing:false, selected:false,
                headerLabel:"", headerMeta:"", art:"", available:true })
    }
    function test_hidden_grid_restores_its_preview_edition_geometry_and_card() {
        const card = findChild(grid, "albumAnimatedCover").parent
        grid.model.rev = 17
        tryCompare(card, "rev", 17)
        compare(grid.detailsRev, 17)
        grid.model.expanded = 0
        const loader = findChild(grid, "albumQuickLoader")
        tryVerify(() => loader.item !== null)
        const detail = loader.item.children.find(child => child.albumDetail)?.albumDetail
        verify(detail)
        detail.selectedId = -7
        const list = findItem(loader.item, item => item.viTrackList === true)
        compare(list.viewport, page)
        appendTracks(list.model)
        tryVerify(() => list.height > page.height * 2)
        page.contentY = 150
        const height = grid.height
        const revision = card.rev
        page.visible = false
        tryCompare(loader, "item", null)
        grid.model.rev++
        wait(0)
        compare(card.rev, revision)
        compare(grid.detailsRev, grid.model.rev - 1)
        compare(grid.model.expanded, 0)
        compare(grid.height, height)
        compare(page.contentY, 150)
        page.visible = true
        tryVerify(() => loader.item !== null)
        compare(loader.item.albumId, card.albumId)
        compare(loader.item.children.find(child => child.albumDetail).albumDetail.albumId, -7)
        compare(grid.detailsRev, grid.model.rev)
        compare(card.rev, grid.model.rev)
        compare(grid.height, height)
        compare(page.contentY, 150)
        compare(findChild(grid, "albumAnimatedCover").parent, card)
        loader.item.close()
        tryCompare(grid.model, "expanded", -1)
        grid.model.expanded = 0
        tryVerify(() => loader.item !== null)
        compare(loader.item.version, card.albumId)
    }
    function test_hidden_list_releases_track_bodies_and_distant_quality_reads() {
        const list = findItem(page, item => item.viTrackList === true)
        compare(list.viewport, page)
        appendTracks(list.model)
        tryVerify(() => list.rows.itemAtIndex(49) !== null)
        const row = list.rows.itemAtIndex(0)
        const distant = list.rows.itemAtIndex(49)
        page.contentY += row.mapToItem(page, 0, 0).y
        tryVerify(() => row.creditedTitle !== null)
        verify(row.cols.qualityShown)
        compare(row.quality, "FLAC")
        compare(distant.quality, "")
        const height = list.height
        for (const outer of [page, null]) {
            page.visible = false
            list.viewport = outer
            tryCompare(row, "creditedTitle", null)
            compare(row.quality, "")
            compare(list.height, height)
            list.viewport = page
            page.visible = true
            tryVerify(() => row.creditedTitle !== null)
            compare(row.quality, "FLAC")
            compare(list.rows.itemAtIndex(0), row)
        }
    }
}
