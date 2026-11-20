import QtQuick
import QtTest
import app.chilpa
import app.chilpa.artwork
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Screens"
import "../../qml/Patterns"

TestCase {
    id:test
    property int retiredShelves:0
    property int uncancelledShelves:0
    property int cancelledViews:0
    property int genreRequests:0
    name: "MusicDiscovery"
    when: windowShown
    visible: true
    width: 1100; height: 820
    Component { id: maker; DiscoveryScreen { width: 1100; height: 820 } }
    Component { id: genreBrowserMaker; AlbumGenreBrowser {} }
    Component { id: genrePluginMaker; GenrePluginShelves { width: 800; nearViewport: true } }
    Component {
        id: albumHeroMaker
        AlbumHero {
            width:1100
            detail:AlbumDetailModel { selectedId:17;title:"Album";genreLinks:'{"items":[],"total":9,"hasMore":true}' }
            onGenresRequested:test.genreRequests++
        }
    }
    function init() { failOnWarning(/ReferenceError|TypeError|Binding loop|invalid context/);Visit.kept = ({}); Navigation.went = []; Navigation.page = ""; Navigation.selectedId = 0 }
    function test_an_empty_plugin_section_can_receive_replies_when_scrolled_into_view() {
        const page = createTemporaryObject(maker, this, {height:600, routeKey:'{"kind":"genre","name":"Rock"}'})
        const model = findChild(page, "discoveryModel")
        model.document = '{"items":[],"summary":{"albums":4}}'
        const content = findChild(page, "discoveryContent")
        tryCompare(content, "visible", true)
        const viewport = content.item.scrollView
        const shelves = findChild(content.item, "genrePluginShelves")
        const feed = findChild(shelves, "genrePluginModel")
        tryVerify(() => viewport.contentHeight > viewport.height)
        viewport.contentY = viewport.contentHeight - viewport.height
        tryCompare(shelves, "nearViewport", true)
        compare(shelves.height, 0)
        const before = feed.requests.length
        Library.remoteRev++
        tryVerify(() => feed.requests.length > before)
    }

    function test_plugin_discovery_groups_albums_and_artists_by_source_and_omits_empty_sections() {
        const shelves = createTemporaryObject(genrePluginMaker, this, {genreKey:'{"kind":"genre","name":"Rock"}'})
        const feed = findChild(shelves, "genrePluginModel")
        tryVerify(() => feed.requests.length > 0)
        compare(feed.requests[0].kind, "genre_plugins")
        verify(!findChild(shelves, "genrePluginReleases").visible)
        verify(!findChild(shelves, "genrePluginArtists").visible)
        const releases = [1,2,3,4,5,6,7,8].map(id => ({id:-id, name:"Release " + id, credit:"Performer", image:""}))
        const artists = [1,2,3].map(id => ({id:-10-id, key:"one native:artist:" + id, name:"Artist " + id, image:""}))
        feed.document = JSON.stringify({items:[], plugins:[
            {id:"one", name:"First source", newReleases:releases, artists:[]},
            {id:"two", name:"Second source", newReleases:releases, artists}]})
        verify(findChild(shelves, "genrePluginReleaseRow:one").visible)
        verify(findChild(shelves, "genrePluginReleaseRow:two").visible)
        verify(!findChild(shelves, "genrePluginArtistRow:one"))
        const artistRow = findChild(shelves, "genrePluginArtistRow:two")
        const releaseRow = findChild(shelves, "genrePluginReleaseRow:one")
        const artistCards = findChild(artistRow, "albumCarouselRail")
        const releaseCards = findChild(releaseRow, "albumCarouselRail")
        compare(artistRow.round, true)
        compare(releaseRow.round, false)
        compare(releaseRow.model.kind, "ids")
        compare(JSON.parse(releaseRow.model.key), [-1,-2,-3,-4,-5,-6,-7,-8])
        compare(releaseRow.size, 8)
        tryCompare(artistCards, "count", 3)
        tryCompare(releaseCards, "count", 8)
        tryVerify(() => releaseRow.farthest > 0)
        artistCards.forceLayout()
        releaseCards.forceLayout()
        verify(waitForRendering(shelves))
        const artistGrid = findChild(artistRow, "albumGridInteraction").grid
        const releaseInteraction = findChild(releaseRow, "albumGridInteraction")
        artistGrid.open(-11, "Artist 1")
        compare(Navigation.went, ["source", "one native:artist:1"])
        releaseInteraction.grid.open(-1, "Release 1")
        compare(Navigation.selectedId, -1)
        releaseInteraction.toggleQuick(0)
        compare(releaseRow.model.expanded, 0)
        releaseInteraction.toggleQuick(0)
        compare(releaseRow.model.expanded, -1)
        shelves.genreKey = '{"kind":"genre","name":"Jazz"}'
        verify(!findChild(shelves, "genrePluginReleases").visible)
    }

    function test_a_delayed_genre_overview_shows_a_skeleton_until_the_summary_arrives() {
        const page=createTemporaryObject(maker,this,{routeKey:JSON.stringify({kind:"genre",name:"Asian"})})
        const model=findChild(page,"discoveryModel")
        const content=findChild(page,"discoveryContent")
        const skeleton=findChild(page,"discoveryLoading")
        model.loading=true
        tryVerify(()=>model.requests.length>0)
        verify(skeleton.visible)
        verify(!content.visible)
        verify(!findChild(page,"discoveryMessage").visible)
        Library.contentRev++
        tryVerify(()=>model.requests.length>1)
        verify(skeleton.visible,"a pending query must not turn into an empty result")
        model.document=JSON.stringify({items:[],total:0,summary:{albums:6,images:[]}})
        verify(!content.visible,"the summary is not published until loading finishes")
        model.loading=false
        tryVerify(()=>content.visible)
        verify(!skeleton.visible)
        verify(findChild(page,"genrePlay").enabled)
        findChild(page,"genreSources").chosen(["local:default"])
        verify(skeleton.visible,"the previous source's summary must disappear before debounce")
        verify(!content.visible)
    }
    function test_a_failed_genre_summary_offers_retry_without_a_zero_album_header() {
        const page=createTemporaryObject(maker,this,{routeKey:JSON.stringify({kind:"genre",name:"Asian"})})
        const model=findChild(page,"discoveryModel")
        tryVerify(()=>model.requests.length>0)
        model.document=JSON.stringify({items:[],total:0,error:"reader temporarily unavailable"})
        tryVerify(()=>findChild(page,"discoveryMessage").visible)
        verify(!findChild(page,"discoveryContent").visible)
        compare(page.failure,"reader temporarily unavailable")
        const count=model.requests.length
        findChild(page,"discoveryRetry").clicked()
        verify(findChild(page,"discoveryLoading").visible)
        tryVerify(()=>model.requests.length>count)
        model.document=JSON.stringify({items:[],total:0,summary:{albums:6,images:[]}})
        tryVerify(()=>findChild(page,"discoveryContent").visible)
        verify(!findChild(page,"discoveryMessage").visible)
    }
    function test_album_queries_keep_loading_and_errors_distinct_from_an_empty_result() {
        const page=createTemporaryObject(maker,this,{mode:"composers",routeKey:JSON.stringify({kind:"composer",name:"Writer"})})
        page.changeTab(1)
        const records=findChild(page,"discoveryAlbumModel")
        const content=findChild(page,"discoveryContent")
        records.ready=false
        verify(findChild(page,"discoveryLoading").visible)
        verify(!content.visible)
        verify(!findChild(page,"discoveryMessage").visible)
        records.error="could not open library reader";records.ready=true
        verify(findChild(page,"discoveryMessage").visible)
        verify(!content.visible)
        findChild(page,"discoveryRetry").clicked()
        compare(records.retries,1)
        verify(findChild(page,"discoveryLoading").visible)
        records.ready=true
        verify(content.visible,"only a completed successful result may show the empty selection")
    }
    function test_a_delayed_album_shelf_keeps_the_summary_and_shows_loading_or_retry() {
        const page=createTemporaryObject(maker,this,{height:1100,routeKey:JSON.stringify({kind:"genre",name:"Asian"})})
        findChild(page,"discoveryModel").document=JSON.stringify({items:[],total:0,summary:{albums:6,images:[]}})
        let records
        tryVerify(()=>{records=findChild(page,"genreAlbumModel");return !!records})
        records.ready=false
        verify(findChild(page,"genreAlbumsLoading").visible)
        verify(!findChild(page,"genreAlbumsMessage").visible)
        verify(findChild(page,"genreHero").visible)
        records.error="reader temporarily unavailable";records.ready=true
        verify(findChild(page,"genreAlbumsMessage").visible)
        verify(!findChild(page,"genreAlbumsLoading").visible)
        findChild(page,"genreAlbumsRetry").clicked()
        compare(records.retries,1)
        verify(findChild(page,"genreAlbumsLoading").visible)
        records.albums=6;records.ready=true
        verify(!findChild(page,"genreAlbumsMessage").visible)
    }
    function test_leaving_the_injected_artist_grid_cancels_its_view_before_removal() {
        const page=createTemporaryObject(maker,this,{routeKey:JSON.stringify({kind:"genre",name:"Metal"})})
        page.changeTab(2)
        findChild(page,"discoveryArtistModel").albums=40
        const view=findChild(page,"discoveryContent").item.scrollView
        verify(view && view.model===40)
        test.cancelledViews=0
        view.modelChanged.connect(()=>{if(view.model===null || view.model===undefined)test.cancelledViews++})
        page.changeTab(1)
        compare(test.cancelledViews,1)
    }
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
        model.document=JSON.stringify({items:[{kind:"genre",name:"Rock"}],total:20000})
        page.visible = false
        compare(model.count,0)
        const requests = model.requests.length
        Library.remoteRev++
        wait(400)
        compare(model.requests.length, requests)
        verify(model.cancellations > 0)
        page.visible = true
        tryVerify(() => model.requests.length > requests)
    }
    function test_album_genres_are_virtual_and_keep_the_opaque_source_route() {
        const browser = createTemporaryObject(genreBrowserMaker, this, {albumId:17})
        verify(browser)
        browser.open()
        const model = findChild(browser, "albumGenreModel")
        tryVerify(() => model.requests.length > 0)
        compare(model.requests[0].kind, "album_genres")
        compare(model.requests[0].key, "17")
        const key = JSON.stringify({kind:"genre",name:"Heavy Metal",excluded_sources:["plugin:other"]})
        model.document = JSON.stringify({total:20000,items:[{name:"heavy-metal",key:key}]})
        const rows = findChild(browser, "albumGenreRows")
        tryCompare(rows, "count", 20000)
        verify(rows.width > 0 && rows.height > 0, "The genre viewport has visible room: " + rows.width + " by " + rows.height)
        verify(rows.contentItem.children.length < 40)
        tryVerify(() => !!findChild(rows.contentItem, "albumGenreResult:0"))
        const first = findChild(rows.contentItem, "albumGenreResult:0")
        first.forceActiveFocus(); keyClick(Qt.Key_Return)
        compare(Navigation.went, ["genres",key])
        tryCompare(browser, "visible", false)
        verify(model.cancellations > 0)
    }
    function test_album_header_composition_forwards_the_genre_browser_action() {
        genreRequests=0
        const hero=createTemporaryObject(albumHeroMaker,this)
        verify(hero)
        findChild(hero,"albumGenresMore").clicked()
        compare(genreRequests,1)
    }
    function test_closed_album_genres_cancel_debounce_and_do_not_request_hidden_pages() {
        const browser = createTemporaryObject(genreBrowserMaker, this, {albumId:-7})
        browser.open()
        const model = findChild(browser, "albumGenreModel")
        tryVerify(() => model.requests.length > 0)
        findChild(browser, "albumGenreSearch").text = "metal"
        browser.close()
        const requests = model.requests.length
        Library.remoteRev++
        wait(400)
        compare(model.requests.length, requests)
        browser.open()
        tryVerify(() => model.requests.length > requests)
        model.document = JSON.stringify({total:20000,items:[]})
        const rows = findChild(browser, "albumGenreRows")
        rows.contentY = 10000
        tryVerify(() => model.seeks.length > 0 && model.seeks[model.seeks.length - 1].first > 60)
        browser.albumId = -8
        tryCompare(browser, "visible", false)
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
        findChild(page,"discoveryModel").document = '{"items":[]}'
        page.changeTab(3)
        const list=findChild(page,"discoveryTrackList")
        verify(list)
        list.model.append({title:"Old source track",artist:"Artist",album:"Album",rowKind:0,trackId:1,playing:false,selected:false,available:true})
        const play = findChild(page,"collectionPlay")
        const shuffle = findChild(page,"collectionShuffle")
        const queue = findChild(page,"collectionQueue")
        list.model.ready=false
        verify(!play.enabled && !shuffle.enabled && !queue.enabled)
        verify(!list.enabled)
        list.model.ready=true
        tryCompare(play, "enabled", true)
        verify(shuffle.enabled && queue.enabled && list.enabled)
        verify(!findChild(page,"collectionRandomAlbum").visible)
    }

    function test_genre_albums_use_the_loaded_collection_for_all_playback_actions() {
        const page=createTemporaryObject(maker,this,{routeKey:JSON.stringify({kind:"genre",name:"Rock"})})
        findChild(page,"discoveryModel").document = '{"items":[]}'
        page.changeTab(1)
        const controls = findChild(page,"collectionActions")
        const albums = findChild(page,"discoveryAlbumModel")
        const random = findChild(controls,"collectionRandomAlbum")
        compare(controls.model, albums)
        verify(!controls.enabled)
        albums.albums = 25
        const shuffle = findChild(controls,"collectionShuffle")
        tryCompare(shuffle, "enabled", true)
        verify(random.visible && random.enabled)
        tryVerify(() => shuffle.x > findChild(controls,"collectionPlay").x)
        verify(waitForRendering(shuffle))
        mouseClick(shuffle)
        const modal = findChild(controls,"collectionSizeDialog")
        tryCompare(modal, "opened", true)
        modal.close()
        findChild(page,"discoverySearch").text = "One record"
        verify(controls.filtered)
    }

    function test_retargeting_a_genre_route_updates_its_source_scope_immediately() {
        const page=createTemporaryObject(maker,this,{routeKey:JSON.stringify({kind:"genre",name:"Metal",excluded_sources:["local:default"]})})
        page.routeKey=JSON.stringify({kind:"genre",name:"Heavy Metal",excluded_sources:["server:fixture"]})
        compare(findChild(page,"genreSources").excluded,["server:fixture"])
        compare(JSON.parse(page.collectionKey).excluded_sources,["server:fixture"])
    }
    function test_an_album_genre_hydrates_its_source_scope_until_the_listener_changes_it() {
        const key={kind:"genre",name:"Curated genre",album:17,genreToken:"opaque",albumScope:true}
        const page=createTemporaryObject(maker,this,{routeKey:JSON.stringify(key)})
        const model=findChild(page,"discoveryModel")
        const picker=findChild(page,"genreSources")
        tryVerify(() => model.requests.length > 0)
        compare(JSON.parse(page.collectionKey).albumScope,true)
        model.document=JSON.stringify({items:[],total:0,excluded_sources:["plugin:other"]})
        compare(picker.excluded,["plugin:other"])
        page.open({kind:"genre",name:"Child style"})
        compare(JSON.parse(Navigation.went[1]).excluded_sources,["plugin:other"])
        picker.chosen(["local:default"])
        compare(JSON.parse(page.collectionKey).albumScope,false)
        compare(JSON.parse(page.collectionKey).genreToken,"opaque")
        model.document=JSON.stringify({items:[],total:1,excluded_sources:["plugin:other"]})
        compare(picker.excluded,["local:default"])
        page.destroy();wait(1)
        const restored=createTemporaryObject(maker,this,{routeKey:JSON.stringify(key)})
        compare(JSON.parse(restored.collectionKey).albumScope,false)
        compare(findChild(restored,"genreSources").excluded,["local:default"])
    }
    function test_an_album_genre_shows_the_first_response_without_a_filter_change_data() {
        return [
            {tag:"aor", name:"AOR", albums:0, tagText:"aor"},
            {tag:"heavy metal", name:"Heavy Metal", albums:3, tagText:"Heavy Metal"}
        ]
    }

    function test_an_album_genre_shows_the_first_response_without_a_filter_change(data) {
        const route = JSON.stringify({kind:"genre", name:data.tagText, album:17, genreToken:"opaque", albumScope:true})
        const page = createTemporaryObject(maker, this, {routeKey:route})
        const model = findChild(page,"discoveryModel")
        tryVerify(() => model.requests.length > 0)
        const key = page.collectionKey
        const identity = page.queryIdentity
        verify(findChild(page,"discoveryLoading").visible)
        model.loading = true
        model.document = JSON.stringify({items:[], total:0, summary:{albums:data.albums, images:[]},
            excluded_sources:["plugin:remote"], breadcrumb:[data.name]})
        model.loading = false
        tryVerify(() => findChild(page,"discoveryContent").visible, 500)
        verify(!findChild(page,"discoveryLoading").visible)
        compare(page.collectionKey, key)
        compare(page.queryIdentity, identity)
        compare(findChild(page,"genreSources").excluded, ["plugin:remote"])
        compare(findChild(page,"genrePlay").enabled, data.albums > 0)
    }

    function test_a_shortened_genre_entry_keeps_its_opaque_identity_and_source_scope() {
        const page=createTemporaryObject(maker,this,{routeKey:JSON.stringify({kind:"genre",name:"Rock",excluded_sources:["plugin:other"]})})
        const key=JSON.stringify({kind:"genre",name:"Long genre…",genreId:42,genreToken:"opaque",album:17,albumScope:true})
        page.open({kind:"genre",name:"Long genre…",key:key})
        const selected=JSON.parse(Navigation.went[1])
        compare(selected.genreId,42);compare(selected.genreToken,"opaque")
        compare(selected.excluded_sources,["plugin:other"])
        compare(selected.albumScope,false)
        compare(selected.trail,["Long genre…"])
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
        const picture = findChild(tile, "discoveryGenrePortrait")
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
    function test_genre_image_actions_keep_the_opaque_genre_key() {
        const route=JSON.stringify({kind:"genre",name:"heavy-metal",identity:"opaque:genre"})
        const page=createTemporaryObject(maker,this,{routeKey:route})
        const key=page.collectionKey
        const model=findChild(page,"discoveryModel")
        model.document=JSON.stringify({items:[],total:0,summary:{albums:0,images:[]}})
        let choose
        tryVerify(()=>{choose=findChild(page,"genreChooseImage");return !!choose})
        Artwork.asked=[]
        const hero=findChild(page,"genreHero")
        compare(choose.parent,hero)
        verify(choose.x > hero.width / 2 && choose.y < hero.height / 2,"the edit control belongs to the hero's top right")
        choose.clicked()
        compare(Artwork.asked,[],"choose the artwork role before opening a file picker")
        const editor=findChild(page,"genreArtworkEditor")
        tryCompare(editor,"opened",true)
        tryVerify(()=>!!findChild(editor.contentItem,"genreCardChoose"))
        findChild(editor.contentItem,"genreCardChoose").clicked()
        findChild(editor.contentItem,"genreHeaderChoose").clicked()
        compare(Artwork.asked,["choose genre " + key,"choose genre header " + key])
        const chosen={},headers={}
        chosen[key]="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='8' height='8'/%3E"
        headers[key]="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='16' height='4'/%3E"
        Artwork.genres=chosen;Artwork.genreHeaders=headers;Artwork.rev++
        tryCompare(findChild(editor.contentItem,"genreCardForget"),"visible",true)
        tryCompare(findChild(editor.contentItem,"genreHeaderForget"),"visible",true)
        compare(findChild(editor.contentItem,"genreCardPreview").source.toString(),chosen[key])
        compare(findChild(editor.contentItem,"genreHeaderPreview").source.toString(),headers[key])
        compare(findChild(page,"discoveryContent").item.chosenImage.toString(),headers[key])
        findChild(editor.contentItem,"genreCardForget").clicked()
        findChild(editor.contentItem,"genreHeaderForget").clicked()
        compare(Artwork.asked.slice(2),["forget genre " + key,"forget genre header " + key])
        editor.close()
        tryVerify(()=>!findChild(page,"genreArtworkEditor"),5000,"closing releases the selector and both previews")
        compare(page.routeKey,route)
        Artwork.genres=({});Artwork.genreHeaders=({});Artwork.rev++
    }
    function genreTracks(name) {
        const page = createTemporaryObject(maker, this, {routeKey:JSON.stringify({kind:"genre",name:name})})
        findChild(page, "discoveryModel").document = JSON.stringify({items:[],total:0,summary:{albums:2,images:[]}})
        page.changeTab(3)
        let list
        tryVerify(() => { list = findChild(page, "discoveryTrackList"); return !!list })
        return {page:page, list:list, section:findChild(page, "discoveryContent").item}
    }
    function test_the_content_ends_at_the_plane_unless_the_player_floats() {
        const page = createTemporaryObject(maker, this, {mode:"genres"})
        const content = findChild(page, "discoveryContent")
        Metrics.playerSpace = 0
        compare(content.anchors.bottomMargin, 0, "a docked player leaves no band under the content")
        compare(content.y + content.height, page.height)
        Metrics.playerSpace = Metrics.playerHeight
        compare(content.anchors.bottomMargin, Metrics.floatingPlayerClearance, "a floating player keeps its room")
    }
    function test_discovery_scrollbars_and_their_hover_margin_belong_to_the_page_data() {
        return [{tag:"genres",mode:"genres",name:"discoveryScrollbar"},{tag:"compositions",mode:"compositions",name:"discoveryScrollbar"}]
    }
    function test_discovery_scrollbars_and_their_hover_margin_belong_to_the_page(data) {
        const page = createTemporaryObject(maker, this, {mode:data.mode})
        findChild(page, "discoveryModel").document = JSON.stringify({total:100,items:Array.from({length:100},(_,index)=>({kind:data.mode === "compositions" ? "work" : "genre",name:"Entry " + index,albums:1,tracks:1}))})
        let bar
        tryVerify(() => { bar = findChild(page, data.name); return !!bar && bar.parent === page })
        compare(bar.overlayParent, page)
        compare(bar.area.parent, page, "the hover bridge outside the list is the page's, or hover never reaches it")
    }
    function test_genre_tracks_put_their_scrollbar_on_the_page() {
        const genre = genreTracks("Asian")
        const bar = findChild(genre.list, "trackListScrollbar")
        tryCompare(bar, "parent", genre.page)
        compare(bar.area.parent, genre.page)
    }
    function test_genre_tracks_filter_from_their_column_headers() {
        const genre = genreTracks("Asian")
        const head = findChild(genre.section, "discoveryTrackHeader")
        verify(head && head.visible, "the genre's tracks have Tracks' filtering headers")
        compare(genre.list.header, false, "the list's own header gives way to them")
        verify(findChild(head, "tracksColumn-artist").visible)
        const key = genre.list.model.key
        compare(key, genre.page.collectionKey, "no filter keeps the plain selection")
        const columnFilter = findChild(genre.section, "tracksColumnFilter")
        verify(columnFilter)
        columnFilter.applied("artist", ["Mari Iijima"], ({}))
        tryVerify(() => genre.list.model.key !== key)
        const parts = genre.list.model.key.split("\u001e")
        compare(parts[0], genre.page.collectionKey)
        compare(JSON.parse(parts[1].split("\u001f")[4]), {artist:["Mari Iijima"]})
        verify(findChild(genre.section, "discoveryFilterTrail").visible, "the chips say what filters the list")
        verify(genre.section.sheetFiltered)
        findChild(genre.section, "discoveryFilterTrail").cleared()
        tryCompare(genre.list.model, "key", key)
    }
    function test_a_recording_keeps_the_plain_track_list() {
        const page = createTemporaryObject(maker, this, {mode:"compositions",routeKey:JSON.stringify({kind:"work",name:"Suite",work:"work:3",recording:"first"})})
        let list
        tryVerify(() => { list = findChild(page, "discoveryTrackList"); return !!list })
        verify(!findChild(page, "discoveryTrackHeader").visible)
        compare(list.header, true)
    }
    function test_discovery_scrollbars_never_overlap_content_data() {
        return [{tag:"genres",mode:"genres"},{tag:"composers",mode:"composers"},{tag:"compositions",mode:"compositions"}]
    }
    function test_discovery_scrollbars_never_overlap_content(data) {
        const page=createTemporaryObject(maker,this,{mode:data.mode,width:560})
        const model=findChild(page,"discoveryModel")
        model.document=JSON.stringify({total:100,items:Array.from({length:100},(_,index)=>({kind:data.mode === "compositions" ? "work" : data.mode === "composers" ? "composer" : "genre",name:"Entry " + index,albums:1,tracks:1}))})
        const view=findChild(page,"discoveryEntries")
        let bar
        tryVerify(()=>{bar=findChild(page,"discoveryScrollbar");return !!bar && bar.viewport===view})
        tryVerify(()=>bar.mapToItem(page,0,0).x >= view.mapToItem(page,view.width,0).x + 5)
        verify(bar.mapToItem(page,bar.width,0).x <= page.width,"the entire scrollbar stays within the page")
        page.width=1100
        tryVerify(()=>bar.mapToItem(page,0,0).x >= view.mapToItem(page,view.width,0).x + 5)
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
    function test_recordings_show_stripes_metadata_and_keep_the_selected_identity() {
        const page = createTemporaryObject(maker, this, {width:560,mode:"compositions",routeKey:JSON.stringify({
            kind:"work",name:"Suite",work:"work:3",composers:["Writer"]})})
        findChild(page, "discoveryModel").document = JSON.stringify({total:2,items:[
            {kind:"recording",name:"First record",artist:"Quartet",recording:"first",source:"Fixture",year:1987,tracks:3,duration:125000,album:-7},
            {kind:"recording",name:"Second record",artist:"Pianist",recording:"second",source:"Local",tracks:2,duration:4000000,album:3}
        ]})
        const view = findChild(page, "discoveryEntries")
        tryVerify(() => !!view.itemAtIndex(1))
        const first = view.itemAtIndex(0)
        const second = view.itemAtIndex(1)
        compare(findChild(first, "discoveryWorkDetails").text, "Quartet · 1987 · Fixture · 2:05 · 3 parts")
        verify(findChild(first, "discoveryWorkStripe").color !== findChild(second, "discoveryWorkStripe").color)
        verify(findChild(first, "discoveryWorkArt"))
        compare(findChild(second, "discoveryWorkDetails").text, "Pianist · Local · 1:06:40 · 2 parts")
        page.width = 1100
        tryCompare(findChild(first, "discoveryWorkDetails"), "text", "1987 · Fixture")
        first.forceActiveFocus()
        keyClick(Qt.Key_Return)
        const selected = JSON.parse(Navigation.went[1])
        compare(selected.work, "work:3")
        compare(selected.recording, "first")
        compare(selected.album, -7)
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
