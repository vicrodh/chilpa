import QtQuick
import QtQuick.Controls.Basic
import QtTest
import app.chilpa
import "../../qml/Components"
import "../../qml/Patterns"

TestCase {
    name: "PlaylistDialogs"
    when: windowShown
    visible: true
    width: 960
    height: 600

    PlaylistEditDialog { id: editor }
    PlaylistFolderDialog { id: folder }

    function init() {
        Playlists.fail = false
        Playlists.created = []
        Playlists.added = ""
        Playlists.sourceChoices = []
        Playlists.remoteWrite = []
        Playlists.rows = [
            { playlist: { id: 1, name: "Evening music", mixed: true, track_count: 8 }, accepts: true, exact: 1, similar: 1 },
            { playlist: { id: 2, name: "Road music", mixed: false, provider: "Server", track_count: 12 }, accepts: false, exact: 0, similar: 0 }
        ]
    }
    function cleanup() { editor.close(); folder.close(); wait(0) }
    function field(dialog) { return findChild(dialog.contentItem, "playlistName") }

    function test_edit_loads_and_preserves_metadata_and_folder() {
        editor.show(7)
        tryCompare(editor, "opened", true)
        compare(field(editor).width, editor.width - 40)
        compare(field(editor).text, "Stored playlist")
        field(editor).text = "Renamed playlist"
        editor.save()
        compare(Playlists.created, ["Renamed playlist", "Stored description", "cover.png", "night, jazz", 4, 7])
        tryCompare(editor, "visible", false)
    }

    function test_folder_create_edit_and_failure_keep_the_same_geometry() {
        folder.show(0, "", "accent")
        tryCompare(folder, "opened", true)
        const name = findChild(folder.contentItem, "folderName")
        const submit = findChild(folder.contentItem, "saveFolder")
        compare(name.width, folder.width - 40)
        compare(submit.enabled, false)
        name.text = "Folder"
        folder.save()
        compare(Playlists.created, ["Folder", "accent"])
        tryCompare(folder, "visible", false)
        folder.show(4, "Folder", "positive")
        Playlists.fail = true
        folder.save()
        verify(folder.visible)
        verify(folder.notice !== "")
        Playlists.fail = false
        name.text = "Renamed"
        folder.save()
        compare(Playlists.created, ["Renamed", "positive", 4])
    }
}
