import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import app.chilpa.pages
import "../../qml/Screens"
import "../../qml/Components"
import "../../qml/Patterns"
import "../../qml" as PageHosts
import "qrc:/qt/qml/app/chilpa/kit/qml/Components" as KitUi
import "qrc:/qt/qml/app/chilpa/kit/qml/Patterns" as KitPatterns
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives" as KitPrimitives

TestCase {
    id: test
    name: "NativeCollections"
    when: windowShown
    width: 1000; height: 800
    visible: true
    Component { id: collectionFactory; SourceCollectionScreen { width: test.width; height: test.height; source: "fixture native:saved:all" } }
    Component { id: controlsFactory; CollectionControls { width: test.width; source: "fixture native:saved:all" } }
    Component { id: hostFactory; PageHosts.NativeSourceHost { width:test.width; height:test.height; source:"fixture native:saved:all" } }
    Component { id: tileFactory; CollectionTile { width:200; height:250; plugin:"fixture"; entry:({rowToken:"row:1",kind:"track",ref:"track:1",title:"Song"}) } }
    Component {
        id: scrollHostFactory
        Item {
            width:test.width;height:test.height
            property alias page:nativeHost.item
            PageHosts.NativeSourceHost { id:nativeHost;anchors.fill:parent;source:"fixture native:saved:all";presentation:({layout:"collection"}) }
            KitUi.ScrollMemory { viewport:nativeHost.scrollView;key:"fixture-native-scroll" }
        }
    }
    Component {
        id: routedHostFactory
        KitPatterns.PageTransition {
            width:test.width;height:test.height
            route:"fixture\u001fsource\u001f0\u001fnative-collection-router-scroll"
            sourceComponent: Component { PageHosts.NativeSourceHost { source:"fixture native:saved:all";presentation:({layout:"collection"}) } }
        }
    }
    Component { id: treeFactory; SourceNavigationTree { width:240; pluginId:"fixture"; controller:treeController; entries:[{id:"view:library",title:"Collection",route:"fixture native:saved:all",children:[{id:"view:library/artists",title:"Artists",route:"fixture native:saved:artists"}]}] } }
    QtObject { id:treeController; property var expandedSources:({}); property var sourceView:({}) }
    SignalSpy { id: menus; signalName:"menuRequested" }
    SignalSpy { id: reads; target: Sources; signalName: "pageRequested" }
    SignalSpy { id: scrollPointers; signalName:"scrollViewChanged" }
    function document(extra) {
        return JSON.stringify(Object.assign({layout:"collection",plugin:"fixture",title:"Collection",items:[
            {rowToken:"row:0",kind:"album",ref:"album:1",title:"Record",_host:{card_id:-1,track_row:-1}},
            {rowToken:"row:1",kind:"track",ref:"track:1",title:"First song",_host:{track_row:0}},
            {rowToken:"row:2",kind:"artist",ref:"artist:1",title:"Artist",_host:{card_id:-2,track_row:-1}},
            {rowToken:"row:3",kind:"track",ref:"track:2",title:"Second song",_host:{track_row:1}}
        ]},extra || {}))
    }
    function init() {
        failOnWarning(/Binding loop detected|Cannot assign|is not a function|ReferenceError|TypeError/)
        Sources.pages = {"fixture native:saved:all":document()}
        Sources.revision++
        NativePages.requests=[]; NativePages.revalidations=[]
        Navigation.went=[]; SettingsModel.preferences={};treeController.expandedSources={};Sources.asked=[]
    }
    function test_hidden_dispatcher_uses_the_initial_snapshot_and_keeps_outgoing_layout() {
        const host=createTemporaryObject(hostFactory,test,{visible:false,presentation:{layout:"collection"}})
        verify(host);compare(host.about.layout,"collection")
        verify(host.item && host.item.refreshCollection !== undefined)
        reads.clear();Library.remoteRev++;wait(30);compare(reads.count,0)
        host.visible=true;wait(30);reads.clear()
        Library.remoteRev++;wait(30);compare(reads.count,0,"the collection owns its window instead of rereading the catalog document")
        host.leaving=true;host.presentation={layout:"mixed"}
        compare(host.about.layout,"collection")
        verify(host.item && host.item.refreshCollection !== undefined)
    }
    function test_context_menu_is_available_without_album_overlay() {
        const tile=createTemporaryObject(tileFactory,test)
        menus.target=tile;menus.clear()
        mouseClick(tile,50,50,Qt.RightButton)
        compare(menus.count,1)
        menus.target=null
    }
    function test_an_article_opens_its_typed_route_without_a_music_lease() {
        const page=createTemporaryObject(collectionFactory,test)
        const window=findChild(page,"nativeCollectionModel")
        window.resolved={"row:0":{kind:"article",navigationKey:"fixture native:typed-article"}}
        page.openItem({rowToken:"row:0",kind:"article"})
        tryCompare(Navigation,"went",["source","fixture native:typed-article"])
        compare(page.pendingAction,null)
    }
    function test_navigation_parent_expands_and_children_use_their_routes() {
        const tree=createTemporaryObject(treeFactory,test)
        const parent=findChild(tree,"sourceChild")
        verify(parent)
        mouseClick(parent)
        compare(Navigation.went,["source","fixture native:saved:all"])
        tryVerify(()=>treeController.expandedSources["fixture/view:library"]===true)
        function child(node) {
            if(node.objectName==="sourceChild"&&node.text==="Artists")return node
            for(const entry of node.children||[]){const found=child(entry);if(found)return found}
            return null
        }
        tryVerify(()=>child(tree)!==null)
        tryVerify(()=>child(tree).mapToItem(test,0,0).y>=parent.height)
        mouseClick(child(tree),child(tree).width/2,child(tree).height/2)
        compare(Navigation.went,["source","fixture native:saved:artists"])
        Navigation.routeKey="fixture native:saved:artists:filtered"
        verify(child(tree).active)
    }
    function test_mixed_positions_play_the_corresponding_track() {
        const page=createTemporaryObject(collectionFactory,test)
        verify(page)
        const model=findChild(page,"collectionTracks")
        verify(model)
        const items=JSON.parse(document()).items
        page.openItem(items[3])
        tryCompare(model,"lastAction","play");compare(model.lastRow,1)
        model.lastAction=""
        page.openItem(Object.assign({},items[3],{available:false}))
        wait(30);compare(model.lastAction,"")
        model.ready=false;page.openItem(items[1]);wait(30);compare(model.lastAction,"")
        model.ready=true;tryCompare(model,"lastAction","play");compare(model.lastRow,0)
        page.openItem(items[0]);tryCompare(Navigation,"selectedId",-1)
        page.openItem(items[2]);tryCompare(Navigation,"went",["source","fixture native:artist:1"])
    }

    function test_queries_keep_filters_and_order_and_drop_exclusions() {
        const controls=createTemporaryObject(controlsFactory,test,{about:{base_key:"fixture native:saved:all",filters:{hide_purchases:true,text:"kept"},order:"title",filter_controls:[]}})
        controls.applyField("membership","purchased",["hide_purchases"])
        compare(NativePages.requests[0],["fixture native:saved:all",{filters:{hide_purchases:false,text:"kept",membership:"purchased"},order:"title"}])
        compare(Navigation.went,["source","fixture native:saved:all:filtered"])
        compare(SettingsModel.kept("ui.source.collection.fixture native:saved:all"),Navigation.went[1])
    }
    function test_initial_sync_polls_but_hidden_pages_do_no_work() {
        Sources.pages={"fixture native:saved:all":document({loading:true,updating:true,refresh_after_ms:250})}
        const page=createTemporaryObject(collectionFactory,test)
        tryVerify(()=>NativePages.revalidations.length>0)
        page.visible=false;wait(30)
        reads.clear();NativePages.revalidations=[]
        Sources.revision++;Library.remoteRev++;wait(350)
        compare(reads.count,0);compare(NativePages.revalidations.length,0)
        page.visible=true;tryVerify(()=>reads.count>0)
    }
    function test_pending_requests_do_not_queue_more_polls() {
        Sources.pages={"fixture native:saved:all":document({loading:true,updating:true,request_pending:true,refresh_after_ms:250})}
        const page=createTemporaryObject(collectionFactory,test)
        wait(350);compare(NativePages.revalidations.length,0)
    }
    function test_a_new_collection_shows_its_header_and_filters_above_the_first_row() {
        Sources.pages={"fixture native:saved:all":document({knownCount:100,filters:{membership:"all"},filter_controls:[
            {key:"membership",type:"choice",label:"Membership",options:[{value:"all",label:"All"},{value:"saved",label:"Saved"}]}
        ]})}
        const router=createTemporaryObject(routedHostFactory,test,{route:"fixture\u001fsource\u001f0\u001ffresh-collection-header"})
        tryCompare(router,"phase","content")
        tryVerify(()=>router.item?.item?.count === 100)
        const page=router.item.item
        tryCompare(page,"restorePending",false)
        wait(80)
        verify(page.headerItem.mapToItem(page,0,0).y>=0,"a fresh collection must show its title and filters before its first row")
    }
    function test_a_saved_offset_in_the_first_row_survives_collection_activation() {
        Sources.pages={"fixture native:saved:all":document({knownCount:100})}
        const route="fixture\u001fsource\u001f0\u001ffirst-row-saved-offset"
        KitPrimitives.Visit.keep(route,40)
        const router=createTemporaryObject(routedHostFactory,test,{route:route})
        tryCompare(router,"phase","content")
        tryVerify(()=>router.item?.item?.count === 100)
        const page=router.item.item
        wait(80)
        verify(Math.abs(page.contentY-page.originY+page.topMargin-40)<1,"restoring the first row must preserve the router's saved pixel offset")
    }
    function test_empty_collections_keep_their_header_and_filters_visible() {
        Sources.pages={"fixture native:saved:all":document({knownCount:0,items:[],filters:{text:"no match"},filter_controls:[{key:"text",type:"search",label:"Search"}]})}
        const router=createTemporaryObject(routedHostFactory,test,{route:"fixture\u001fsource\u001f0\u001fempty-collection-header"})
        tryCompare(router,"phase","content")
        tryVerify(()=>router.item?.item !== null)
        const page=router.item.item
        wait(80)
        compare(page.count,0)
        verify(page.headerItem.mapToItem(page,0,0).y>=0)
    }
    function test_million_row_grid_creates_only_viewport_delegates() {
        Sources.pages={"fixture native:saved:all":document({knownCount:1000000})}
        const page=createTemporaryObject(collectionFactory,test)
        tryCompare(page,"count",1000000)
        compare(findChild(page,"collectionNextPage"),null)
        function cells(item) { let count=item.entry !== undefined && item.inViewport !== undefined ? 1 : 0;for (const child of item.children || []) count+=cells(child);return count }
        wait(80);verify(cells(page)<100)
        page.positionViewAtIndex(900000,GridView.Beginning);wait(80)
        verify(cells(page)<100)
        const window=findChild(page,"nativeCollectionModel")
        verify(window.calls.some(call=>call[0] === "seek" && call[1]>800000))
    }
    function test_old_page_action_uses_its_frozen_track_page_after_eviction() {
        Sources.pages={"fixture native:saved:all":document({knownCount:1000})}
        const page=createTemporaryObject(collectionFactory,test)
        const window=findChild(page,"nativeCollectionModel"),tracks=findChild(page,"collectionTracks")
        const old=JSON.parse(window.entry(0,window.revision))
        window.seek(800,899)
        compare(window.entry(0,window.revision),"{}")
        window.resolved={"row:0":{lease:7,key:"fixture original-page|tracks",kind:"track",trackRow:0}}
        tracks.ready=false;page.openItem(Object.assign({},old,{kind:"track"}))
        wait(30);compare(tracks.lastAction,"")
        tryCompare(tracks,"key","fixture original-page|tracks")
        tracks.ready=true;tryCompare(tracks,"lastAction","play");compare(tracks.lastRow,0)
        verify(window.calls.some(call=>call[0] === "action" && call[1] === "row:0"))
    }
    function test_leaving_retires_work_while_preserving_the_outgoing_projection() {
        const page=createTemporaryObject(collectionFactory,test)
        const window=findChild(page,"nativeCollectionModel")
        compare(page.count,4)
        page.leaving=true
        compare(window.key,"");compare(page.count,4)
        const calls=window.calls.length
        page.contentY=1000;Sources.revision++;Library.remoteRev++;wait(350)
        compare(window.calls.length,calls)
        page.visible=false;compare(page.count,0)
    }
    function test_return_restores_the_saved_window_without_reading_all_previous_rows() {
        Sources.pages={"fixture native:saved:all":document({knownCount:1000000,anchorIndex:800000})}
        const page=createTemporaryObject(collectionFactory,test)
        const window=findChild(page,"nativeCollectionModel")
        tryVerify(()=>window.calls.some(call=>call[0] === "seek" && call[1]>=799990))
        verify(window.calls.length<10)
        verify(page.contentY>1000000)
    }
    function test_a_failed_occurrence_action_does_not_use_the_previous_track_model() {
        const page=createTemporaryObject(collectionFactory,test)
        const window=findChild(page,"nativeCollectionModel"),tracks=findChild(page,"collectionTracks")
        window.resolved={"row:1":{failed:true,error:"The item changed."}}
        page.openItem(JSON.parse(document()).items[1])
        tryCompare(page,"actionError","The item changed.")
        compare(tracks.lastAction,"");compare(page.pendingAction,null)
    }
    function test_favorite_resolves_the_opaque_occurrence_before_sending_its_reference() {
        const page=createTemporaryObject(collectionFactory,test)
        const window=findChild(page,"nativeCollectionModel")
        window.resolved={"row:3":{lease:8,kind:"track",ref:"opaque:original-song"}}
        page.requestAction({rowToken:"row:3",kind:"track"},"favorite",0,0,true)
        tryVerify(()=>Sources.asked.length>0)
        compare(Sources.asked[0],["native","fixture",JSON.stringify({action:"favorite",ref:"opaque:original-song",on:true})])
        Sources.revision++;tryCompare(page,"favoriteAction",null)
        tryVerify(()=>window.calls.some(call=>call[0] === "refresh" && call[1] === false))
    }
    function test_multiple_query_changes_share_the_sidebar_preference_key() {
        const base="fixture native:saved:all"
        const controls=createTemporaryObject(controlsFactory,test,{source:base+":filtered",about:{base_key:base,filters:{text:"one"},order:"added"}})
        controls.navigate({filters:{text:"two"},order:"added"})
        controls.source=base+":another-query"
        controls.navigate({filters:{text:"two"},order:"title"})
        compare(Object.keys(SettingsModel.preferences),["ui.source.collection."+base])
        const tree=createTemporaryObject(treeFactory,test)
        mouseClick(findChild(tree,"sourceChild"))
        compare(Navigation.went,["source",SettingsModel.kept("ui.source.collection."+base)])
    }
    function test_background_revalidation_stays_quiet_and_explicit_refresh_waits_for_it() {
        const page=createTemporaryObject(collectionFactory,test)
        const window=findChild(page,"nativeCollectionModel")
        wait(50)
        const retained=page.itemAtIndex(0)
        window.simulateAsync=true
        page.refreshQuietly()
        compare(window.busy,true);compare(page.silentWork,true)
        compare(findChild(page,"collectionLoadingFeedback").visible,false)
        compare(findChild(page,"collectionPagingFeedback").visible,false)
        const refresh=findChild(page,"collectionRefresh")
        compare(refresh.enabled,true);compare(page.itemAtIndex(0),retained)
        refresh.clicked()
        compare(page.manualRefreshPending,true)
        compare(window.calls.filter(call=>call[0] === "refresh" && call[1] === true).length,0)
        window.busy=false
        tryVerify(()=>window.calls.some(call=>call[0] === "refresh" && call[1] === true))
        compare(window.calls.filter(call=>call[0] === "refresh" && call[1] === true).length,1)
        compare(page.silentWork,false);compare(findChild(page,"collectionLoadingFeedback").visible,true)
    }
    function test_collection_scroll_survives_the_router_scroll_memory() {
        Sources.pages={"fixture native:saved:all":document({knownCount:1000})}
        const wrapper=createTemporaryObject(scrollHostFactory,test)
        tryVerify(()=>wrapper.page && wrapper.page.count === 1000)
        wait(80)
        wrapper.page.positionViewAtEnd()
        wait(80)
        verify(wrapper.page.contentY>1000,"the router must not reset the requested collection position")
    }
    function test_collection_scroll_survives_the_complete_page_transition() {
        Sources.pages={"fixture native:saved:all":document({knownCount:1000})}
        const router=createTemporaryObject(routedHostFactory,test)
        tryCompare(router,"phase","content")
        tryVerify(()=>router.item?.item?.count === 1000)
        const page=router.item.item
        wait(80);page.positionViewAtEnd();wait(80)
        verify(page.contentY>1000,"the integrated page must keep the requested collection position")
    }
    function test_scrolling_does_not_publish_a_new_viewport_identity() {
        Sources.pages={"fixture native:saved:all":document({knownCount:1000})}
        const host=createTemporaryObject(hostFactory,test,{presentation:{layout:"collection"}})
        tryVerify(()=>host.item?.count === 1000)
        wait(80);scrollPointers.target=host;scrollPointers.clear()
        host.item.positionViewAtEnd();wait(80)
        compare(scrollPointers.count,0,"scrolling keeps the same viewport QObject")
        scrollPointers.target=null
    }
}
