import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
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
    function test_remote_mix_card_shows_animated_art_with_empty_url() {
        Sources.show({artColors:{"-1":["#221725","#d17655","#f8dda7"]}})
        records.kind = "source"
        records.rev++
        const mix = findChild(grid, "albumAnimatedCover")
        tryCompare(mix, "visible", true)
        compare(mix.colors.length, 3)
        records.kind = ""
        records.rev++
    }
    // Beside a source's records the library's own wear the player's badge; alone, none.
    function test_the_records_of_the_library_wear_the_players_badge_beside_a_source() {
        tryVerify(() => findChild(grid, "albumAnimatedCover") !== null, 1000, "the grid laid its cards out")
        const image = findChild(findChild(grid, "albumAnimatedCover").parent, "sourceImage")
        records.kind = ""; records.mixed = true; records.rev++
        tryCompare(image, "source", "qrc:/qt/qml/app/chilpa/kit/assets/logo.png")
        records.mixed = false; records.rev++
        tryCompare(image, "source", "")
    }
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
    // A name the card cuts short says itself whole on hover; one that fits asks for nothing.
    function test_a_name_cut_short_shows_whole_in_a_tooltip() {
        const name = findChild(card, "albumTitle")
        card.title = "A Record Whose Name Runs Far Past The Width Of Its Card"
        mouseMove(name, name.width / 2, name.height / 2)
        tryCompare(TipState, "text", card.title)
        mouseMove(card, card.width / 2, card.width / 2); TipState.text = ""; card.title = "Record"
        mouseMove(name, 4, name.height / 2)
        compare(TipState.text, "")
    }
    function shown(item) {
        let texts = []
        for (const child of item.children) {
            if (!child.visible) continue
            if (typeof child.text === "string") texts.push(child.text)
            texts = texts.concat(shown(child))
        }
        return texts
    }
    // Under the pointer a card says the day its record came out, when that is
    // known, and the year otherwise.
    function test_the_overlay_says_the_whole_release_date_when_it_is_known() {
        card.albumId = 5; card.year = "1981"
        AlbumActions.informationDocument = {released: "April 26, 1981"}
        mouseMove(card, card.width / 2, card.width / 2)
        tryVerify(() => shown(card).includes("April 26, 1981"))
        mouseMove(card, card.width / 2, card.height + 40)
        AlbumActions.informationDocument = {}
        mouseMove(card, card.width / 2, card.width / 2)
        tryVerify(() => shown(card).includes("1981"), 1000, "the year when no day is known")
        mouseMove(card, card.width / 2, card.height + 40)
        card.albumId = 0; card.year = ""
    }
    function test_source_album_overlay_uses_its_native_favorite() {
        Sources.show({cards: {"-7": {plugin: "test", ref: "album:record", favorite: false}}})
        card.albumId = -7
        mouseMove(card, card.width / 2, card.width / 2)
        const favorite = findChild(card, "albumFavorite")
        tryVerify(() => favorite !== null && favorite.visible)
        compare(favorite.checked, false)
        mouseClick(favorite)
        const command = Sources.asked[Sources.asked.length - 1]
        compare(command[0], "native")
        compare(command[1], "test")
        compare(JSON.parse(command[2]), {action: "favorite", ref: "album:record", on: true})
        Sources.show({cards: {"-7": {plugin: "test", ref: "album:record", favorite: true}}})
        Library.remoteRev++
        tryCompare(favorite, "checked", true)
        mouseMove(card, card.width / 2, card.height + 40)
        card.albumId = 0
        Sources.show({cards: {}})
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
