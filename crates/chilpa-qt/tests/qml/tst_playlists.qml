import QtQuick
import QtTest
import com.blitzfc.chilpa
import "../../qml/Components"
import "../../qml/Patterns"

TestCase {
    name: "PlaylistDialogs"
    when: windowShown
    visible: true
    width: 960
    height: 600

    PlaylistPickerDialog { id: picker }
    PlaylistEditDialog { id: editor }
    PlaylistFolderDialog { id: folder }

    function init() {
        Playlists.fail = false
        Playlists.created = []
        Playlists.added = ""
        Playlists.rows = [
            { playlist: { id: 1, name: "Evening music", mixed: true, track_count: 8 }, accepts: true, exact: 1, similar: 1 },
            { playlist: { id: 2, name: "Road music", mixed: false, provider: "Server", track_count: 12 }, accepts: false, exact: 0, similar: 0 }
        ]
    }
    function cleanup() { picker.close(); editor.close(); folder.close(); wait(0) }
    function field(dialog) { return findChild(dialog.contentItem, "playlistName") }
    function list() { return findChild(picker.contentItem, "playlistTargets") }

    function test_picker_has_width_height_and_target_rows() {
        picker.show("[42,43]")
        tryCompare(picker, "opened", true)
        compare(picker.trackCount, 2)
        compare(picker.creating, false)
        const search = findChild(picker.contentItem, "playlistSearch")
        compare(search.width, picker.width - 40)
        verify(list().height > 200)
        compare(list().count, 2)
        tryVerify(() => list().itemAtIndex(0) !== null)
        compare(list().itemAtIndex(0).target.playlist.name, "Evening music")
        compare(list().itemAtIndex(0).width, search.width)
        search.text = "ROAD"
        tryCompare(list(), "count", 1)
        search.text = "not found"
        tryCompare(list(), "count", 0)
        picker.close()
        picker.show("[44]")
        compare(search.text, "")
        compare(picker.trackCount, 1)
        tryCompare(list(), "count", 2)
    }

    function test_add_preserves_payload_and_reports_both_memberships() {
        picker.show("[42,43]")
        tryCompare(picker, "opened", true)
        tryVerify(() => list().itemAtIndex(0) !== null)
        const row = list().itemAtIndex(0)
        waitForRendering(row)
        mouseClick(row, row.width / 2, row.height / 2)
        compare(Playlists.added, "[42,43]")
        verify(picker.notice.includes("already"))
        verify(picker.notice.includes("similar"))
        verify(list().height > 0)
        Playlists.added = ""
        const disabled = list().itemAtIndex(1)
        mouseClick(disabled, disabled.width / 2, disabled.height / 2)
        compare(Playlists.added, "")
    }

    function test_create_resets_and_has_a_compact_full_width_form() {
        picker.beginCreate()
        tryCompare(picker, "opened", true)
        compare(picker.creating, true)
        compare(field(picker).width, picker.width - 40)
        verify(picker.height < 550, "height=" + picker.height)
        tryVerify(() => picker.height > 300, 1000, "the form is laid out before interacting")
        const submit = findChild(picker.contentItem, "createPlaylist")
        compare(submit.enabled, false)
        field(picker).text = "  "
        compare(submit.enabled, false)
        field(picker).text = "A new mix"
        compare(submit.enabled, true)
        waitForRendering(submit)
        mouseClick(submit, submit.width / 2, submit.height / 2)
        compare(Playlists.created[0], "A new mix")
        compare(Playlists.created[5], "[]")
        tryCompare(picker, "visible", false)
        picker.beginCreate()
        compare(field(picker).text, "")
    }

    function test_create_and_add_failure_keeps_the_form_and_selection() {
        picker.show("[42,43]")
        picker.creating = true
        field(picker).text = "Keep this draft"
        Playlists.fail = true
        picker.create()
        verify(picker.visible)
        verify(picker.notice !== "")
        compare(field(picker).text, "Keep this draft")
        compare(Playlists.created[5], "[42,43]")
    }

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

    function test_small_overlay_limits_the_dialog_and_leaves_scroll_space() {
        picker.beginCreate()
        tryCompare(picker, "opened", true)
        const overlay = picker.parent
        const oldHeight = overlay.height
        const oldWidth = overlay.width
        overlay.height = 340
        overlay.width = 430
        wait(0)
        verify(picker.height <= 320)
        compare(field(picker).width, picker.width - 40)
        picker.show("[42]")
        wait(0)
        verify(list().height > 0)
        overlay.height = oldHeight
        overlay.width = oldWidth
    }
}
