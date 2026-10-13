import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import "../../qml/Patterns"
import "../../qml/Screens"

TestCase {
    name: "SourceHeaders"; when:windowShown; visible:true; width:1100; height:800
    SourcePageHeader { id:header; width:parent.width; info:({kind:"album",title:"Album",quality_label:"WAV 24/96",quality_tier:3}) }
    SourceShelf { id:shelf; y:header.height; width:parent.width; wrap:true; section:({title:"Releases",key:"fixture native:artist:1|albums",more:"page:2",error:"Loading"}) }
    function init() { header.info = {kind:"album",title:"Album",quality_label:"WAV 24/96",quality_tier:3} }
    function findText(item,text) {
        if(item.text===text) return item
        for(const child of item.children || []) { const found=findText(child,text); if(found) return found }
        return null
    }
    function test_radio_is_a_header_action_for_albums_and_artists() {
        const button = findChild(header, "radioAction")
        header.info = {kind:"album",title:"Album"}
        verify(!button.visible)
        header.info = {kind:"album",title:"Album",radio_id:-42}
        verify(button.visible)
        compare(button.tip, "Album radio")
        waitForRendering(header)
        mouseClick(button)
        compare(AlbumActions.playedId, -42)
        header.info = {kind:"artist",title:"Artist",radio_id:-43}
        compare(button.tip, "Artist radio")
        waitForRendering(header)
        mouseClick(button)
        compare(AlbumActions.playedId, -43)
        header.info = {kind:"album",title:"Album",quality_label:"WAV 24/96",quality_tier:3}
    }
    function test_technical_quality_is_visible_before_playback() {
        const label=findText(header,"WAV 24/96")
        verify(label); verify(label.visible)
        header.info=Object.assign({},header.info,{quality_label:"WAV 16/44.1",quality_tier:2})
        verify(findText(header,"WAV 16/44.1").visible)
    }
    function test_more_is_below_the_content_and_centered() {
        const button=findText(shelf,"Load more")
        verify(button)
        compare(Math.round(button.x+button.width/2),Math.round(shelf.width/2))
        verify(button.y>findText(shelf,"Loading").y)
        button.clicked()
        compare(Sources.asked[Sources.asked.length-1],["section_more",shelf.activeKey,""])
    }
    // While more is on its way the button says so and takes no click.
    function test_more_on_its_way_says_so() {
        const was = shelf.section
        shelf.section = Object.assign({}, was, { loading: true })
        const button = findText(shelf, "Loading…")
        verify(button && button.visible)
        verify(!button.enabled)
        shelf.section = was
        verify(findText(shelf, "Load more").enabled)
    }
    // A page read again with the same shelves keeps each shelf's item on
    // screen; only a shelf that comes or goes is built or dropped.
    Component { id: homeMaker; SourceHome { width: 1100; height: 600; key: "fixture native:artist:1" } }
    function shelvesOf(item) {
        let out = []
        for (const child of item.children || []) {
            if (child.hasOwnProperty("activeKey")) out.push(child)
            out = out.concat(shelvesOf(child))
        }
        return out
    }
    function test_a_page_read_again_keeps_its_shelves() {
        const page = (n) => JSON.stringify({ music: true, title: "Artist", header: { kind: "artist", title: "Artist" }, shelves: [
            { id: "album", key: "fixture native:artist:1|album", title: "Albums", kind: "album", items: Array.from({ length: n }, (_, i) => ({ ref: "album:" + i })), more: "x" },
            { id: "similar", key: "fixture native:artist:1|similar", title: "Similar", kind: "artist", items: [] }
        ] })
        Sources.pages = { "fixture native:artist:1": page(2) }
        // Made with its page already there: the shelves show without a change.
        const home = createTemporaryObject(homeMaker, this)
        waitForRendering(home)
        tryVerify(() => shelvesOf(home).length === 2)
        const before = shelvesOf(home)
        compare(before.length, 2)
        compare(before[0].section.items.length, 2)
        Sources.pages = { "fixture native:artist:1": page(4) }
        Library.remoteRev++
        tryVerify(() => shelvesOf(home)[0].section.items.length === 4)
        const after = shelvesOf(home)
        compare(after.length, 2)
        verify(after[0] === before[0] && after[1] === before[1], "the same shelves stand")
        compare(after[0].section.items.length, 4, "with what came")
    }
}
