import QtQuick
import QtTest
import app.chilpa
import "../../qml/Components"

TestCase {
    name: "AlbumViewportGeometry"
    when: windowShown
    visible: true
    width: 1000; height: 600
    property var page
    Component {
        id: fixture
        Flickable {
            id: viewport
            width: 900; height: 500; contentHeight: 4000
            property alias ancestor: ancestor
            property alias grid: grid
            AlbumGridModel {
                id: records; albums: 6
                property var artPriorities: ({})
                function art(index, revision, visible) { artPriorities[index] = visible; return "" }
                Component.onCompleted: append({first:0,span:6,group:"", revision:0})
            }
            Item {
                id: ancestor
                width: viewport.width; height: grid.height
                transformOrigin: Item.TopLeft
                AlbumGrid { id: grid; width: parent.width; height: implicitHeight; fitContent: true; viewport: viewport; fixedColumns: 6; model: records }
            }
        }
    }
    function init() {
        failOnWarning(/Binding loop|ReferenceError|TypeError/)
        page = createTemporaryObject(fixture, this)
        verify(page)
        tryVerify(() => findChild(page, "albumAnimatedCover") !== null)
    }
    function test_an_ancestor_move_updates_the_band_without_scrolling() {
        compare(page.contentY, 0)
        compare(page.grid.visibleTop, 0)
        const height = page.grid.height
        page.ancestor.y = 2000
        tryCompare(page.grid, "visibleTop", -2000)
        tryVerify(() => findChild(page, "albumAnimatedCover") === null)
        compare(page.grid.height, height)
        compare(page.contentY, 0)
        page.ancestor.y = 0
        tryVerify(() => findChild(page, "albumAnimatedCover") !== null)
        compare(page.grid.visibleTop, 0)
    }
    function test_retained_read_ahead_is_promoted_only_when_it_enters_the_viewport() {
        tryVerify(() => page.grid.model.artPriorities[0] === true)
        const cover = findChild(page, "albumAnimatedCover")
        page.visible = false
        tryVerify(() => page.grid.model.artPriorities[0] === false)
        page.visible = true
        tryVerify(() => page.grid.model.artPriorities[0] === true)
        compare(findChild(page, "albumAnimatedCover"), cover)
        page.ancestor.y = 550
        tryVerify(() => page.grid.model.artPriorities[0] === false)
        compare(findChild(page, "albumAnimatedCover"), cover)
        page.ancestor.y = 450
        tryVerify(() => page.grid.model.artPriorities[0] === true)
        compare(findChild(page, "albumAnimatedCover"), cover)
        compare(page.contentY, 0)
    }
    function test_horizontal_shelf_promotes_a_retained_card_when_it_scrolls_into_view() {
        const records = page.grid.model
        records.maxRows = 1
        records.clear()
        records.artPriorities = ({})
        for (let index = 0; index < 12; index++) records.append({first:index,span:1,group:"",revision:0})
        page.grid.scrollView.forceLayout()
        const next = Math.ceil(page.grid.scrollView.width / page.grid.cardWidth)
        tryVerify(() => records.artPriorities[next] === false)
        page.grid.scrollView.contentX = page.grid.cardWidth * 2
        tryVerify(() => records.artPriorities[next] === true)
        compare(page.contentY, 0)
    }
    function test_shelf_height_follows_late_quickview_metadata_without_feedback() {
        const grid=page.grid, records=grid.model
        records.maxRows=1
        records.clear()
        for (let index=0;index<6;index++) records.append({first:index,span:1,group:"",revision:0})
        grid.scrollView.forceLayout()
        waitForRendering(grid)
        const base=grid.height
        records.expanded=0
        let quick=null
        tryVerify(()=> { quick=findChild(grid.scrollView.itemAtIndex(0),"albumQuickLoader");return quick?.item?.height>0 })
        const panel=quick.item.children.find(child=>child.albumDetail!==undefined)
        verify(panel)
        const detail=panel.albumDetail
        const firstHeight=quick.item.height
        detail.meta="Late release information with credits and format. ".repeat(20)
        tryVerify(()=>quick.item.height>firstHeight)
        tryVerify(()=>grid.height>base+quick.item.height)
        records.expanded=-1
        tryCompare(grid,"height",base)
    }
    function test_mapping_preserves_ancestor_scale_rotation_and_reparenting() {
        page.grid.x = 100
        page.ancestor.y = 300
        page.ancestor.scale = 1.5
        page.ancestor.rotation = 15
        const expected = page.contentY - page.grid.mapToItem(page.contentItem, 0, 0).y
        tryVerify(() => Math.abs(page.grid.visibleTop - expected) < 0.01)
        page.ancestor.rotation = 30
        const moved = page.contentY - page.grid.mapToItem(page.contentItem, 0, 0).y
        verify(Math.abs(moved - expected) > 1)
        tryVerify(() => Math.abs(page.grid.visibleTop - moved) < 0.01)
        compare(page.contentY, 0)
        page.grid.parent = page.contentItem
        tryCompare(page.grid, "visibleTop", 0)
    }
}
