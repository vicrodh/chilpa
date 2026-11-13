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
        button.forceActiveFocus()
        keyClick(Qt.Key_Return)
        compare(Navigation.page, "album")
        compare(Navigation.selectedId, -7)
    }
    function test_only_the_selected_detail_view_is_mounted() {
        const page = createTemporaryObject(maker, this, {mode:"composers",routeKey:JSON.stringify({kind:"composer",name:"Writer"})})
        compare(findChild(page, "discoveryContent").item.toString().includes("DiscoveryIndex"), true)
        page.tab = 2
        tryVerify(() => findChild(page, "discoveryContent").item.toString().includes("DiscoveryTracks"))
    }
}
