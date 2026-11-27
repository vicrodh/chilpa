import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.folders
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Patterns"

// An album taken out of the library is asked about first and goes only
// once confirmed; Settings > Library opens the list of removed albums, and
// each of them comes back from there.
TestCase {
    name: "RemovedAlbums"
    when: windowShown
    visible: true
    width: 960
    height: 700
    RemovedAlbumsHost { id: host }

    function confirmation() { return findChild(host, "albumRemovalConfirmation") }
    function list() { return findChild(host, "removedAlbums") }

    function init() {
        RemovedAlbums.removedIds = []
        RemovedAlbums.restoredIds = []
        RemovedAlbums.albums = []
    }

    function test_an_album_goes_only_once_its_removal_is_confirmed() {
        RemovedAlbums.ask(5, "Disco")
        tryCompare(confirmation(), "opened", true)
        verify(confirmation().title.indexOf("Disco") >= 0, confirmation().title)
        compare(RemovedAlbums.removedIds.length, 0, "nothing goes before the answer")
        findChild(confirmation().contentItem, "confirmationAccept").clicked()
        compare(RemovedAlbums.removedIds, [5])
    }

    function test_a_source_s_record_is_not_asked_about() {
        RemovedAlbums.ask(-3, "Streamed")
        wait(50)
        verify(!confirmation().opened)
    }

    function test_settings_opens_the_list_and_an_album_comes_back_from_it() {
        RemovedAlbums.albums = [{id: 7, title: "Disco", artist: "Artist", artwork_path: "", tracks: 9, removed_at: 1000},
                                {id: 8, title: "Other", artist: "", artwork_path: "", tracks: 1, removed_at: 900}]
        RemovedAlbums.requested = true
        Prefs.rev = Prefs.rev + 1
        tryCompare(list(), "opened", true)
        const rows = findChild(list().contentItem, "removedAlbumRows")
        tryCompare(rows, "count", 2)
        verify(!findChild(list().contentItem, "removedAlbumsEmpty").visible)
        const restore = findChild(rows.itemAtIndex(0), "restoreAlbum")
        restore.clicked()
        compare(RemovedAlbums.restoredIds, [7])
        tryCompare(rows, "count", 1)
        list().close()
    }

    function test_an_empty_list_says_so() {
        RemovedAlbums.requested = true
        Prefs.rev = Prefs.rev + 1
        tryCompare(list(), "opened", true)
        verify(findChild(list().contentItem, "removedAlbumsEmpty").visible)
        list().close()
    }
}
