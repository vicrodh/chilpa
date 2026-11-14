import QtQuick
import QtTest
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../../qml/Screens"
import "../../qml/Patterns"

TestCase {
    id:test
    property int retiredShelves:0
    property int uncancelledShelves:0
    property int cancelledViews:0
    name: "MusicDiscovery"
    when: windowShown
    visible: true
    width: 1100; height: 820
    Component { id: maker; DiscoveryScreen { width: 1100; height: 820 } }
    function init() { failOnWarning(/ReferenceError|TypeError|Binding loop|invalid context/);Visit.kept = ({}); Navigation.went = []; Navigation.page = ""; Navigation.selectedId = 0 }
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
    function test_search_order_and_scroll_return_after_leaving_the_route() {
        let page = createTemporaryObject(maker, this, {mode:"compositions"})
        findChild(page, "discoverySearch").text = "Suite"
        findChild(page, "discoverySort").picked("name-desc")
        wait(300)
        const items = Array.from({length:60}, (_, n) => ({kind:"work",name:"Suite " + n,composers:["Writer"]}))
        findChild(page, "discoveryModel").document = JSON.stringify({items:items,total:200})
        const view = findChild(page, "discoveryEntries")
        tryVerify(() => view.contentHeight > view.height + 400)
        view.contentY = 400
        page.destroy(); wait(1)
        page = createTemporaryObject(maker, this, {mode:"compositions"})
        compare(findChild(page, "discoverySort").value, "name-desc")
        compare(findChild(page, "discoverySearch").text, "Suite")
        wait(300)
        findChild(page, "discoveryModel").document = JSON.stringify({items:items,total:200})
        tryCompare(findChild(page, "discoveryEntries"), "contentY", 400)
    }
    function test_a_large_inventory_requests_the_viewport_without_a_pager_or_all_delegates() {
        const page = createTemporaryObject(maker, this, {mode:"composers"})
        const model = findChild(page, "discoveryModel")
        model.document = JSON.stringify({total:20000,items:[{kind:"composer",name:"First"}]})
        const view = findChild(page, "discoveryEntries")
        tryCompare(view, "count", 20000)
        verify(view.contentItem.children.length < 100)
        view.contentY = 10000
        tryVerify(() => model.seeks.length > 0 && model.seeks[model.seeks.length - 1].first > 60)
        verify(view.contentItem.children.length < 100)
        compare(findChild(page, "discoveryPager"), null)
    }
    function test_overview_has_search_sort_and_a_genre_scope_filter() {
        const page = createTemporaryObject(maker, this, {routeKey:JSON.stringify({kind:"genre",name:"Rock"})})
        const model = findChild(page, "discoveryModel")
        verify(findChild(page, "discoverySearch").visible)
        verify(findChild(page, "discoverySort").visible)
        verify(findChild(page, "discoveryFilter").visible)
        findChild(page, "discoverySearch").text = "metal"
        findChild(page, "discoveryFilter").picked("library")
        findChild(page, "discoverySort").picked("name-desc")
        tryVerify(() => model.requests.length > 0 && model.requests[model.requests.length - 1].text === "metal")
        compare(JSON.parse(model.requests[model.requests.length - 1].options), {order:"name-desc",scope:"library",excluded_sources:[]})
    }
    function test_genre_sources_follow_breadcrumbs_children_and_collection_tabs() {
        const excluded = ["plugin:other"]
        const page = createTemporaryObject(maker, this, {routeKey:JSON.stringify({kind:"genre",name:"Metal",trail:["Rock","Metal"],excluded_sources:excluded})})
        const model = findChild(page, "discoveryModel")
        tryVerify(() => model.requests.length > 0)
        compare(JSON.parse(model.requests[model.requests.length - 1].options).excluded_sources, excluded)
        page.open({kind:"genre",name:"Heavy Metal"})
        compare(JSON.parse(Navigation.went[1]).excluded_sources, excluded)
        compare(JSON.parse(page.collectionKey).excluded_sources, excluded)
        mouseClick(findChild(page, "discoveryRoot"))
        compare(Navigation.went[0], "genres")
        compare(JSON.parse(Navigation.went[1]), {excluded_sources:excluded})
    }
    function test_pending_source_tracks_cannot_trigger_old_collection_actions() {
        const page=createTemporaryObject(maker,this,{routeKey:JSON.stringify({kind:"genre",name:"Heavy Metal"})})
        page.changeTab(3)
        const list=findChild(page,"discoveryTrackList")
        verify(list)
        list.model.append({title:"Old source track",rowKind:0})
        list.model.ready=false
        verify(!findChild(page,"discoveryPlay").enabled)
        verify(!findChild(page,"discoveryQueue").enabled)
        verify(!list.enabled)
        list.model.ready=true
        verify(findChild(page,"discoveryPlay").enabled)
        verify(findChild(page,"discoveryQueue").enabled)
        verify(list.enabled)
    }
    function test_retargeting_a_genre_route_updates_its_source_scope_immediately() {
        const page=createTemporaryObject(maker,this,{routeKey:JSON.stringify({kind:"genre",name:"Metal",excluded_sources:["local:default"]})})
        page.routeKey=JSON.stringify({kind:"genre",name:"Heavy Metal",excluded_sources:["server:fixture"]})
        compare(findChild(page,"genreSources").excluded,["server:fixture"])
        compare(JSON.parse(page.collectionKey).excluded_sources,["server:fixture"])
    }
    function test_genre_source_choices_are_local_to_this_view_and_survive_return() {
        let page = createTemporaryObject(maker, this)
        const model = findChild(page, "discoveryModel")
        model.document = JSON.stringify({total:1,items:[{kind:"genre",name:"Rock"}],sources:[{key:"local:default",label:"Local library"},{key:"plugin:fixture",label:"Source"}]})
        const picker = findChild(page, "genreSources")
        verify(picker.visible); compare(picker.sources.length, 2)
        picker.chosen(["plugin:fixture"])
        tryVerify(() => model.requests.length > 0 && JSON.parse(model.requests[model.requests.length-1].options).excluded_sources.length === 1)
        compare(JSON.parse(page.collectionKey).excluded_sources, ["plugin:fixture"])
        page.destroy(); wait(1)
        page = createTemporaryObject(maker, this)
        compare(findChild(page, "genreSources").excluded, ["plugin:fixture"])
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
    function watchAlbumView(page) {
        const view = findChild(page, "discoveryContent").item.scrollView
        verify(!!view.model)
        view.modelChanged.connect(() => { if (!view.model) test.cancelledViews++ })
    }
    function test_switching_a_restored_composer_tab_cancels_its_view_before_removal() {
        const key = JSON.stringify({kind:"composer",name:"Writer"})
        const visits = {}
        visits["discovery.composers." + key] = {version:2,tab:1}
        Visit.keep("discovery.visits", visits)
        const page = createTemporaryObject(maker, this, {mode:"composers",routeKey:key})
        compare(page.tab, 1)
        test.cancelledViews = 0
        watchAlbumView(page)
        page.changeTab(0)
        compare(test.cancelledViews, 1, "cancel the album view before replacing its loader context")
        page.changeTab(1)
        verify(!!findChild(page, "discoveryContent").item.scrollView.model)
    }
    function test_hiding_a_detail_cancels_its_view_and_reopens_it() {
        const page = createTemporaryObject(maker, this, {mode:"composers",routeKey:JSON.stringify({kind:"composer",name:"Writer"})})
        page.changeTab(1)
        fillAlbumView(page).contentY = 500
        test.cancelledViews = 0
        watchAlbumView(page)
        page.visible = false
        compare(test.cancelledViews, 1, "cancel the album view before hiding its loader")
        compare(findChild(page, "discoveryContent").item, null)
        compare(page.positions[1], 500)
        page.visible = true
        const reopened = fillAlbumView(page)
        tryCompare(reopened, "contentY", 500)
    }
    function fillAlbumView(page) {
        const view = findChild(page, "discoveryContent").item.scrollView
        for (let index = 0; index < 40; index++) view.model.append({first:index * 3,span:3,group:"",revision:0})
        view.forceLayout()
        return view
    }
    function test_changing_tabs_keeps_the_other_tabs_scroll_position() {
        const page = createTemporaryObject(maker, this, {mode:"composers",routeKey:JSON.stringify({kind:"composer",name:"Writer"})})
        page.changeTab(1)
        const view = fillAlbumView(page)
        view.contentY = 500
        page.positions[0] = 123
        page.changeTab(0)
        compare(page.restoredPosition, 123, "retiring the old view must not reset the destination tab's position")
        compare(page.positions[1], 500)
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
        tryVerify(() => view.contentHeight > view.height)
        view.contentY = Math.max(0, shelf.mapToItem(view.contentItem, 0, 0).y - view.height / 2)
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
    Component {
        id:shelfMaker
        GenreOverview {
            width:1100; height:640; name:"Metal"
            about:({items:[],total:0,summary:{albums:4,images:[]}})
            artistsContent: Component {
                GridView {
                    objectName:"genreShelfPendingView"
                    property bool clearedBeforeDestroy:false
                    model:500; cellWidth:100; cellHeight:100
                    delegate:Item { width:100; height:100 }
                    onModelChanged:if (model===null || model===undefined) clearedBeforeDestroy=true
                    Component.onDestruction:{ test.retiredShelves++; if (!clearedBeforeDestroy) test.uncancelledShelves++ }
                }
            }
        }
    }
    function test_shelves_cancel_views_before_releasing_their_creation_contexts() {
        test.retiredShelves=0; test.uncancelledShelves=0
        const page=createTemporaryObject(shelfMaker,this)
        page.scrollView.contentY=600
        let grid
        tryVerify(()=>{grid=findChild(page,"genreShelfPendingView");return !!grid})
        grid.clearedBeforeDestroy=false
        page.scrollView.contentY=0
        tryCompare(findChild(page,"genreArtistShelf"),"active",false)
        tryCompare(test,"retiredShelves",1)
        compare(test.uncancelledShelves,0,"a view's model is cancelled while its creation context still exists")
        page.scrollView.contentY=600
        tryVerify(()=>{grid=findChild(page,"genreShelfPendingView");return !!grid && grid.model===500})
        page.scrollView.contentY=0
        tryCompare(test,"retiredShelves",2)
        compare(test.uncancelledShelves,0)
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
