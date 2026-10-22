import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import app.chilpa.playlists
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Screens"

TestCase {
    name: "PlaylistManagerActions"
    when: windowShown
    visible: true
    width: 960
    height: 600
    PlaylistManagerScreen { id: screen; anchors.fill: parent }

    // The mock's own rows, kept before any test adds to them.
    property var stored: []
    function initTestCase() { stored = PlaylistManager.rows.slice() }
    function mixed() {
        PlaylistManager.rows = stored.concat([{ kind: "Playlist", id: -9, name: "From a source", detail: "", source: "fixture", source_name: "fixture", artwork_path: "", covers: [], color: "", track_count: 5, play_count: 0, folder_id: null, local: false, writable: false, visible_sidebar: true }])
        Sources.state = Object.assign({}, Sources.state, {panels: [{id: "fixture", name: "Fixture", icon: "broadcast"}]})
        screen.reload()
    }
    function cleanup() {
        screen.source = ""
        PlaylistManager.rows = stored
        screen.reload()
    }
    function named(item, name) {
        const out = []
        const walk = node => { if (node.objectName === name) out.push(node); for (const child of node.children || []) walk(child) }
        walk(item)
        return out
    }

    // Whose playlists the page lists is chosen as on the Artists screen, and
    // kept: one source's, the library's own, or every one.
    function test_a_source_filter_lists_one_source_s_playlists() {
        mixed()
        compare(screen.rows.length, 2)
        const button = findChild(screen, "playlistSourceFilter")
        verify(button.visible, "two sources to tell apart")
        compare(button.text, "All sources")
        const menu = findChild(screen, "playlistSourceMenu")
        compare(menu.entries.map(entry => entry.text), ["All sources", "Chilpa", "Fixture"])
        menu.chosen(2)
        compare(PlaylistManager.askedSource, "fixture")
        compare(screen.rows.map(row => row.name), ["From a source"])
        compare(button.text, "Fixture")
        compare(SettingsModel.kept("ui.playlists.source"), "fixture")
        menu.chosen(1)
        compare(screen.rows.map(row => row.name), ["Stored playlist"])
        menu.chosen(0)
        compare(screen.rows.length, 2)
    }

    function cards() { return named(screen, "playlistCardName").map(name => name.parent.parent) }
    function cardOf(id) { return cards().find(card => card.modelData.entryId === id) }
    // Opens the shared menu from a card or a row and picks what it offers.
    function choose(menu, act, what) {
        tryCompare(menu, "opened", true, 2000, what || act)
        let item = null
        tryVerify(() => { item = menu.entryItems.find(row => row.entry !== undefined && row.entry.act === act); return !!item }, 2000,
            "the menu offers " + act + ": " + JSON.stringify(menu.entries.map(entry => entry.act)))
        waitForRendering(item)
        mouseClick(item, item.width / 2, item.height / 2)
    }

    // The grid draws the cards of the rest of the app: the cover wears
    // whose it is as a record's sources, Chilpa's mark for the library's own.
    function test_a_card_is_the_app_s_card_and_says_whose_it_is() {
        mixed()
        screen.grid = true
        tryVerify(() => cards().length === 2)
        const local = cardOf(7), source = cardOf(-9)
        verify(local.width === CardLayout.cardWidth, "a record's card size")
        verify(local.origin.chilpa === true)
        compare(source.origin.icon, "broadcast")
        compare(local.detail, "Chilpa · 3 tracks", "not the provider's key")
        compare(source.detail, "Fixture · 5 tracks")
        const mark = findChild(findChild(local, "playlistCardOrigin"), "sourceImage")
        tryCompare(mark, "status", Image.Ready, 2000, "Chilpa's mark is drawn, not the folder glyph")
        verify(String(mark.source).endsWith("chilpa-mono.svg"))
    }

    // Edit is reached from a card and from a row, through one menu.
    function test_edit_is_reachable_from_grid_and_list() {
        const menu = findChild(screen, "playlistActions")
        for (const grid of [true, false]) {
            screen.grid = grid
            wait(0)
            if (grid) {
                tryVerify(() => cards().length > 0)
                cards()[0].menuAsked(0, 0)
            } else {
                const button = findChild(screen, "playlistMore")
                waitForRendering(button)
                mouseClick(button, button.width / 2, button.height / 2)
            }
            choose(menu, "edit", grid ? "from a card" : "from a row")
            const dialog = findChild(screen, "playlistEditDialog")
            tryCompare(dialog, "opened", true)
            compare(findChild(dialog.contentItem, "playlistName").text, "Stored playlist")
            dialog.close()
            tryCompare(dialog, "visible", false)
        }
    }

    // Remove asks again. The library's own is deleted; a source's goes
    // through the facade, which deletes it at the source when it is the
    // listener's and stops following it otherwise.
    function test_remove_asks_again_before_it_removes() {
        mixed()
        screen.grid = true
        const menu = findChild(screen, "playlistActions")
        tryVerify(() => cards().length === 2)
        Playlists.removed = 0
        cardOf(7).menuAsked(0, 0)
        choose(menu, "remove")
        compare(Playlists.removed, 0, "nothing yet")
        compare(menu.entries[0].text, "Delete “Stored playlist”?")
        choose(menu, "confirm")
        compare(Playlists.removed, 7)
        cardOf(-9).menuAsked(0, 0)
        verify(!menu.entries.some(entry => entry.act === "edit"), "someone else's list is not edited here")
        choose(menu, "remove")
        compare(menu.entries[1].text, "Remove from library", "only followed: taken from the library")
        choose(menu, "confirm")
        compare(Playlists.removed, -9)
    }

    // A source's list that is the listener's is edited at its source.
    function test_a_source_s_own_list_is_edited_at_its_source() {
        PlaylistManager.rows = stored.concat([{ kind: "Playlist", id: -10, name: "Mine there", detail: "", source: "fixture", source_name: "fixture", artwork_path: "", covers: [], color: "", track_count: 2, play_count: 0, folder_id: null, local: false, writable: true, visible_sidebar: true }])
        Sources.state = Object.assign({}, Sources.state, {cards: {"-10": {plugin: "fixture", ref: "playlist:10", title: "Mine there", editable: true}}})
        screen.reload()
        screen.grid = true
        tryVerify(() => cards().length === 2)
        const menu = findChild(screen, "playlistActions")
        cardOf(-10).menuAsked(0, 0)
        choose(menu, "edit")
        const editor = findChild(screen, "sourcePlaylistEditor")
        tryCompare(editor, "opened", true)
        compare(editor.info.ref, "playlist:10")
        compare(editor.mode, "update")
        editor.close()
    }
}
