import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import app.chilpa.playlists
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
        PlaylistManager.rows = stored.concat([{ kind: "Playlist", id: 9, name: "From a source", detail: "", source: "fixture", source_name: "fixture", artwork_path: "", covers: [], color: "", track_count: 5, play_count: 0, folder_id: null, local: false, visible_sidebar: true }])
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

    // On a card, whose it is reads on its cover: Chilpa's mark for the
    // library's own, the source's icon for a source's.
    function test_a_card_says_whose_it_is() {
        mixed()
        screen.grid = true
        tryVerify(() => named(screen, "playlistOrigin").length === 2)
        const badges = named(screen, "playlistOrigin").map(badge => badge.parent)
        const local = badges.find(row => row.entryId === 7), source = badges.find(row => row.entryId === 9)
        verify(local.origin.chilpa === true)
        compare(source.origin.icon, "broadcast")
        compare(local.owner, "Chilpa", "not the provider's key")
        compare(source.owner, "Fixture")
        verify(named(screen, "playlistOrigin").every(badge => badge.visible))
        const mark = findChild(named(screen, "playlistOrigin").find(badge => badge.parent.entryId === 7), "sourceImage")
        tryCompare(mark, "status", Image.Ready, 2000, "Chilpa's mark is drawn, not the folder glyph")
        verify(String(mark.source).endsWith("chilpa-mono.svg"))
    }

    function test_edit_is_reachable_from_grid_and_list() {
        for (const grid of [true, false]) {
            screen.grid = grid
            wait(0)
            const button = findChild(screen, "playlistMore")
            verify(button !== null)
            waitForRendering(button)
            mouseClick(button, button.width / 2, button.height / 2)
            const menu = findChild(button.parent.parent, "playlistActions")
            verify(menu !== null)
            tryCompare(menu, "opened", true)
            let edit = null
            for (const item of menu.entryItems)
                if (item.entry !== undefined && item.entry.act === "edit-playlist") edit = item
            verify(edit !== null)
            waitForRendering(edit)
            mouseClick(edit, edit.width / 2, edit.height / 2)
            const dialog = findChild(screen, "playlistEditDialog")
            tryCompare(dialog, "opened", true)
            compare(findChild(dialog.contentItem, "playlistName").text, "Stored playlist")
            dialog.close()
        }
    }
}
