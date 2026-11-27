import QtQuick
import QtTest
import app.chilpa
import app.chilpa.playlists
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Components"

// A playlist written to a file: offered on every playlist of the sidebar,
// and once written it says what went in and what had no place there. A
// name with an emoji or a symbol warns that a source may cut it.
TestCase {
    name: "PlaylistExport"
    when: windowShown
    visible: true
    width: 960
    height: 600
    SidebarPlaylists { id: sidebar; width: 240; height: 580 }
    SidebarPlaylistActions { id: actions; sidebar: sidebar }
    PlaylistExport { id: exporter }
    PlaylistNameNote { id: note; width: 300 }

    function init() {
        Playlists.catalog = {folders: [], playlists: [
            {id: 21, name: "Evening", source: "", local: true, mixed: false, visible_sidebar: true, track_count: 3,
                artwork_path: "", covers: [], custom_position: 0, updated_at: 1}]}
        sidebar.reload()
        PlaylistManager.exports = []
    }
    function menu() { return findChild(actions, "playlistsMenu") }
    function offered() { return menu().entries.map(entry => entry.action) }
    function outcome() { return findChild(exporter, "playlistExportOutcome") }
    function message() { return findChild(outcome().contentItem, "playlistExportMessage").text }

    function test_every_playlist_of_the_sidebar_offers_to_be_exported() {
        actions.row(sidebar.rows.find(row => row.playlistId === 21), 0, 0)
        verify(offered().indexOf("export") >= 0, String(offered()))
        menu().close()
    }

    // M3U8 for players, XSPF and the importer's JSON for every track.
    function test_the_three_formats_are_offered() {
        const filters = findChild(exporter, "playlistExportPicker").nameFilters
        compare(filters.length, 3)
        verify(filters[0].indexOf("*.m3u8") >= 0 && filters[1].indexOf("*.xspf") >= 0 && filters[2].indexOf("*.json") >= 0, String(filters))
    }

    function test_a_written_list_says_how_many_tracks_went_in() {
        PlaylistManager.exported = {written: 3, left_out: 0, error: ""}
        exporter.playlistId = 21
        exporter.save("/tmp/evening.m3u8")
        compare(PlaylistManager.exports[0].id, 21)
        compare(PlaylistManager.exports[0].path, "/tmp/evening.m3u8")
        tryCompare(outcome(), "opened", true)
        verify(message().indexOf("3") >= 0, message())
        verify(message().indexOf("XSPF") < 0, "nothing was left out: " + message())
        outcome().close()
    }

    function test_tracks_that_are_no_files_here_are_said_with_where_they_are_kept() {
        PlaylistManager.exported = {written: 2, left_out: 1, error: ""}
        exporter.save("/tmp/evening.m3u8")
        tryCompare(outcome(), "opened", true)
        verify(message().indexOf("XSPF") >= 0, message())
        outcome().close()
    }

    function test_a_list_that_could_not_be_written_says_why() {
        PlaylistManager.exported = {written: 0, left_out: 0, error: "the list could not be saved: disk full"}
        exporter.save("/tmp/evening.m3u8")
        tryCompare(outcome(), "opened", true)
        compare(message(), "the list could not be saved: disk full")
        outcome().close()
    }

    function test_a_name_with_an_emoji_or_a_symbol_warns_that_a_source_may_cut_it() {
        note.name = "Evening"
        verify(!note.visible)
        note.name = "Evening 🌙"
        verify(note.visible)
        note.name = "Love ♥"
        verify(note.visible)
        note.name = "Zoë’s «best»"
        verify(!note.visible, "accents and quotes are ordinary")
    }
}
