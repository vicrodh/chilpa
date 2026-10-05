import QtQuick
import QtTest
import com.blitzfc.chilpa
import "../../qml/Components"
import "../../qml/Primitives"

TestCase {
    name: "AlbumCardQualityAndViewport"
    when: windowShown
    visible: true
    width: 1000; height: 800
    AlbumCard { id: card; width: 180; height: 230; title: "Record"; qualityInfo: ({tier:3,label:"FLAC 24/96"}) }
    AlbumGridModel { id: records }
    AlbumGrid { id: grid; y: 260; width: 1400; height: 500; fitContent: true; model: records; viewportWidth: 700; viewportX: 320 }
    function initTestCase() { records.append({first:0,span:6,group:""}) }
    function test_quality_is_visible_without_card_hover_and_has_a_tooltip() {
        const badge = findChild(card, "albumQualityBadge")
        verify(badge.visible)
        compare(badge.tier, 3)
        mouseMove(badge, badge.width / 2, badge.height / 2)
        tryCompare(TipState, "text", "FLAC 24/96")
        card.round = true
        verify(!badge.visible)
        card.round = false
    }
    function test_quick_view_uses_the_visible_width_and_tracks_horizontal_scroll() {
        records.expanded = 2
        const quick = findChild(grid, "albumQuickLoader")
        verify(quick !== null)
        tryCompare(quick, "active", true)
        verify(quick.width <= grid.viewportWidth)
        verify(quick.width > grid.viewportWidth - 10)
        verify(quick.x >= grid.viewportX)
        const previous = quick.x
        grid.viewportX += 100
        compare(quick.x, previous + 100)
        verify(quick.item.notchX >= 0 && quick.item.notchX <= quick.width)
    }
}
