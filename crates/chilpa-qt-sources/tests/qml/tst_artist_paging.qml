import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import "../../qml/Screens"
import "../../qml/Patterns"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

TestCase {
    id: test
    name: "ArtistPaging"
    when: windowShown
    visible: true
    width: 1400; height: 900
    readonly property string artistKey: "fixture native:artist:27"
    function document() {
        return {music:true, artist:true, header:{kind:"artist",title:"Band",plugin:"fixture",ref:"artist:27"}, shelves:[
            {id:"tracks",key:artistKey + "|tracks",role:"popular",kind:"track",title:"Popular Tracks",preview:5,items:[{ref:"track:1"}]},
            {id:"latest",key:artistKey + "|latest",role:"latest",kind:"album",title:"Latest release",tracks_key:artistKey + "|latest-tracks",items:[{ref:"album:9"}]},
            {id:"album",key:artistKey + "|album",role:"release",kind:"album",title:"Albums",more:"artist-more:27:albums:30",items:Array.from({length:30}, (_, i) => ({ref:"album:" + i}))}
        ]}
    }
    Component { id: pageMaker; ArtistPage { width: test.width; height: test.height; key: test.artistKey } }
    Component { id: tracksMaker; ArtistTracks { width: test.width; section: ({key:test.artistKey + "|tracks",preview:5,title:"Popular Tracks",role:"popular"}) } }
    Component { id: memoryMaker; ScrollMemory {} }
    function init() {
        failOnWarning(/Binding loop detected/)
        failOnWarning(/AlbumGridModel.art requires|Insufficient arguments/)
        Visit.kept = {}
        Sources.asked = []
        Navigation.page = ""
        Sources.pages = {[artistKey]:JSON.stringify(document())}
        Library.remoteRev++
    }
    function ready(page) {
        waitForRendering(page)
        tryVerify(() => page.blockOf(artistKey + "|tracks")?.item?.ready === true)
        tryVerify(() => page.blockOf(artistKey + "|album")?.item?.ready === true)
    }
    function test_latest_release_has_an_explicit_album_destination() {
        const page = createTemporaryObject(pageMaker, test)
        ready(page)
        tryVerify(() => findChild(page, "latestReleaseOpen") !== null)
        const button = findChild(page, "latestReleaseOpen")
        verify(button.visible)
        page.jump(artistKey + "|latest")
        waitForRendering(page)
        verify(findChild(page, "latestReleaseTracks").visible)
        mouseClick(button)
        compare(Navigation.page, "album")
        compare(Navigation.selectedId, -1)
    }
    function test_refresh_keeps_populated_blocks_mounted() {
        const page = createTemporaryObject(pageMaker, test)
        ready(page)
        const block = page.blockOf(artistKey + "|album")
        const retained = block.item
        const height = block.height
        const refreshed = document()
        refreshed.loading = true
        Sources.pages = {[artistKey]:JSON.stringify(refreshed)}
        Library.remoteRev++
        wait(40)
        compare(block.item, retained, "Revalidation must keep the displayed shelf alive")
        compare(block.height, height)
        refreshed.loading = false
        Sources.pages = {[artistKey]:JSON.stringify(refreshed)}
        Library.remoteRev++
        wait(40)
        compare(block.item, retained)
        compare(block.height, height)
    }
    function test_release_rows_reserve_their_height_before_the_viewport_mounts_them() {
        const page = createTemporaryObject(pageMaker, test, {visible:false})
        tryVerify(() => page.blockOf(artistKey + "|album") !== null)
        const block = page.blockOf(artistKey + "|album")
        const reserved = block.reservedHeight
        page.visible = true
        ready(page)
        verify(Math.abs(block.height - reserved) <= 1,
            "Mounting cached releases changed height from " + reserved + " to " + block.height)
    }
    function test_popular_expansion_returns_with_its_section() {
        const first = createTemporaryObject(tracksMaker, test)
        waitForRendering(first)
        const more = findChild(first, "tracksMore")
        mouseClick(more)
        verify(first.expanded)
        first.destroy()
        wait(0)
        const returned = createTemporaryObject(tracksMaker, test)
        waitForRendering(returned)
        verify(returned.expanded, "Back and forward must recover the loaded track count")
        returned.section = {key:"fixture native:artist:28|tracks",preview:5,title:"Other"}
        compare(returned.expanded, false, "Each artist keeps its own expansion")
    }
    function test_popular_remote_more_is_explicit_and_error_keeps_existing_rows() {
        const tracks = createTemporaryObject(tracksMaker, test)
        tracks.section = Object.assign({}, tracks.section, {more:"artist-more:27:top:8"})
        waitForRendering(tracks)
        const more = findChild(tracks, "tracksMore")
        mouseClick(more)
        compare(Sources.asked.length, 0, "The cached rows are revealed before requesting another page")
        mouseClick(more)
        compare(Sources.asked, [["section_more", artistKey + "|tracks", ""]])
        tracks.section = Object.assign({}, tracks.section, {loading:true})
        compare(more.enabled, false)
        tracks.section = Object.assign({}, tracks.section, {loading:false,error:"Try again"})
        verify(tracks.shown && tracks.expanded)
        verify(more.enabled)
    }
    function test_returned_page_restores_expansion_before_its_scroll_position() {
        const first = createTemporaryObject(pageMaker, test)
        ready(first)
        const memory = createTemporaryObject(memoryMaker, test, {viewport:first.scrollView,key:artistKey})
        tryCompare(memory, "pending", -1)
        const tracks = first.blockOf(artistKey + "|tracks").item
        mouseClick(findChild(tracks, "tracksMore"))
        verify(tracks.expanded)
        first.scrollView.contentY = 600
        memory.remember()
        compare(Visit.recall(artistKey, -1), 600)
        memory.enabled = false
        first.destroy()
        wait(0)
        const returned = createTemporaryObject(pageMaker, test)
        memory.viewport = returned.scrollView
        memory.enabled = true
        memory.restore()
        ready(returned)
        verify(returned.blockOf(artistKey + "|tracks").item.expanded)
        tryCompare(returned.scrollView, "contentY", 600)
        memory.enabled = false
    }
}
