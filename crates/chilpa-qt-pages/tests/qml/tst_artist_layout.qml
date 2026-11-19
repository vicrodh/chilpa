import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/sources/qml/Screens"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

TestCase {
    id: test
    name: "ArtistLayout"
    when: windowShown
    visible: true
    width: 1400; height: 900
    readonly property string artistKey: "fixture native:artist:27"
    Component { id: pageMaker; ArtistPage { width: test.width; height: test.height; key: test.artistKey } }
    Component { id: memoryMaker; ScrollMemory {} }
    function document() {
        return {artist:true, header:{kind:"artist",title:"Band",plugin:"fixture",ref:"artist:27"}, shelves:[
            {id:"tracks",key:artistKey + "|tracks",role:"popular",kind:"track",title:"Popular Tracks",items:[{ref:"track:1"}]},
            {id:"latest",key:artistKey + "|latest",role:"latest",kind:"album",title:"Latest release",tracks_key:artistKey + "|latest-tracks",items:[{ref:"album:9"}]},
            {id:"album",key:artistKey + "|album",role:"release",kind:"album",title:"Albums",items:Array.from({length:60}, (_, i) => ({ref:"album:" + i}))}
        ]}
    }
    function init() {
        failOnWarning(/Binding loop detected/)
        Visit.kept = {}
        SettingsModel.keep("ui.artist.network", "false")
        Sources.pages = {[artistKey]:JSON.stringify(document())}
        Library.remoteRev++
    }
    function ready(page) {
        waitForRendering(page)
        tryVerify(() => page.blockOf(artistKey + "|album")?.ready === true)
        tryVerify(() => page.blockOf(artistKey + "|latest")?.ready === true)
        waitForRendering(page)
    }
    function test_delayed_preview_keeps_the_visible_release_section_in_place_data() {
        return [
            {tag:"compact", density:"compact", width:1400},
            {tag:"rich", density:"rich", width:1400},
            {tag:"stacked", density:"rich", width:440}
        ]
    }
    function test_delayed_preview_keeps_the_visible_release_section_in_place(data) {
        SettingsModel.density = data.density
        const page = createTemporaryObject(pageMaker, test, {width:data.width})
        ready(page)
        const latest = page.blockOf(artistKey + "|latest")
        const albums = page.blockOf(artistKey + "|album")
        const tracks = findChild(latest, "latestReleaseTracks").model
        const rows = []
        for (let i = 0; i < tracks.count; i++) rows.push(Object.assign({}, tracks.get(i)))
        tracks.ready = false
        tracks.clear()
        // As on a first visit: the preview has not been drawn yet.
        latest.settled = false
        waitForRendering(page)
        page.jump(artistKey + "|album")
        waitForRendering(page)
        const before = albums.mapToItem(page, 0, 0).y
        const previewHeight = latest.height
        // The model publishes the delayed response on a later event turn.
        const arrival = createTemporaryObject(arrivalMaker, test, {model:tracks, rows:rows})
        arrival.start()
        tryCompare(tracks, "ready", true)
        waitForRendering(page)
        verify(latest.height > previewHeight, "the real asynchronous preview changed the preceding section's height")
        verify(Math.abs(albums.mapToItem(page, 0, 0).y - before) <= 1,
            "Albums moved after a preceding preview settled: " + before + " -> " + albums.mapToItem(page, 0, 0).y)
    }
    function test_a_refresh_of_a_shown_list_keeps_every_section_in_place_data() {
        return [{tag:"top", jump:false}, {tag:"after_jump", jump:true}]
    }
    function test_a_refresh_of_a_shown_list_keeps_every_section_in_place(data) {
        const page = createTemporaryObject(pageMaker, test, {width:1400})
        ready(page)
        const latest = page.blockOf(artistKey + "|latest")
        const albums = page.blockOf(artistKey + "|album")
        tryVerify(() => page.blockOf(artistKey + "|tracks")?.ready === true)
        if (data.jump) page.jump(artistKey + "|album")
        let settledAt = -1
        tryVerify(() => { const y = albums.mapToItem(page, 0, 0).y; const still = y === settledAt; settledAt = y; return still })
        const tracks = findChild(latest, "latestReleaseTracks").model
        const before = {y:albums.mapToItem(page, 0, 0).y, height:latest.height}
        // A change in the catalog reads the list again: for a moment its model is not ready.
        tracks.ready = false
        waitForRendering(page)
        compare(latest.height, before.height, "a section already drawn must keep its height while it reads again")
        compare(albums.mapToItem(page, 0, 0).y, before.y, "what follows it must not move")
        tracks.ready = true
        waitForRendering(page)
        compare(latest.height, before.height)
        compare(albums.mapToItem(page, 0, 0).y, before.y)
    }
    function cleanup() { SettingsModel.density = "compact" }
    function test_delayed_release_card_without_a_track_preview_keeps_albums_in_place() {
        const doc = document()
        delete doc.shelves[1].tracks_key
        Sources.pages = {[artistKey]:JSON.stringify(doc)}
        const page = createTemporaryObject(pageMaker, test)
        ready(page)
        const latest = page.blockOf(artistKey + "|latest")
        const albums = page.blockOf(artistKey + "|album")
        const record = Array.from(latest.item.data).find(child => child.albums !== undefined && child.limit === 1)
        verify(record !== undefined)
        record.ready = false
        record.albums = 0
        waitForRendering(page)
        page.jump(artistKey + "|album")
        waitForRendering(page)
        const before = albums.mapToItem(page, 0, 0).y
        const arrival = createTemporaryObject(arrivalMaker, test, {model:record, album:true})
        arrival.start()
        tryCompare(record, "ready", true)
        waitForRendering(page)
        verify(Math.abs(albums.mapToItem(page, 0, 0).y - before) <= 1,
            "Albums moved after the preceding card settled: " + before + " -> " + albums.mapToItem(page, 0, 0).y)
    }
    function test_wheel_scroll_updates_the_anchor_without_snapping_to_a_heading_data() {
        return [{tag:"standalone", memory:false}, {tag:"with_scroll_memory", memory:true}]
    }
    function test_wheel_scroll_updates_the_anchor_without_snapping_to_a_heading(data) {
        if (data.memory) Visit.keep(artistKey, 120)
        const page = createTemporaryObject(pageMaker, test)
        const memory = data.memory ? createTemporaryObject(memoryMaker, test, {viewport:page.scrollView,key:artistKey}) : null
        ready(page)
        if (memory) {
            tryCompare(memory, "pending", -1)
            compare(page.scrollView.contentY, 120, "the remembered offset was restored before user scrolling")
        }
        const albums = page.blockOf(artistKey + "|album")
        const latest = page.blockOf(artistKey + "|latest")
        // Popular Tracks, above the releases, loads on a turn of its own and takes a few frames to lay out. Settled before the
        // scroll, it cannot move the releases during the wheel, which took the wheel's motion with it.
        const popular = page.blockOf(artistKey + "|tracks")
        tryVerify(() => popular?.ready === true && findChild(page, "artistHeaderLoader").status === Loader.Ready, 2000, "the header and Popular Tracks are loaded")
        const popularRows = findChild(popular.item, "trackListView")
        tryVerify(() => popularRows.count > 0 && Array.from({length:popularRows.count}, (_, i) => popularRows.itemAtIndex(i)).every(row => row !== null), 2000, "every popular track has its row")
        popularRows.forceLayout(); popular.item.forceLayout(); albums.parent.forceLayout()
        const tracks = findChild(latest, "latestReleaseTracks").model
        const rows = []
        for (let i = 0; i < tracks.count; i++) rows.push(Object.assign({}, tracks.get(i)))
        tracks.ready = false
        tracks.clear()
        waitForRendering(page)
        page.scrollView.contentY = albums.parent.y + albums.y - 48
        const initial = page.scrollView.contentY
        waitForRendering(page)
        mouseMove(page.scrollView, 10, 100)
        mouseWheel(page.scrollView, 10, 100, 0, -120, Qt.NoButton, Qt.NoModifier, 20)
        tryVerify(() => page.scrollView.contentY > initial, 1000,
            "wheel scroll from " + initial + ", height=" + page.scrollView.contentHeight + ", viewport=" + page.scrollView.height)
        tryCompare(page.scrollView, "moving", false)
        const before = albums.mapToItem(page, 0, 0).y
        verify(before < 48, "the wheel moved beyond the section's heading")
        const arrival = createTemporaryObject(arrivalMaker, test, {model:tracks, rows:rows})
        arrival.start()
        tryCompare(tracks, "ready", true)
        waitForRendering(page)
        verify(Math.abs(albums.mapToItem(page, 0, 0).y - before) <= 1)
        if (memory) {
            compare(memory.pending, -1)
            compare(Visit.recall(artistKey, -1), page.scrollView.contentY, "history follows the compensated offset")
            memory.enabled = false
        }
    }
    function test_source_switch_drops_the_previous_sections_anchor() {
        const page = createTemporaryObject(pageMaker, test)
        ready(page)
        page.jump(artistKey + "|album")
        const otherKey = "second native:artist:41"
        const next = document()
        next.header.plugin = "second"
        next.header.ref = "artist:41"
        next.shelves = next.shelves.slice(1).map(section => Object.assign({}, section, {
            key:section.key.replace(artistKey, otherKey),
            tracks_key:section.tracks_key ? section.tracks_key.replace(artistKey, otherKey) : undefined
        }))
        Sources.pages = {[artistKey]:JSON.stringify(document()), [otherKey]:JSON.stringify(next)}
        const offset = page.scrollView.contentY
        page.key = otherKey
        tryVerify(() => page.blockOf(otherKey + "|album")?.ready === true)
        waitForRendering(page)
        compare(page.scrollView.contentY, offset, "the old section cannot move the new source's scroll position")
        page.jump("about")
        Library.remoteRev++
        waitForRendering(page)
        compare(page.scrollView.contentY, 0, "returning to About keeps the origin through revalidation")
    }
    Component {
        id: arrivalMaker
        Timer {
            property var model
            property var rows
            property bool album: false
            interval: 20
            onTriggered: {
                if (album) model.albums = 1
                else for (const row of rows) model.append(row)
                model.ready = true
            }
        }
    }
}
