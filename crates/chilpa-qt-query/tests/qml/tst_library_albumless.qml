import QtQuick
import QtTest
import app.chilpa
import "../../../chilpa-qt/qml/Screens"
import "../../../chilpa-qt/qml/Components"

TestCase {
    name: "LibraryAlbumless"
    when: windowShown
    visible: true
    width: 1200; height: 700
    Component { id: factory; LibraryScreen {} }
    TrackListModel { id: toolbarTracks; summary: "A very long summary of this collection that must fit beside its controls" }
    Component { id: toolbarFactory; LibraryToolbar { tracks: toolbarTracks } }
    Loader { id: page; anchors.fill: parent; active: false; sourceComponent: factory }
    function init() { Library.albumless = false; Library.busy = false; SettingsModel.libraryView = "grid"; page.active = true }
    function cleanup() { page.active = false; Library.albumless = false; Library.busy = false }

    function test_loose_tracks_use_the_library_model_and_leave_no_hidden_list() {
        tryVerify(() => page.item !== null)
        const toggle = findChild(page.item, "albumlessToggle")
        const loader = findChild(page.item, "albumlessList")
        const albums = findChild(page.item, "libraryAlbums")
        verify(toggle !== null && loader !== null && albums !== null)
        compare(loader.active, false)
        mouseClick(toggle)
        tryCompare(Library, "albumless", true)
        tryVerify(() => loader.item !== null)
        compare(albums.visible, false)
        const list = loader.item
        compare(list.model.kind, "library")
        compare(list.showAlbum, false)
        list.model.clear()
        for (let at = 0; at < 200; ++at) {
            list.model.append({trackId: at + 1, rowKind: 0, number: 0, title: "Loose " + at, artist: "Solo", album: "",
                sourceName: "", duration: "0:01", plays: 0, quality: "WAV", tier: 2, available: true, playing: false,
                selected: false, headerLabel: "", headerMeta: "", art: ""})
        }
        tryCompare(list.rows, "count", 200)
        list.rows.positionViewAtEnd()
        tryVerify(() => list.rows.itemAtIndex(199) !== null)
        const delegate = list.rows.itemAtIndex(199)
        const position = list.rows.contentY
        Library.busy = true
        compare(loader.item, list, "refresh keeps the same list")
        compare(list.rows.itemAtIndex(199), delegate, "refresh keeps its delegates")
        Library.busy = false
        Library.rev++
        wait(0)
        compare(loader.item, list)
        compare(list.rows.contentY, position, "an equivalent refresh preserves scroll")
        list.model.play(199)
        compare(list.model.lastAction, "play")
        compare(list.model.lastRow, 199)
        list.model.queueAll()
        compare(list.model.lastAction, "queue")
        list.model.clear()
        compare(list.rows.count, 0)
        verify(toggle.visible && toggle.enabled, "an empty refresh must not trap the listener in the loose-track view")
        mouseClick(toggle)
        tryCompare(Library, "albumless", false)
        tryVerify(() => loader.item === null)
        tryCompare(albums, "visible", true)
    }

    function test_pending_filter_does_not_show_the_previous_album_snapshot_as_loose_tracks() {
        Library.busy = true
        Library.albumless = true
        const loader = findChild(page.item, "albumlessList")
        compare(loader.active, false)
        Library.busy = false
        compare(loader.active, false, "publication must finish before the first list is constructed")
        Library.rev++
        tryVerify(() => loader.item !== null)
        const list = loader.item
        Library.busy = true
        compare(loader.item, list)
        Library.busy = false
        wait(0)
        compare(loader.item, list, "an unchanged snapshot can complete without incrementing rev")
    }

    function test_toolbar_wraps_the_collection_and_elides_the_summary() {
        const bar = createTemporaryObject(toolbarFactory, this)
        verify(bar !== null)
        const actions = findChild(bar, "libraryActions")
        const play = actions.children[0]
        const shuffle = actions.children[1]
        const toggle = findChild(bar, "albumlessToggle")
        const views = findChild(bar, "libraryViewTools")
        const summary = findChild(bar, "libraryTrackSummary")
        const intersects = (a, b) => {
            const p = a.mapToItem(bar, 0, 0), q = b.mapToItem(bar, 0, 0)
            return p.x < q.x + b.width && q.x < p.x + a.width && p.y < q.y + b.height && q.y < p.y + a.height
        }
        for (const labels of [["Play", "Shuffle", "No album tracks"], ["Reproducir", "Mezclar", "Pistas sin álbum"],
                              ["Wiedergeben", "Zufallswiedergabe", "Musiktitel ohne Albumzuordnung"]]) {
            play.text = labels[0]; shuffle.text = labels[1]; toggle.text = labels[2]
            for (const width of [304, 360, 480, 700, 1100]) {
                bar.width = width
                for (const mode of [false, true]) {
                    bar.albumless = mode
                    for (const presentation of ["grid", "tracks"]) {
                        SettingsModel.libraryView = presentation
                        wait(0)
                        verify(summary.x + summary.width <= width, "summary stays inside the toolbar")
                        for (const item of [play, shuffle, toggle, views]) {
                            if (!item.visible) continue
                            const point = item.mapToItem(bar, 0, 0)
                            verify(point.x >= 0 && point.x + item.width <= width, "controls stay reachable at " + width)
                            verify(point.y + item.height <= bar.height, "toolbar reserves every band")
                        }
                        verify(!intersects(play, shuffle), "playback controls wrap independently")
                        verify(!intersects(actions, toggle), "the collection does not overlap playback")
                        if (views.visible) {
                            verify(!intersects(actions, views), "playback controls do not overlap view choices")
                            verify(!intersects(toggle, views), "collection and view choices use separate bands when necessary")
                            verify(!intersects(summary, views), "summary does not overlap view choices")
                        }
                    }
                }
            }
        }
    }
}
