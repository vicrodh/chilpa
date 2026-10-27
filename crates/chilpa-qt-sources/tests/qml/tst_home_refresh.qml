import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import "../../qml/Patterns"
import "../../qml/Screens"

TestCase {
    id: test
    name: "HomeRefresh"
    when: windowShown
    visible: true
    width: 1000; height: 800

    SignalSpy { id: reads; target: Sources; signalName: "pageRequested" }
    Component { id: slotFactory; HomeSection { width: test.width; entry: ({id:"fixture.module"}); started: true; content: body } }
    Component { id: body; Item { property bool ready: true; property bool shown: true; implicitHeight: 200 } }
    Component { id: homeFactory; SourceHome { width: test.width; height: test.height; key: "fixture native:discover:home" } }
    Component { id: integratedFactory; SourceHomeSections { width: test.width; rendered: false } }
    Component { id: shelfFactory; SourceShelf { width: test.width; oneRow: true; section: ({key:"fixture native:discover:home|recent",size:12}) } }

    function init() {
        failOnWarning(/Binding loop detected/)
        Sources.pages = {"fixture native:discover:home": JSON.stringify({shelves:[{
            id:"recent",title:"Records",kind:"album",size:12,key:"fixture native:discover:home|recent",
            items:Array.from({length:12}, (_, index) => ({kind:"album",ref:"album:" + index}))
        }]})}
        Sources.show({homeLibraries:[{id:"fixture",name:"Fixture",enabled:true}]})
        reads.clear()
    }

    function test_loaded_module_keeps_the_same_body_when_start_is_withdrawn() {
        const slot = createTemporaryObject(slotFactory, test)
        tryCompare(slot, "ready", true)
        const loader = findChild(slot, "homeModuleLoader")
        const item = loader.item
        slot.started = false
        wait(50)
        compare(loader.item, item)
        verify(loader.active && slot.ready)
        slot.started = true
        compare(loader.item, item)
    }

    function test_hidden_source_home_defers_reads_and_retains_the_document() {
        const page = createTemporaryObject(homeFactory, test)
        tryVerify(() => page.shelves.length === 1)
        const document = page.document
        page.visible = false
        wait(50)
        reads.clear()
        Library.remoteRev++
        wait(80)
        compare(reads.count, 0)
        compare(page.document, document)
        page.visible = true
        tryVerify(() => reads.count > 0)
        compare(page.document, document)
    }

    function test_hidden_shelf_freezes_revisions_until_reopened() {
        const shelf = createTemporaryObject(shelfFactory, test)
        tryCompare(shelf, "ready", true)
        shelf.active = false
        const revision = shelf.catalogRevision
        Library.remoteRev++
        Library.remoteArtRev++
        wait(50)
        compare(shelf.catalogRevision, revision)
        shelf.active = true
        tryCompare(shelf, "catalogRevision", Library.contentRev + Library.remoteRev + Library.remoteArtRev)
    }

    function test_inactive_and_disabled_sources_do_not_start_home_reads() {
        const home = createTemporaryObject(integratedFactory, test)
        tryVerify(() => home.ready && home.sections.length === 1)
        home.active = false
        wait(50)
        reads.clear()
        Library.remoteRev++
        home.refresh++
        wait(80)
        compare(reads.count, 0)
        Sources.show({homeLibraries:[{id:"fixture",name:"Fixture",enabled:false}]})
        home.active = true
        tryVerify(() => home.ready && home.sections.length === 1 && !home.sections[0].enabled)
        compare(reads.count, 0)
    }

    function test_equal_source_documents_preserve_delegates_and_scroll() {
        const page = createTemporaryObject(homeFactory, test)
        page.presented = true
        tryVerify(() => findChild(page, "sourceShelfRail") !== null)
        const rail = findChild(page, "sourceShelfRail")
        tryVerify(() => rail.contentWidth > rail.width)
        rail.contentX = 100
        const loader = findChild(page, "homeModuleLoader")
        tryCompare(loader, "status", Loader.Ready)
        const item = loader.item
        for (let cycle = 0; cycle < 3; cycle++) {
            Library.remoteRev++
            wait(50)
            compare(loader.item, item)
            compare(findChild(page, "sourceShelfRail"), rail)
            compare(rail.contentX, 100)
            verify(loader.visible)
        }
    }
}
