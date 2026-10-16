import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../../qml/Patterns"
import "../../qml/Screens"

TestCase {
    name: "SourceReservations"
    when: windowShown
    visible: true
    width: 1000
    height: 800

    Component {
        id: slotComponent
        HomeSection {
            width: 900
            entry: ({id:"reservation-fixture", kind:"shelf", title:"Waiting"})
            content: Component {
                Item {
                    property bool ready: false
                    property bool shown: false
                    implicitHeight: 360
                }
            }
        }
    }
    Component { id: pageComponent; SourceHome { width: 900; height: 700; leaving: true } }

    function test_a_delayed_offscreen_completion_keeps_the_reserved_height() {
        const slot = createTemporaryObject(slotComponent, this, {y:1000})
        verify(slot.height > 0)
        const reserved = slot.height
        slot.started = true
        const loader = findChild(slot, "homeModuleLoader")
        tryVerify(() => loader.item !== null)
        compare(slot.height, reserved)
        verify(Visit.recall("home.module.reservation-fixture", null) === null)
        loader.item.shown = true
        loader.item.ready = true
        tryVerify(() => slot.ready)
        verify(slot.height >= reserved)
        slot.y = 0
        verify(slot.height > 0)
    }

    function test_a_completed_empty_shelf_releases_its_reservation() {
        const slot = createTemporaryObject(slotComponent, this, {entry:{id:"empty-fixture",kind:"shelf"}, started:true})
        const loader = findChild(slot, "homeModuleLoader")
        tryVerify(() => loader.item !== null)
        verify(slot.height > 0)
        loader.item.ready = true
        tryCompare(slot, "height", 0)
        tryVerify(() => Visit.recall("home.module.empty-fixture", null)?.height === 0)
    }

    function test_section_keys_keep_distinct_footprints_without_ids() {
        Sources.pages = {"fixture section:library":JSON.stringify({shelves:[
            {key:"fixture section:albums",title:"Albums"},
            {key:"fixture section:artists",title:"Artists"}
        ]})}
        const page = createTemporaryObject(pageComponent, this, {key:"fixture section:library"})
        tryCompare(page, "count", 2)
        tryVerify(() => page.itemAtIndex(0) !== null && page.itemAtIndex(1) !== null)
        const first = findChild(page.itemAtIndex(0), "homeModuleLoader").entry.id
        const second = findChild(page.itemAtIndex(1), "homeModuleLoader").entry.id
        compare(first, "native:fixture section:library:fixture section:albums")
        compare(second, "native:fixture section:library:fixture section:artists")
        verify(first !== second)
    }
}
