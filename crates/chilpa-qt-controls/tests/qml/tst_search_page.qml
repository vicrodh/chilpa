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

    function row(kind, section, title) { return {kind: kind, section: section, title: title, detail: "", art: "", duration: "", sourceIcon: "", sourceName: "", albumId: 0, artist: "", year: ""} }
    // Even records are the library's own, odd ones a source's, with a catalog id.
    function record(i) {
        const theirs = i % 2 === 1
        return Object.assign(row(3, "albums", "Record " + i), {albumId: theirs ? -(i + 1) : i + 1, artist: "Band", year: "1999", sourceIcon: theirs ? "broadcast" : "", sourceName: theirs ? "Fixture" : ""})
    }
    function found(albums) {
        let rows = [row(0, "top", "Top result"), row(1, "top", "Mora Vale"), row(0, "artists", "Artists"), row(2, "artists", "Mora")]
        rows.push(row(0, "albums", "Albums"))
        for (let i = 0; i < albums; i++) rows.push(record(i))
        rows.push(row(0, "songs", "Songs"))
        for (let i = 0; i < 8; i++) rows.push(Object.assign(row(4, "songs", "Song " + i), {key: "track:" + i}))
        rows.push(row(0, "playlists", "Playlists"))
        rows.push(row(6, "playlists", "Own list"))
        rows.push(Object.assign(row(6, "playlists", "Their list"), {sourceIcon: "broadcast", sourceName: "Fixture"}))
        return rows
    }
    function shown(prefixTest) {
        let out = []
        const walk = node => { if (node.visible && prefixTest(node)) out.push(node); for (const child of node.children || []) walk(child) }
        walk(page.contentItem)
        return out
    }
    function records() { return shown(node => node.hasOwnProperty("albumId") && node.hasOwnProperty("qualityInfo")) }
    function named(item, prefix) {
        let out = []
        const walk = node => { if ((node.objectName || "").startsWith(prefix)) out.push(node); for (const child of node.children || []) walk(child) }
        walk(item)
        return out
    }
    function init() {
        AlbumActions.sourcesOf = ({})
        page.show("all")
        page.search.rowsJsonText = JSON.stringify(found(25))
        page.read()
    }

    // A record is drawn as it is everywhere else, and says whose it is.
    function test_records_are_drawn_as_records_are_everywhere_else() {
        AlbumActions.sourcesOf = ({[-2]: [{icon: "broadcast", name: "Fixture"}]})
        page.read()
        tryVerify(() => records().length === 25)
        const own = records().find(card => card.title === "Record 0"), theirs = records().find(card => card.title === "Record 1")
        compare(own.albumId, 1)
        compare(own.subtitle, "Band")
        compare(own.qualityInfo.tier, 3, "the record's quality, as a library card shows it")
        verify(own.sources[0].chilpa === true, "the library's own wear Chilpa's mark")
        compare(theirs.albumId, -2)
        compare(theirs.sources[0].icon, "broadcast")
        verify(own.available && !own.canExpand)
    }

    // Records page three rows at a time, read across and down, by the arrows.
    function test_records_page_up_to_three_rows() {
        const rail = findChild(page.contentItem, "searchAlbumsRail")
        const pages = findChild(rail, "searchRailPages")
        compare(rail.used, 3)
        verify(rail.pageCount > 1, "twenty-five records need a second page: " + rail.perPage)
        tryVerify(() => records().length === 25)
        const at = index => records().find(card => card.title === "Record " + index)
        compare(at(1).y, at(0).y, "across first")
        compare(at(rail.columns).x, at(0).x, "then down")
        compare(at(rail.columns).y, at(0).y + rail.cellHeight + Metrics.gridGap)
        compare(at(rail.perPage).x, at(0).x + rail.step, "the next page")
        const header = rail.children[0]
        verify(header.canForward && !header.canBack)
        header.forward()
        compare(pages.contentX, rail.step)
        page.search.rowsJsonText = JSON.stringify(found(2))
        page.read()
        compare(rail.used, 1, "two records fill one row, not three")
    }

    // Load more adds a few songs where they are; See all is their own tab.
    function test_all_loads_six_more_songs_at_a_time() {
        page.search.rowsJsonText = JSON.stringify(found(25).concat(Array.from({length: 12}, (_, i) => row(4, "songs", "More " + i))))
        page.read()
        const songs = () => shown(node => node.hasOwnProperty("best")).length
        const more = findChild(page.contentItem, "searchSongsMore")
        verify(more.visible)
        compare(more.text, "Load more (6 / 20)")
        more.clicked()
        compare(songs(), 12)
        compare(page.tab, "all", "still on All")
        more.clicked()
        compare(songs(), 18)
        more.clicked()
        compare(songs(), 20)
        verify(!more.visible)
    }

    // Artists and playlists have their own cards, and say whose they are.
    function test_artists_and_playlists_say_whose_they_are() {
        const artist = shown(node => node.objectName === "sceneArtistCard")[0]
        compare(artist.artist.title, "Mora")
        verify(artist.artist.sources[0].chilpa === true)
        const lists = shown(node => node.hasOwnProperty("folderColor") && node.hasOwnProperty("covers"))
        compare(lists.map(list => list.detail), ["Chilpa", "Fixture"])
        verify(lists[0].origin.chilpa === true)
        compare(lists[1].origin.icon, "broadcast")
    }

    function test_a_song_of_the_library_wears_chilpa_s_mark() {
        const marks = shown(node => node.objectName === "hitOrigin")
        compare(marks.length, 6)
        verify(marks[0].sourceInfo.chilpa === true)
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

    // On the Tracks tab songs are chosen a click each, a range with Shift or
    // all with Ctrl+A, and the bar acts on them in the order they are listed.
    function test_songs_are_chosen_and_acted_on_together() {
        page.show("songs")
        const toggle = findChild(page.contentItem, "searchSelectSongs")
        verify(toggle.visible)
        toggle.clicked()
        waitForRendering(page)
        const chosen = page.contentItem.children.find(item => item.hasOwnProperty("picked"))
        const rows = named(page.contentItem, "searchSongChoice").filter(item => item.visible)
        compare(rows.length, 8)
        mouseClick(rows[1])
        mouseClick(rows[3], 5, 5, Qt.LeftButton, Qt.ShiftModifier)
        compare(chosen.keys, ["track:1", "track:2", "track:3"])
        mouseClick(rows[2])
        compare(chosen.keys, ["track:1", "track:3"], "a second click lets a song go")
        const bar = findChild(page, "searchChosenBar")
        verify(bar.visible)
        compare(findChild(bar, "searchChosenCount").text, "2 tracks")
        findChild(bar, "searchChosenQueue").clicked()
        compare(page.search.actedSongs, {keys: ["track:1", "track:3"], act: "queue", label: "Search"})
        findChild(bar, "searchChosenPlaylist").clicked()
        compare(page.search.pickedSongs, ["track:1", "track:3"])
        keyClick(Qt.Key_A, Qt.ControlModifier)
        compare(chosen.keys.length, 8, "every song shown")
        keyClick(Qt.Key_Escape)
        verify(!chosen.selecting && chosen.keys.length === 0 && !bar.visible)
        toggle.clicked()
        mouseClick(rows[0])
        page.show("albums")
        verify(!chosen.selecting && chosen.keys.length === 0, "another tab lets the choice go")
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
