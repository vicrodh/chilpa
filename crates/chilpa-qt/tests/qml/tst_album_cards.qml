import QtQuick
import QtTest
import app.chilpa
import "../../qml/Components"
import "../../../chilpa-qt-kit/qml/Primitives"

TestCase {
    name: "AlbumCardQualityAndViewport"
    when: windowShown
    visible: true
    width: 1000; height: 800
    AlbumCard { id: card; width: 180; height: 230; title: "Record"; qualityInfo: ({tier:3,label:"FLAC 24/96"}) }
    AlbumGridModel { id: records }
    AlbumGrid { id: grid; y: 260; width: 1400; height: 500; fitContent: true; model: records; viewportWidth: 700; viewportX: 320 }
    function initTestCase() { records.append({first:0,span:6,group:""}) }
    function test_quick_view_queue_button_opens_all_placements_and_extra_queues() {
        records.expanded = 0
        const quick = findChild(grid, "albumQuickLoader")
        tryCompare(quick, "active", true)
        const menu = findChild(quick.item, "quickQueueMenu")
        menu.model.append({title:"Track"})
        const button = findChild(quick.item, "quickQueueActions")
        compare(button.tip, "Queue actions")
        mouseClick(button)
        tryCompare(menu, "visible", true)
        compare(menu.entries.slice(0,3).map(entry => entry.act), ["queue","next","later"])
        menu.actionChosen(menu.entries[1])
        compare(menu.model.lastAction, "next")
        const extra = menu.entries[4]
        verify(extra.submenu)
        menu.actionChosen(extra.children[1])
        compare(menu.model.lastAction, "buffer:add:2")
        compare(menu.model.lastRow, -1)
        menu.actionChosen(extra.children[2])
        compare(menu.model.lastAction, "buffer:replace:2")
        menu.actionChosen(extra.children[0])
        compare(menu.model.lastAction, "buffer:new")
        compare(Navigation.page, "listenlist")
        menu.close()
        records.expanded = -1
    }
    // A record's menu offers every placement a track's does, and the extra
    // queues, which act on the record.
    function test_record_menu_offers_every_placement_and_the_extra_queues() {
        const menu = findChild(grid, "albumMenu")
        menu.show(7, "Record", "Artist", 0, 10, 10)
        tryCompare(menu, "visible", true)
        compare(menu.entries.slice(1, 4).map(entry => entry.act), ["queue", "next", "later"])
        menu.actionChosen(menu.entries[3])
        compare(AlbumActions.lastAction, "later:7:Record")
        const extra = menu.entries[menu.entries.length - 1]
        verify(extra.submenu)
        compare(extra.children.length, 3)
        menu.actionChosen(extra.children[1])
        compare(AlbumActions.lastAction, "buffer:add:2:7")
        menu.close()
    }
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
