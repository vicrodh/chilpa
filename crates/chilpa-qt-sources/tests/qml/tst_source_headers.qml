import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
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
    // A record kept by the library and by sources offers its copies here too.
    function test_a_record_with_copies_offers_them() {
        const versions = findChild(header, "recordVersions")
        header.record = { title: "Album", art: "", selectedId: -7, versions: JSON.stringify([
            { id: -7, label: "Source · FLAC", brief: "Source", detail: "", source: { local: false } },
            { id: 9, label: "Local library · FLAC", brief: "Local", detail: "", source: { local: true } }]), selectVersion: function(value) {} }
        tryCompare(versions, "visible", true)
        header.cardId = 7
        const marks = findChild(header, "recordMarks")
        tryCompare(marks, "visible", true)
        tryVerify(() => marks.y === versions.y && marks.x < versions.x, 1000, "the heart and the stars stand with the round buttons, the copies last: " + marks.x + "," + marks.y + " " + versions.x + "," + versions.y)
        header.cardId = 0
        header.record = { title: "Album", art: "", selectedId: -7, versions: JSON.stringify([{ id: -7, label: "Source", brief: "", detail: "", source: {} }]), selectVersion: function(value) {} }
        tryCompare(versions, "visible", false, 1000, "one copy offers no choice")
        header.record = undefined
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
    Component { id: contextMaker; SourceShelf { width:1100; section:({id:"context",key:"fixture native:home|context",kind:"album",
        header:{kind:"album",ref:"album:seed",title:"Seed"},header_key:"fixture native:home|context-header"}) } }
    function test_recommendation_context_has_the_library_card_and_its_actions() {
        Sources.show({cards:{"-1":{plugin:"fixture",ref:"album:seed",favorite:false}}})
        SettingsModel.quickViewClick = false
        const module = createTemporaryObject(contextMaker, this)
        const context = findChild(module, "sourceContextCard")
        verify(context)
        tryVerify(() => findChild(context, "albumTitle") !== null)
        const card = findChild(context, "albumTitle").parent.parent
        compare(card.width, CardLayout.cardWidth)
        mouseClick(card, card.width / 2, card.width / 2)
        compare(Navigation.page, "album")
        compare(Navigation.selectedId, -1)
        mouseMove(card, card.width / 2, card.width / 2)
        tryVerify(() => findChild(card, "albumFavorite") !== null)
        const favorite = findChild(card, "albumFavorite")
        verify(favorite.visible)
        mouseClick(favorite)
        compare(Sources.asked[Sources.asked.length - 1], ["native", "fixture", JSON.stringify({action:"favorite",ref:"album:seed",on:true})])
        const quick = findChild(context, "albumQuickLoader")
        card.quick()
        tryCompare(quick, "active", true)
        compare(quick.width, context.width - Metrics.cardRing * 2)
    }
    // A page read again with the same shelves keeps each shelf's item on
    // screen; only a shelf that comes or goes is built or dropped.
    Component { id: homeMaker; SourceHome { width: 1100; height: 600; key: "fixture native:artist:1" } }
    Component { id: discoverMaker; SourceHome { width: 1100; height: 600; key: "fixture native:discover:home" } }
    function test_cached_discovery_modules_show_actual_cards() {
        Sources.pages = {"fixture native:discover:home": JSON.stringify({plugin:"fixture", tab:"home", title:"Source", shelves:[
            {id:"recent", kind:"album", title:"Recently played", key:"fixture native:discover:home|recent", items:[{ref:"album:1"}]},
            {id:"releases", kind:"album", title:"New releases", key:"fixture native:discover:home|releases", items:[{ref:"album:2"}]}
        ]})}
        const discover = createTemporaryObject(discoverMaker, this)
        tryVerify(() => shelvesOf(discover).some(shelf => shelf.activeKey === "fixture native:discover:home|recent" && shelf.ready && shelf.shown && shelf.visible
            && shelf.width > 0 && shelf.height > 0), 1000, "a cached provider module must reveal its cards")
    }
    Component {
        id: pendingMaker
        HomeSection {
            width: 1100; started: true
            entry: ({id:"pending",kind:"shelf",title:"Albums",key:"fixture native:pending|albums"})
            content: Component { SourceShelf { section: parent.entry; oneRow: true } }
        }
    }
    function test_a_pending_provider_keeps_its_skeleton_footprint() {
        Sources.pages = {
            "fixture native:pending": JSON.stringify({shelves:[{id:"albums",items:[]}]}),
            "fixture native:pending|albums": JSON.stringify({loading:true})
        }
        const module = createTemporaryObject(pendingMaker, this)
        wait(50)
        compare(module.ready, false, "an empty local query does not finish the provider request")
        const height = module.height
        verify(height >= CardLayout.cardHeight)
        Sources.pages = {
            "fixture native:pending": JSON.stringify({shelves:[{id:"albums",items:[{ref:"album:1"}]}]}),
            "fixture native:pending|albums": JSON.stringify({loading:false})
        }
        Library.remoteRev++
        tryCompare(module, "ready", true)
        verify(module.height >= height, "the first polished rail must not contract its placeholder")
    }
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
        tryVerify(() => shelvesOf(home).length === 2 && shelvesOf(home)[0].section.items?.length === 2)
        const before = shelvesOf(home)
        compare(before.length, 2)
        compare(before[0].section.items.length, 2)
        tryVerify(() => before[0].visible && before[0].height > 0 && before[0].width > 0,
            1000, "cached source albums must be visible, not only mounted under a skeleton")
        Sources.pages = { "fixture native:artist:1": page(4) }
        Library.remoteRev++
        tryVerify(() => shelvesOf(home)[0].section.items.length === 4)
        const after = shelvesOf(home)
        compare(after.length, 2)
        verify(after[0] === before[0] && after[1] === before[1], "the same shelves stand")
        compare(after[0].section.items.length, 4, "with what came")
    }
}
