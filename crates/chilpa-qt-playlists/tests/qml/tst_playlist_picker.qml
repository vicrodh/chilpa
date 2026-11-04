import QtQuick
import QtQuick.Controls.Basic
import QtTest
import app.chilpa
import app.chilpa.playlists
import "../../qml/Patterns"

// The add-to-playlist picker as QBZ's: the search opens the list, a click
// chooses and the footer adds, repeats are asked about first, a name is
// asked in place to create one, and "Already in" lists what holds the
// tracks. Chilpa's own: each playlist's mark and the same-source switch.
TestCase {
    name: "PlaylistPicker"
    when: windowShown
    visible: true
    width: 960
    height: 700

    PlaylistPickerModal { id: picker }

    function init() {
        Playlists.fail = false
        Playlists.created = []
        Playlists.added = ""
        Playlists.repeated = false
        Playlists.lastTarget = 0
        Playlists.sourceChoices = []
        Playlists.remoteWrite = []
        SettingsModel.keep("ui.playlists.sameSource", "false")
        picker.sameSource = false
        Playlists.rows = [
            { playlist: { id: 1, name: "Evening music", mixed: true, track_count: 8 }, icon: "chilpa", accepts: true, same_source: true, exact: 1, similar: 0 },
            { playlist: { id: 3, name: "Morning music", mixed: true, track_count: 2 }, icon: "chilpa", accepts: true, same_source: true, exact: 0, similar: 0 },
            { playlist: { id: -5, name: "Cloud music", mixed: false, source: "com.test.cloud", track_count: 20 }, icon: "", accepts: true, same_source: false, exact: 0, similar: 0 },
            { playlist: { id: 2, name: "Road music", mixed: false, track_count: 12 }, icon: "", accepts: false, same_source: false, exact: 0, similar: 0 }
        ]
    }
    function cleanup() { picker.close(); wait(0) }
    function child(name) { return findChild(picker.contentItem, name) }
    function list() { return child("playlistTargets") }

    function test_the_search_opens_the_writable_playlists_with_their_marks() {
        picker.show("[42,43]")
        tryCompare(picker, "opened", true)
        compare((picker.trackCount, picker.creating), false)
        verify(!list().visible, "the list waits for the search")
        child("playlistSearch").focusInput()
        tryCompare(list(), "visible", true)
        compare(list().count, 3, "the read-only one is not offered")
        compare(list().itemAtIndex(0).target.icon, "chilpa")
        child("playlistSearch").text = "ROAD"
        tryCompare(list(), "count", 0)
        child("playlistSearch").text = "cloud"
        tryCompare(list(), "count", 1)
    }

    function test_already_in_says_how_many_of_the_tracks_a_playlist_holds() {
        picker.show("[42,43]")
        tryCompare(picker, "opened", true)
        const column = child("playlistAlreadyIn")
        verify(column.visible)
        compare(column.rows.length, 1)
        compare(column.rows[0].playlist.name, "Evening music")
        picker.show("[42]")
        compare(child("playlistAlreadyIn").rows.length, 1)
    }

    function test_a_choice_is_added_by_the_footer_and_repeats_are_asked_first() {
        picker.show("[42,43]")
        tryCompare(picker, "opened", true)
        const add = child("addToPlaylist")
        compare(add.enabled, false, "nothing chosen yet")
        picker.choose(Playlists.rows[1])
        compare(add.enabled, true)
        picker.confirm()
        compare(Playlists.added, "[42,43]")
        compare(Playlists.repeated, false)
        tryCompare(picker, "visible", false)
        compare(Playlists.lastTarget, 3)

        Playlists.added = ""
        picker.show("[42,43]")
        picker.choose(Playlists.rows[0])
        picker.confirm()
        compare(Playlists.added, "", "a playlist that holds some of them asks first")
        verify(child("playlistRepeats").visible)
        const every = child("addEveryTrack")
        waitForRendering(every)
        mouseClick(every, every.width / 2, every.height / 2)
        compare(Playlists.added, "[42,43]")
        compare(Playlists.repeated, true)
    }

    function test_the_playlist_last_added_to_is_pinned_above_the_rest() {
        Playlists.lastTarget = 3
        picker.show("[42]")
        tryCompare(picker, "opened", true)
        compare(picker.lastUsed.playlist.id, 3)
        child("playlistSearch").focusInput()
        tryCompare(list(), "visible", true)
        compare(list().count, 2, "and not listed twice")
    }

    function test_only_the_tracks_own_source_when_asked() {
        picker.show("[42]")
        tryCompare(picker, "opened", true)
        compare(picker.shown.length, 3)
        const toggle = child("sameSourceOnly")
        waitForRendering(toggle)
        mouseClick(toggle, toggle.width / 2, toggle.height / 2)
        compare(picker.sameSource, true)
        compare(picker.shown.length, 2)
        verify(picker.shown.every(row => row.same_source))
        compare(SettingsModel.kept("ui.playlists.sameSource"), "true")
    }

    function test_a_new_playlist_is_named_in_place_and_takes_the_tracks() {
        picker.show("[42,43]")
        tryCompare(picker, "opened", true)
        child("playlistSearch").focusInput()
        tryCompare(list(), "visible", true)
        const create = findChild(list(), "createPlaylistRow")
        waitForRendering(create)
        mouseClick(create, create.width / 2, create.height / 2)
        compare(picker.naming, true)
        const add = child("addToPlaylist")
        compare(add.enabled, false)
        child("newPlaylistName").text = "Fresh mix"
        compare(add.enabled, true)
        picker.confirm()
        compare(Playlists.created[0], "Fresh mix")
        compare(Playlists.created[5], "[42,43]")
        tryCompare(picker, "visible", false)
    }

    function test_a_source_playlist_is_written_by_its_source() {
        picker.show("[-7,-8]")
        tryCompare(picker, "opened", true)
        picker.choose(Playlists.rows[2])
        picker.confirm()
        verify(picker.pending)
        compare(Playlists.remoteWrite[0], "com.test.cloud")
        compare(Playlists.remoteWrite[1], -5)
        compare(Playlists.remoteWrite[4], "[-7,-8]")
        Playlists.remoteState = {request_id: Playlists.remoteState.request_id, pending: false, result: {}}
        tryCompare(picker, "pending", false)
        tryCompare(picker, "visible", false)
    }

    function test_without_tracks_it_is_the_new_playlist_form() {
        picker.show("[]")
        tryCompare(picker, "opened", true)
        compare(picker.creating, true)
        const submit = child("createPlaylist")
        compare(submit.enabled, false)
        const field = findChild(picker.contentItem, "playlistName")
        verify(field !== null)
    }

    // The New playlist chooser: as wide as its field, each source with its
    // icon in the gutter and a second line wrapped whole, a greyed one with
    // its reason there and not taken, the chosen one marked, and the
    // keyboard kept on the field.
    function test_a_source_it_cannot_take_says_why_in_full_and_cannot_be_chosen() {
        const why = "Still syncing its library for the first time, which takes a while on a big library"
        Playlists.sourceChoices = [{id: "plex:77aa", name: "MacMini Plex", icon: "plex", accepts: true,
                                    detail: "Kept in Chilpa until you add its first tracks, then sent to MacMini Plex"},
                                   {id: "jellyfin:4f1c", name: "Living room", icon: "jellyfin", accepts: false, why: why},
                                   {id: "jellyfin:0a0b", name: "Attic", icon: "jellyfin", accepts: false}]
        picker.show("[]")
        tryCompare(picker, "opened", true)
        const field = findChild(picker.contentItem, "playlistSourceChooser")
        const menu = findChild(field, "selectOptions")
        verify(menu !== null)
        compare(menu.entries.length, 4)
        compare(menu.entries[0].leadingSource.chilpa, true, "Chilpa's own mark for Local")
        compare(menu.entries[1].leadingSource.icon, "plex", "each server its icon")
        compare(menu.entries[2].enabled, false, "greyed")
        compare(menu.entries[2].detail, why, "the reason is its second line")
        compare(menu.entries[3].detail, "Only tracks from this source go in its playlists")
        compare(menu.entries[0].checked, true, "the chosen one is marked")
        // The modal places its fields on the next frame.
        waitForRendering(picker.contentItem)
        mouseClick(field, field.width / 2, field.height / 2)
        tryCompare(menu, "opened", true)
        compare(menu.width, field.width, "as wide as the field")
        const rows = findChild(menu.contentItem, "menuOptions")
        tryVerify(() => rows.itemAtIndex(2) !== null)
        const greyed = rows.itemAtIndex(2)
        verify(greyed.height >= 48, "a row stands at least 48: " + greyed.height)
        verify(!greyed.cut, "nothing is cut")
        menu.choose(2, 0)
        compare(picker.sourceId, "", "a greyed source is not taken")
        menu.close()
        tryCompare(menu, "opened", false)
        field.forceActiveFocus()
        keyClick(Qt.Key_Space)
        tryCompare(menu, "opened", true)
        verify(field.activeFocus, "the keyboard stays on the field")
        keyClick(Qt.Key_Down)
        compare(field.highlight, 1)
        keyClick(Qt.Key_Down)
        compare(field.highlight, 1, "a greyed one is stepped over, and there is none after")
        keyClick(Qt.Key_Return)
        tryCompare(menu, "opened", false)
        compare(picker.sourceId, "plex:77aa")
        compare(findChild(picker.contentItem, "playlistSourceLine").text, "Kept in Chilpa until you add its first tracks, then sent to MacMini Plex", "the chosen one's line stays under it")
    }

    // A playlist waiting for its first tracks says these are the ones.
    function test_a_playlist_waiting_for_its_first_tracks_says_so_in_the_list() {
        Playlists.rows = [{ playlist: { id: 9, name: "Night", mixed: false, source: "plex:77aa", track_count: 0, unsent: true }, icon: "plex", accepts: true, same_source: true, exact: 0, similar: 0 }]
        picker.show("[42]")
        tryCompare(picker, "opened", true)
        child("playlistSearch").focusInput()
        tryCompare(list(), "visible", true)
        tryVerify(() => list().itemAtIndex(0) !== null)
        verify(list().itemAtIndex(0).target.playlist.unsent)
    }
}
