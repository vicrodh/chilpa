import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Screens"

// The results page, as QBZ's: All lays out the top result, the artists, the
// records, six songs and the playlists; a tab lists its own twenty at a
// time; the switch keeps the same preference as Settings.
TestCase {
    name: "SearchPage"
    when: windowShown
    visible: true
    width: 1200
    height: 900

    SearchScreen { id: page; width: 1200; height: 900; query: "mora" }

    function row(kind, section, title) { return {kind: kind, section: section, title: title, detail: "", art: "", duration: "", sourceIcon: "", sourceName: ""} }
    function found(albums) {
        let rows = [row(0, "top", "Top result"), row(1, "top", "Mora Vale"), row(0, "artists", "Artists"), row(2, "artists", "Mora")]
        rows.push(row(0, "albums", "Albums"))
        for (let i = 0; i < albums; i++) rows.push(row(3, "albums", "Record " + i))
        rows.push(row(0, "songs", "Songs"))
        for (let i = 0; i < 8; i++) rows.push(row(4, "songs", "Song " + i))
        return rows
    }
    function named(item, prefix) {
        let out = []
        const walk = node => { if ((node.objectName || "").startsWith(prefix)) out.push(node); for (const child of node.children || []) walk(child) }
        walk(item)
        return out
    }
    function init() {
        page.show("all")
        page.search.rowsJsonText = JSON.stringify(found(25))
        page.read()
    }

    function test_all_lays_out_each_section_and_six_songs() {
        compare(page.sections.top.length, 1)
        compare(page.sections.albums.length, 25)
        compare(page.sections.songs.length, 8)
        compare(page.sections.albums[0].at, 5, "each row keeps its place in the model")
        const hits = named(page.contentItem, "").filter(item => item.hasOwnProperty("best") && item.visible)
        compare(hits.length, 6, "six songs on All")
    }

    function test_a_tab_lists_its_own_twenty_at_a_time() {
        page.show("albums")
        compare(page.search.scope, "albums")
        const more = findChild(page.contentItem, "searchLoadMore")
        verify(more.visible)
        compare(more.text, "Load more (20 / 25)")
        more.clicked()
        compare(page.shown, 40)
        verify(!more.visible)
        page.show("all")
        compare(page.search.scope, "page", "All asks for every section whole")
    }

    function test_the_switch_keeps_the_preference_settings_shows() {
        const toggle = findChild(page.contentItem, "searchPreviewSwitch")
        verify(toggle.checked, "on until turned off")
        toggle.toggled(false)
        compare(SettingsModel.kept("navigation.searchPreview"), "false")
        tryVerify(() => !toggle.checked)
        toggle.toggled(true)
        tryVerify(() => toggle.checked)
    }
}
