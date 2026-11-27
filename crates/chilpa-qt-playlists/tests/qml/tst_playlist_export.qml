import QtQuick
import QtTest
import app.chilpa
import app.chilpa.playlists
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Components"

// A playlist written to a file: offered on every playlist of the sidebar,
// with its files or as plain data, and once written it says what went in
// and what had no place there. A name with an emoji or a symbol warns that
// a source may cut it.
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
    function message() { return findChild(exporter.contentItem, "playlistExportMessage").text }

    function test_a_playlist_goes_out_with_its_files_or_as_plain_data() {
        PlaylistManager.exportResult = {written: 3, left_out: 0, error: ""}
        actions.row(sidebar.rows.find(row => row.playlistId === 21), 0, 0)
        verify(offered().indexOf("export") >= 0 && offered().indexOf("exportPlain") >= 0, String(offered()))
        menu().chosen(offered().indexOf("exportPlain"))
        compare(PlaylistManager.exports[0].id, 21)
        compare(PlaylistManager.exports[0].plain, true)
        tryCompare(exporter, "opened", true)
        exporter.close()
        actions.row(sidebar.rows.find(row => row.playlistId === 21), 0, 0)
        menu().chosen(offered().indexOf("export"))
        compare(PlaylistManager.exports[1].plain, false)
        tryCompare(exporter, "opened", true)
        exporter.close()
    }

    function test_a_written_list_says_how_many_tracks_went_in() {
        PlaylistManager.exportResult = {written: 3, left_out: 0, error: ""}
        PlaylistManager.exportList(21, false)
        tryCompare(exporter, "opened", true)
        verify(message().indexOf("3") >= 0, message())
        verify(message().indexOf("XSPF") < 0, "nothing was left out: " + message())
        exporter.close()
    }

    function test_tracks_that_are_no_files_here_are_said_with_where_they_are_kept() {
        PlaylistManager.exportResult = {written: 2, left_out: 1, error: ""}
        PlaylistManager.exportList(21, false)
        tryCompare(exporter, "opened", true)
        verify(message().indexOf("XSPF") >= 0, message())
        exporter.close()
    }

    function test_a_list_that_could_not_be_written_says_why() {
        PlaylistManager.exportResult = {written: 0, left_out: 0, error: "the list could not be saved: disk full"}
        PlaylistManager.exportList(21, false)
        tryCompare(exporter, "opened", true)
        compare(message(), "the list could not be saved: disk full")
        exporter.close()
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
