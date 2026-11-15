import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import app.chilpa.pages
import "../../qml/Screens"
import "../../qml/Components"
import "../../qml/Patterns"
import "../../qml" as PageHosts

TestCase {
    id: test
    name: "NativeCollections"
    when: windowShown
    width: 1000; height: 800
    visible: true
    Component { id: collectionFactory; SourceCollectionScreen { width: test.width; height: test.height; source: "fixture native:saved:all" } }
    Component { id: controlsFactory; CollectionControls { width: test.width; source: "fixture native:saved:all" } }
    Component { id: webFactory; SourceWebScreen { width: test.width; height: test.height; source: "fixture native:journal" } }
    Component { id: hostFactory; PageHosts.NativeSourceHost { width:test.width; height:test.height; source:"fixture native:saved:all" } }
    Component { id: tileFactory; CollectionTile { width:200; height:250; plugin:"fixture"; entry:({kind:"track",ref:"track:1",title:"Song"}) } }
    Component { id: treeFactory; SourceNavigationTree { width:240; pluginId:"fixture"; controller:treeController; entries:[{id:"view:library",title:"Collection",route:"fixture native:saved:all",children:[{id:"view:library/artists",title:"Artists",route:"fixture native:saved:artists"}]}] } }
    QtObject { id:treeController; property var expandedSources:({}); property var sourceView:({}) }
    SignalSpy { id: menus; signalName:"menuRequested" }
    SignalSpy { id: reads; target: Sources; signalName: "pageRequested" }
    function document(extra) {
        return JSON.stringify(Object.assign({layout:"collection",plugin:"fixture",title:"Collection",items:[
            {kind:"album",ref:"album:1",title:"Record",_host:{card_id:-1,track_row:-1}},
            {kind:"track",ref:"track:1",title:"First song",_host:{track_row:0}},
            {kind:"artist",ref:"artist:1",title:"Artist",_host:{card_id:-2,track_row:-1}},
            {kind:"track",ref:"track:2",title:"Second song",_host:{track_row:1}}
        ]},extra || {}))
    }
    function init() {
        failOnWarning(/Binding loop detected|Cannot assign|is not a function|ReferenceError|TypeError/)
        Sources.pages = {"fixture native:saved:all":document(),"fixture native:journal":JSON.stringify({layout:"web",plugin:"fixture",title:"Journal"})}
        Sources.revision++
        NativePages.requests=[]; NativePages.revalidations=[]; NativePages.allowWeb=false
        Navigation.went=[]; SettingsModel.preferences={};treeController.expandedSources={}
    }
    function test_hidden_dispatcher_uses_the_initial_snapshot_and_keeps_outgoing_layout() {
        const host=createTemporaryObject(hostFactory,test,{visible:false,presentation:{layout:"collection"}})
        verify(host);compare(host.about.layout,"collection")
        verify(host.item && host.item.refreshCollection !== undefined)
        reads.clear();Library.remoteRev++;wait(30);compare(reads.count,0)
        host.visible=true;wait(30);reads.clear()
        Library.remoteRev++;wait(30);compare(reads.count,1,"only the leaf reads its page")
        host.leaving=true;host.presentation={layout:"web"}
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
        model.append({rowKind:0,title:"First"});model.append({rowKind:0,title:"Second"})
        const items=JSON.parse(document()).items
        page.openItem(items[3])
        compare(model.lastAction,"play");compare(model.lastRow,1)
        model.lastAction=""
        page.openItem(Object.assign({},items[3],{available:false}))
        compare(model.lastAction,"")
        model.ready=false;page.openItem(items[1]);compare(model.lastAction,"")
        page.openItem(items[0]);compare(Navigation.selectedId,-1)
        page.openItem(items[2]);compare(Navigation.went,["source","fixture native:artist:1"])
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
    function test_next_page_uses_opaque_route_without_accumulating_rows() {
        Sources.pages={"fixture native:saved:all":document({next_key:"fixture native:opaque-cursor"})}
        const page=createTemporaryObject(collectionFactory,test)
        const next=findChild(page,"collectionNextPage")
        verify(next);next.clicked()
        compare(Navigation.went,["source","fixture native:opaque-cursor"])
        compare(page.count,4)
    }
    function test_web_module_stays_unloaded_when_not_permitted() {
        const page=createTemporaryObject(webFactory,test)
        verify(page)
        const loader=findChild(page,"embeddedPageLoader")
        verify(loader);compare(loader.active,false);compare(String(loader.source),"")
        page.visible=false;reads.clear();Sources.revision++;Library.remoteRev++;wait(50)
        compare(reads.count,0);compare(loader.item,null)
    }
}
