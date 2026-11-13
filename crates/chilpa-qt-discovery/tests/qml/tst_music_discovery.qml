import QtQuick
import QtTest
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../../qml/Screens"
import "../../qml/Patterns"

TestCase {
    name: "MusicDiscovery"
    when: windowShown
    visible: true
    width: 1100; height: 820
    Component { id: maker; DiscoveryScreen { width: 1100; height: 820 } }
    function init() { Visit.kept = ({}); Navigation.went = []; Navigation.page = ""; Navigation.selectedId = 0 }
    function test_a_deep_genre_keeps_its_path_in_the_route() {
        const page = createTemporaryObject(maker, this, {routeKey:JSON.stringify({kind:"genre",name:"Death Metal",trail:["Metal","Death Metal"]})})
        verify(page)
        page.open({kind:"genre",name:"Technical Death Metal"})
        compare(Navigation.went[0], "genres")
        const key = JSON.parse(Navigation.went[1])
        compare(key.trail, ["Metal","Death Metal","Technical Death Metal"])
        compare(key.name, "Technical Death Metal")
    }
    function test_a_composition_and_a_recording_keep_their_own_context() {
        const page = createTemporaryObject(maker, this, {mode:"compositions",routeKey:JSON.stringify({kind:"work",name:"Suite",work:"work:1",composers:["Writer"]})})
        page.open({kind:"recording",recording:"recording:2",name:"Live album",artist:"Quartet",album:42})
        const key = JSON.parse(Navigation.went[1])
        compare(key.work, "work:1")
        compare(key.composers, ["Writer"])
        compare(key.recording, "recording:2")
        compare(key.album, 42)
    }
    function test_the_breadcrumb_returns_to_the_root_without_a_key() {
        const page = createTemporaryObject(maker, this, {mode:"genres",routeKey:JSON.stringify({kind:"genre",name:"Death Metal"})})
        mouseClick(findChild(page, "discoveryRoot"))
        compare(Navigation.page, "genres")
        compare(Navigation.selectedId, 0)
    }
    function test_hidden_pages_stop_queries() {
        const page = createTemporaryObject(maker, this)
        const model = findChild(page, "discoveryModel")
        verify(model)
        page.visible = false
        const requests = model.requests.length
        Library.remoteRev++
        wait(400)
        compare(model.requests.length, requests)
        verify(model.cancellations > 0)
        page.visible = true
        tryVerify(() => model.requests.length > requests)
    }
    function test_search_page_and_scroll_return_after_leaving_the_route() {
        let page = createTemporaryObject(maker, this, {mode:"compositions"})
        findChild(page, "discoverySearch").text = "Suite"
        page.number = 2
        wait(300)
        const items = Array.from({length:60}, (_, n) => ({kind:"work",name:"Suite " + n,composers:["Writer"]}))
        findChild(page, "discoveryModel").document = JSON.stringify({items:items,total:200})
        const view = findChild(page, "discoveryEntries")
        tryVerify(() => view.contentHeight > view.height + 400)
        view.contentY = 400
        page.destroy(); wait(1)
        page = createTemporaryObject(maker, this, {mode:"compositions"})
        compare(page.number, 2)
        compare(findChild(page, "discoverySearch").text, "Suite")
        wait(300)
        findChild(page, "discoveryModel").document = JSON.stringify({items:items,total:200})
        tryCompare(findChild(page, "discoveryEntries"), "contentY", 400)
    }
    function test_a_source_recording_opens_its_album_by_keyboard_on_a_narrow_page() {
        const page = createTemporaryObject(maker, this, {width:560,mode:"compositions",routeKey:JSON.stringify({
            kind:"work",name:"Suite",work:"work:1",recording:"source:part",album:-7,
            recordingTitle:"A long recording title ".repeat(20),performer:"Quartet"})})
        const button = findChild(page, "discoveryRecordingAlbum")
        verify(button.visible)
        verify(button.width <= page.width)
        const list = findChild(page, "discoveryTrackList")
        verify(list)
        verify(list.columns.titleW >= 150)
        button.forceActiveFocus()
        keyClick(Qt.Key_Return)
        compare(Navigation.page, "album")
        compare(Navigation.selectedId, -7)
    }
    function test_only_the_selected_detail_view_is_mounted() {
        const page = createTemporaryObject(maker, this, {mode:"composers",routeKey:JSON.stringify({kind:"composer",name:"Writer"})})
        compare(findChild(page, "discoveryContent").item.toString().includes("DiscoveryWorkTable"), true)
        page.tab = 2
        tryVerify(() => findChild(page, "discoveryContent").item.toString().includes("DiscoveryTracks"))
    }
    function test_a_retained_genre_tile_releases_images_outside_the_viewport() {
        const page = createTemporaryObject(maker, this)
        const image = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='8' height='8'%3E%3Crect width='8' height='8' fill='navy'/%3E%3C/svg%3E"
        const items = Array.from({length:60}, (_, n) => ({kind:"genre",name:"Style " + n,images:[image],albums:n + 1}))
        findChild(page, "discoveryModel").document = JSON.stringify({items:items,total:60})
        const view = findChild(page, "discoveryEntries")
        let tile
        tryVerify(() => { tile = findChild(page, "discoveryEntry:Style 0"); return !!tile })
        const picture = findChild(tile, "discoveryMosaicImage0")
        verify(picture)
        tryVerify(() => picture.source.toString().length > 0)
        view.contentY = view.cellHeight
        tryCompare(tile, "activeImages", false)
        compare(picture.source.toString(), "")
        view.contentY = 0
        tryCompare(tile, "activeImages", true)
        verify(picture.source.toString().length > 0)
        page.visible = false
        tryCompare(tile, "activeImages", false)
        compare(picture.source.toString(), "")
    }
    function test_a_work_table_row_opens_its_recordings_by_keyboard_on_a_narrow_page() {
        const page = createTemporaryObject(maker, this, {width:560,mode:"compositions"})
        findChild(page, "discoveryModel").document = JSON.stringify({total:1,items:[{kind:"work",name:"Suite",work:"work:3",composers:["Writer"],albums:2,tracks:8}]})
        const view = findChild(page, "discoveryEntries")
        tryVerify(() => !!view.itemAtIndex(0))
        const row = view.itemAtIndex(0)
        verify(row.width <= page.width)
        row.forceActiveFocus()
        keyClick(Qt.Key_Return)
        const key = JSON.parse(Navigation.went[1])
        compare(key.kind, "work")
        compare(key.work, "work:3")
        compare(key.composers, ["Writer"])
    }
    function test_a_genre_overview_keeps_shelves_lazy_and_restores_its_scroll() {
        const page = createTemporaryObject(maker, this, {height:640,routeKey:JSON.stringify({kind:"genre",name:"Metal"})})
        const items = Array.from({length:8}, (_, n) => ({kind:"genre",name:"Style " + n,images:[],albums:n + 1}))
        findChild(page, "discoveryModel").document = JSON.stringify({items:items,total:8,summary:{albums:4,images:[]}})
        const view = findChild(page, "genreOverviewScroll")
        verify(view)
        verify(findChild(page, "genreHero"))
        const shelf = findChild(page, "genreAlbumShelf")
        tryCompare(shelf, "active", true)
        tryCompare(findChild(page, "genreArtistShelf"), "active", false)
        view.contentY = 900
        tryCompare(shelf, "active", false)
        compare(shelf.item, null)
        page.changeTab(1)
        page.changeTab(0)
        tryCompare(findChild(page, "genreOverviewScroll"), "contentY", 900)
        page.visible = false
        tryCompare(findChild(page, "discoveryContent"), "active", false)
    }
    function test_existing_genre_visits_keep_their_selected_section_after_adding_overview() {
        const key = JSON.stringify({kind:"genre",name:"Metal"})
        const visits = {}
        visits["discovery.genres." + key] = {tab:1,positions:{1:123}}
        Visit.keep("discovery.visits", visits)
        const page = createTemporaryObject(maker, this, {routeKey:key})
        compare(page.tab, 2)
        compare(page.positions[2], 123)
    }
    function test_playing_a_genre_requires_the_explicit_play_action() {
        const page = createTemporaryObject(maker, this, {routeKey:JSON.stringify({kind:"genre",name:"Metal"})})
        findChild(page, "discoveryModel").document = JSON.stringify({items:[],total:0,summary:{albums:4,images:[]}})
        compare(page.playSelection, false)
        const play = findChild(page, "genrePlay")
        verify(play.enabled)
        waitForRendering(play)
        mouseClick(play)
        compare(page.tab, 3)
        compare(page.playSelection, true)
    }
}
