import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import "../../qml/Screens"
import "../../qml" as PageHosts
import "qrc:/qt/qml/app/chilpa/kit/qml/Patterns" as KitPatterns
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives" as KitPrimitives

TestCase {
    id: test
    name: "CollectionScroll"
    when: windowShown
    width: 1000; height: 800
    visible: true
    readonly property string source: "fixture native:saved:all"
    Component {
        id: routerFactory
        KitPatterns.PageTransition {
            width: test.width; height: test.height
            sourceComponent: screen === "source" ? collection : emptyPage
            Component { id:collection; PageHosts.NativeSourceHost { source:test.source; presentation:({layout:"collection"}); scrollMemory:parent?.scroll || null } }
            Component { id:emptyPage; Item {} }
        }
    }
    Component { id: pageFactory; SourceCollectionScreen { width:test.width; height:test.height; source:test.source } }
    function init() {
        failOnWarning(/Binding loop detected|Cannot assign|is not a function|ReferenceError|TypeError/)
        KitPrimitives.Visit.kept = ({})
        Sources.pages = ({})
        Sources.pages[source] = JSON.stringify({layout:"collection", title:"Collection", knownCount:1000, anchorIndex:200, items:[]})
        Sources.revision++
    }
    function offset(page) {
        const view=page.scrollView
        return view.contentY-view.originY+view.topMargin
    }
    function test_pixel_memory_wins_over_a_stale_collection_anchor_data() {
        return [{tag:"first-row", saved:40}, {tag:"scrolled", saved:400}, {tag:"top", saved:0}]
    }
    function test_pixel_memory_wins_over_a_stale_collection_anchor(data) {
        const route="fixture\u001fsource\u001f0\u001fscroll-"+data.tag
        KitPrimitives.Visit.keep(route,data.saved)
        const router=createTemporaryObject(routerFactory,test,{route:route})
        tryCompare(router,"phase","content")
        tryVerify(()=>router.item?.item?.scrollView.count > 0)
        const page=router.item.item
        tryCompare(page,"restorePending",false)
        wait(100)
        verify(Math.abs(offset(page)-data.saved)<1,"saved pixels must take precedence over the last fetched row: "+offset(page))
        const model=findChild(page,"nativeCollectionModel")
        verify(!model.calls.some(call=>call[0]==="seek" && call[1]>=200),"restoration must not fetch the stale distant window")
    }
    function test_back_restores_pixels_after_the_distant_window_was_cached() {
        const route="fixture\u001fsource\u001f0\u001fscroll-return"
        const away="fixture\u001fhome\u001f0\u001f"
        const router=createTemporaryObject(routerFactory,test,{route:route})
        tryCompare(router,"phase","content")
        tryVerify(()=>router.item?.item?.scrollView.count > 0)
        const view=router.item.item.scrollView
        view.contentY=view.originY-view.topMargin+400
        wait(50)
        router.route=away
        tryCompare(router,"committedRoute",away)
        tryCompare(router,"phase","content")
        router.route=route
        tryCompare(router,"committedRoute",route)
        tryCompare(router,"phase","content")
        tryVerify(()=>router.item?.item?.scrollView.count > 0)
        wait(100)
        verify(Math.abs(offset(router.item.item)-400)<1,"returning must retain pixels after recreating the collection")
    }
    function test_unmanaged_collection_keeps_its_bounded_anchor_fallback() {
        Sources.pages[source]=JSON.stringify({layout:"collection",title:"Collection",knownCount:1000000,anchorIndex:800000,items:[]})
        const page=createTemporaryObject(pageFactory,test)
        const model=findChild(page,"nativeCollectionModel")
        tryVerify(()=>model.calls.some(call=>call[0]==="seek" && call[1]>=799990))
        verify(model.calls.length<10,"restoring a distant anchor must not fetch all earlier rows")
        verify(page.scrollView.contentY>1000000)
    }
}
