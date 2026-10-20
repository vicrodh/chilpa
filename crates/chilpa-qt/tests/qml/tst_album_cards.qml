import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import "../../qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

TestCase {
    name: "AlbumCardQualityAndViewport"
    when: windowShown
    visible: true
    width: 1000; height: Math.max(800, grid.y + grid.height)
    AlbumCard { id: card; width: 180; height: 230; title: "Record"; qualityInfo: ({tier:3,label:"FLAC 24/96"}) }
    property int albumClicks: 0
    Connections { target: card; function onClicked() { albumClicks++ } }
    AlbumGridModel { id: records }
    AlbumGrid { id: grid; y: 260; width: 1400; height: implicitHeight; fitContent: true; model: records; viewportWidth: 700; viewportX: 320 }
    Component {
        id: distantRail
        Flickable {
            id: railPage
            width: 900; height: 500; contentHeight: 3000
            AlbumGridModel { id: shelfRecords; albums: 6; Component.onCompleted: append({first:0,span:6,group:""}) }
            AlbumCarousel { id: shelf; y: 1800; width: parent.width; viewport: railPage; title: "More records"; model: shelfRecords }
            property alias shelf: shelf
        }
    }
    function test_a_distant_rail_keeps_geometry_and_builds_its_cards_when_scrolled_into_view() {
        const page = createTemporaryObject(distantRail, this)
        verify(page)
        wait(100)
        const height = page.shelf.height
        verify(height > 0)
        compare(findChild(page, "albumAnimatedCover"), null, "I keep offscreen cards unconstructed")
        page.contentY = 1800
        tryVerify(() => findChild(page, "albumAnimatedCover") !== null)
        compare(page.shelf.height, height, "I keep the shelf's scroll geometry while its cards load")
    }
    function initTestCase() { records.append({first:0,span:6,group:""}) }
    function test_artist_name_opens_the_source_artist_without_opening_the_album() {
        card.albumId = -7; card.subtitle = "Band"
        AlbumActions.artistDestination = {target:"source",key:"fixture native:artist:7"}
        const label = findChild(card, "albumCardArtist")
        waitForRendering(label)
        albumClicks = 0
        mouseClick(label)
        compare(Navigation.went, ["source", "fixture native:artist:7"])
        compare(albumClicks, 0)
        AlbumActions.artistDestination = {}; card.albumId = 0; card.subtitle = ""
    }
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
    function test_mixed_module_artist_opens_without_album_quick_view() {
        records.kind = "source"; records.rev++
        grid.roundIds = [-1]
        SettingsModel.quickViewClick = true
        records.expanded = -1
        grid.clicked(0, -1, "Artist", 0, true)
        compare(records.expanded, -1)
        grid.clicked(1, -2, "Record", 0, true)
        compare(records.expanded, 1)
        grid.roundIds = []; SettingsModel.quickViewClick = false
        records.expanded = -1; records.kind = ""; records.rev++
    }
    // Beside a source's records the library's own wear the player's badge; alone, none.
    function test_the_records_of_the_library_wear_the_players_badge_beside_a_source() {
        tryVerify(() => findChild(grid, "albumAnimatedCover") !== null, 1000, "the grid laid its cards out")
        const first = findChild(grid, "albumAnimatedCover").parent
        records.kind = ""; records.mixed = true; records.rev++
        tryVerify(() => findChild(first, "sourceImage") !== null)
        tryCompare(findChild(first, "sourceImage"), "source", "qrc:/qt/qml/app/chilpa/kit/assets/logo.png")
        records.mixed = false; records.rev++
        tryVerify(() => findChild(first, "sourceImage") === null, 1000, "a grid of the library alone wears no badge")
    }
    function test_a_record_kept_by_several_sources_wears_each_one_inline() {
        tryVerify(() => findChild(grid, "albumAnimatedCover") !== null, 1000, "the grid laid its cards out")
        const first = findChild(grid, "albumAnimatedCover").parent
        AlbumActions.sourcesOf = {[first.albumId]: [{key: "local:default", local: true},
            {key: "one", icon: "qrc:/qt/qml/app/chilpa/kit/assets/logo.png?one", local: false},
            {key: "two", icon: "qrc:/qt/qml/app/chilpa/kit/assets/logo.png?two", local: false}]}
        records.rev++
        const badges = findChild(first, "sourceBadges")
        tryCompare(badges, "visible", true)
        tryCompare(first.sources, "length", 3, 1000, "the library's own and each source's, side by side")
        compare(first.sources[0].icon, "qrc:/qt/qml/app/chilpa/kit/assets/logo.png", "the library's own first, as the player's logo")
        AlbumActions.sourcesOf = {}
        records.rev++
        tryCompare(first.sources, "length", 0)
    }
    function test_quick_view_queue_button_opens_all_placements_and_extra_queues() {
        tryVerify(() => findChild(grid, "albumQuickLoader") !== null)
        records.expanded = 0
        const quick = findChild(grid, "albumQuickLoader")
        tryCompare(quick, "active", true)
        const menu = findChild(quick.item, "quickQueueMenu")
        menu.model.append({title:"Track"})
        const button = findChild(quick.item, "quickQueueActions")
        compare(button.tip, "Queue actions")
        waitForRendering(button)
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
    function test_overlay_shows_only_the_first_genre_without_changing_metadata() {
        card.albumId = 5
        AlbumActions.informationDocument = {genre:"Rock, Alternative, Pop", genres:["Rock", "Alternative", "Pop"]}
        mouseMove(card, card.width / 2, card.width / 2)
        tryVerify(() => shown(card).includes("Rock"))
        verify(!shown(card).includes("Rock, Alternative, Pop"))
        compare(AlbumActions.informationDocument.genres.length, 3)
        mouseMove(card, card.width / 2, card.height + 40)
        AlbumActions.informationDocument = {}; card.albumId = 0
    }
    function test_source_album_overlay_uses_its_native_favorite() {
        Sources.show({cards: {"-7": {plugin: "test", ref: "album:record", favorite: false}}})
        card.albumId = -7
        mouseMove(card, card.width / 2, card.width / 2)
        const favorite = findChild(card, "albumFavorite")
        tryVerify(() => favorite !== null && favorite.visible)
        compare(favorite.checked, false)
        waitForRendering(favorite)
        mouseClick(favorite, favorite.width / 2, favorite.height / 2)
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
