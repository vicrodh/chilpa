import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import "../../qml/Screens"

// The artist page draws the facade's shaped document: the blocks in their
// order, the jump bar, and the Network sidebar with what the source and
// the network know.
TestCase {
    name: "ArtistPage"; when: windowShown; visible: true; width: 1400; height: 900
    readonly property string key: "fixture native:artist:1"
    function document() {
        return JSON.stringify({ music: true, artist: true, title: "Maiden", header: { kind: "artist", title: "Maiden", plugin: "fixture", ref: "artist:1", description: "x".repeat(500), radio_id: -5, kept: 3 },
            jump: [{ id: "about", title: "About" }, { id: key + "|tracks", title: "Popular Tracks" }, { id: key + "|album", title: "Albums" }],
            shelves: [
                { id: "tracks", role: "popular", title: "Popular Tracks", kind: "track", key: key + "|tracks", tracks: true, items: [{ ref: "track:1" }] },
                { id: "latest", role: "latest", title: "Latest release", kind: "album", key: key + "|latest", items: [{ ref: "album:9" }] },
                { id: "album", role: "release", section: "album", title: "Albums", kind: "album", key: key + "|album", items: [{ ref: "album:1" }], more: "x" },
                { id: "labels", role: "labels", sidebar: true, kind: "label", key: key + "|labels", items: [{ ref: "label:3", title: "EMI" }] },
                { id: "similar", role: "similar", sidebar: true, kind: "artist", key: key + "|similar", items: [{ ref: "artist:2", title: "Saxon" }] }
            ] })
    }
    Component { id: pageMaker; ArtistPage { width: 1400; height: 900; key: "fixture native:artist:1" } }
    function findText(item, text) {
        if (item.text === text) return item
        for (const child of item.children || []) { const found = findText(child, text); if (found) return found }
        return null
    }
    function init() {
        Sources.pages = { "fixture native:artist:1": document() }
        Sources.networks = { "Maiden": JSON.stringify({ loading: false, available: true, origin: { person: false, begin: "Dec 25, 1975", end: "", place: "Leyton", exact: true },
            relationships: { any: true, members: [{ name: "Steve Harris", tooltip: "bass (1975 - present)" }], groups: [], collaborators: [] }, similar: ["Judas Priest", "Saxon"], resolves: true }) }
        Library.remoteRev++
    }

    function test_the_blocks_come_in_order_with_the_bar_and_the_sidebar() {
        const page = createTemporaryObject(pageMaker, this)
        waitForRendering(page)
        verify(findText(page, "Popular Tracks"), "the tracks block")
        verify(findText(page, "Latest release"))
        verify(findText(page, "Albums"))
        verify(findText(page, "JUMP TO"))
        verify(findText(page, "Read more"), "a long biography is cut")
        verify(findChild(page, "radioAction").visible)
        verify(findChild(page, "networkAction").checked, "the sidebar opens by itself")
        // The sidebar: labels and similar artists from the source, the
        // origin and the members from the network.
        verify(findText(page, "LABELS"))
        verify(findText(page, "EMI"))
        verify(findText(page, "Saxon"))
        verify(findText(page, "FOUNDED IN"))
        verify(findText(page, "Leyton"))
        verify(findText(page, "Steve Harris"))
        verify(findText(page, "Judas Priest"), "a network artist the source does not name")
        compare(findText(page, "YOU MAY ALSO LIKE") !== null, true)
    }

    // Top right, the library and each installed source with artist pages,
    // each the same artist by name; for a source, its catalog or what the
    // listener keeps of the artist there.
    function test_the_page_switches_source_and_shows_what_the_listener_keeps() {
        Sources.show({ panels: [{ id: "fixture", name: "Fixture", artists: true }, { id: "radio", name: "Radio", artists: false }] })
        const kept = "fixture artist-library:artist:1"
        Sources.pages[kept] = JSON.stringify({ music: true, artist: true, library: true, header: JSON.parse(document()).header, jump: [],
            shelves: [{ id: "tracks", role: "popular", title: "Tracks", kind: "track", key: kept + "|tracks", tracks: true, items: [{ ref: "track:1" }] }] })
        const page = createTemporaryObject(pageMaker, this)
        waitForRendering(page)
        const sources = findChild(page, "artistSource")
        compare(sources.options.map(o => o.text), ["Chilpa", "Fixture"])
        compare(sources.current, 1)
        const shelf = findChild(page, "artistShelf")
        compare(shelf.options[1].count, 3, "In library says how much it holds")
        verify(findText(shelf, "3").visible, "in a chip after its word")
        verify(shelf.mapToItem(page, 0, 0).y >= sources.mapToItem(page, 0, 0).y + sources.height, "the shelf goes under the sources")
        compare(Math.round(shelf.mapToItem(page, shelf.width, 0).x), Math.round(sources.mapToItem(page, sources.width, 0).x), "both end at the right")
        shelf.activated(1)
        tryVerify(() => findText(page, "Tracks") !== null, 1000, "what the listener keeps")
        verify(!findText(page, "Latest release"), "the catalog's blocks leave")
        sources.activated(0)
        compare(Navigation.went, ["artist", "Maiden"])
        Sources.show({ panels: [{ id: "radio", name: "Radio", icon: "broadcast" }] })
    }

    function test_the_network_button_closes_and_opens_the_sidebar() {
        const page = createTemporaryObject(pageMaker, this)
        waitForRendering(page)
        const button = findChild(page, "networkAction")
        mouseClick(button)
        tryCompare(button, "checked", false)
        mouseClick(button)
        tryCompare(button, "checked", true)
    }

    function test_a_sidebar_row_opens_the_artist_at_its_source() {
        const page = createTemporaryObject(pageMaker, this)
        waitForRendering(page)
        const row = findText(page, "Saxon")
        mouseClick(row)
        compare(Navigation.went, ["source", "fixture native:artist:2"])
        mouseClick(findText(page, "Judas Priest"))
        compare(Navigation.went, ["source", "fixture matching-artist-v3:Judas Priest"])
    }

    function test_play_and_the_menu_act_on_every_popular_track() {
        const page = createTemporaryObject(pageMaker, this)
        waitForRendering(page)
        // Five rows first; "Load more" shows the eight, in the window.
        const more = findChild(page, "tracksMore")
        verify(more.visible)
        tryVerify(() => findText(page, "Track 4") !== null)
        compare(findText(page, "Track 5"), null)
        mouseClick(more)
        tryCompare(more, "text", "View less")
        tryVerify(() => findText(page, "Track 7") !== null)
        const menuButton = findChild(page, "tracksMenu")
        let menu = null
        for (let i = 0; i < menuButton.data.length; i++) if (menuButton.data[i].objectName === "tracksMenuPopup") menu = menuButton.data[i]
        verify(menu, "the menu hangs from its button")
        mouseClick(menuButton)
        tryCompare(menu, "visible", true)
        compare(menu.entries.map(e => e.act), ["next", "later", "queue", "shuffle", "pick"])
        menu.chosen(3)
        menu.close()
    }
}
