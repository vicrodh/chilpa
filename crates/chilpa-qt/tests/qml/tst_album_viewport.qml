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
            AlbumGridModel { id: records; albums: 6; Component.onCompleted: append({first:0,span:6,group:"", revision:0}) }
            Item {
                id: ancestor
                width: viewport.width; height: grid.height
                transformOrigin: Item.TopLeft
                AlbumGrid { id: grid; width: parent.width; height: implicitHeight; fitContent: true; viewport: viewport; fixedColumns: 6; model: records }
            }
        }
    }
    function init() {
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
