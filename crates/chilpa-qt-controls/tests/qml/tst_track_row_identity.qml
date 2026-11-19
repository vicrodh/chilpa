import QtQuick
import QtTest
import app.chilpa
import "../../../chilpa-qt/qml/Components"

// A row acts on the track it shows: a slot whose track left the list holds
// no other track's values and takes no action.
TestCase {
    id: test
    name: "TrackRowIdentity"
    when: windowShown
    visible: true
    width: 1000
    height: 400
    TrackListModel { id: tracks }
    TrackList { id: list; width: 1000; height: 250; model: tracks; rich: true; reorderable: true }
    TrackMenu { id: menu; model: tracks }

    function track(i) {
        return { trackId: 42 + i, rowKind: 0, number: i + 1, title: "Track " + i, artist: "Band", album: "Album", duration: "3:00", plays: 2,
            quality: "FLAC", tier: 2, available: true, playing: i === 0, selected: i === 0, headerLabel: "", headerMeta: "", art: "", missing: false }
    }
    // What the bridge publishes for a snapshot slot whose track is gone (`track_roles::missing`).
    function hole() {
        return { trackId: 0, rowKind: 0, number: 0, title: "", artist: "", album: "", duration: "", plays: 0, quality: "", tier: 0,
            available: false, playing: false, selected: false, headerLabel: "", headerMeta: "", art: "", missing: true }
    }
    function cleanup() { menu.close() }
    function init() {
        Ratings.tracks = ({}); Ratings.favorites = ({}); Ratings.fail = false; ++Ratings.rev
        list.height = 250
        list.rich = true
        tracks.clear()
        tracks.append(track(0))
        tracks.lastAction = ""
        tracks.selectedAt = -1
        list.cursorRow = -1
        wait(30)
    }
    function checkHole(item) {
        verify(item !== null)
        compare([item.trackId, item.title, item.artist, item.album, item.duration, item.plays, item.quality, item.tier], [0, "", "", "", "", 0, "", 0])
        compare([item.available, item.playing, item.selected, item.canRate, item.rating, item.favorite, item.end.visible],
            [false, false, false, false, 0, false, false])
    }
    // Every pointer action a row offers, on that row.
    function actOn(item) {
        const y = item.height / 2, x = item.cols.titleX + 3, revision = Ratings.rev
        mouseClick(item, item.cols.ratingX + 3, y)
        mouseClick(item, item.cols.moreX + 3, y)
        mouseClick(item, item.cols.moreX + item.cols.favorite + 3, y)
        mouseClick(item, x, y)
        mouseDoubleClickSequence(item, x, y)
        mouseClick(item, x, y, Qt.RightButton)
        compare(Ratings.rev, revision, "a missing row writes no rating or favorite")
        compare(tracks.lastAction, "", "a missing row selects, plays and opens nothing")
        compare(list.cursorRow, -1, "the cursor does not land on a missing row")
    }

    function test_a_shown_track_that_becomes_a_hole_drops_its_values_and_actions() {
        const item = list.rows.itemAtIndex(0)
        compare([item.trackId, item.title, item.playing, item.selected], [42, "Track 0", true, true])
        tracks.set(0, hole())
        tryCompare(item, "trackId", 0)
        checkHole(item)
        actOn(item)
        mousePress(item, item.cols.titleX + 3, item.height / 2)
        mouseMove(item, item.cols.titleX + 3, item.height / 2 + 30)
        mouseRelease(item, item.cols.titleX + 3, item.height / 2 + 30)
        compare(tracks.lastAction, "", "a missing row cannot be dragged")
    }
    function test_a_recycled_delegate_landing_on_a_hole_drops_the_track_it_showed() {
        list.height = 70
        for (let i = 1; i < 40; ++i) tracks.append(i === 39 ? hole() : track(i))
        wait(30)
        // Scrolled a step at a time, as a wheel does: rows that leave go back to the pool.
        const built = []
        for (let step = 0; step < 200 && list.rows.itemAtIndex(39) === null; ++step) {
            for (let i = 0; i < tracks.count; ++i) if (list.rows.itemAtIndex(i) && built.indexOf(list.rows.itemAtIndex(i)) < 0) built.push(list.rows.itemAtIndex(i))
            list.rows.contentY += 20
            wait(5)
        }
        const item = list.rows.itemAtIndex(39)
        verify(built.indexOf(item) >= 0, "the hole reuses a delegate that showed a track")
        checkHole(item)
        list.height = 250
        tryVerify(() => list.rows.itemAtIndex(39) === item && item.y + item.height <= list.rows.contentY + list.rows.height)
        actOn(item)
    }
    function test_an_open_menu_closes_when_rows_move_under_it() {
        for (const change of ["remove", "insert", "move", "reset"]) {
            tracks.clear()
            for (let i = 0; i < 4; ++i) tracks.append(track(i))
            tracks.lastAction = ""
            menu.show(2, 100, 100)
            tryCompare(menu, "opened", true, 1000, change)
            if (change === "remove") tracks.remove(0)
            else if (change === "insert") tracks.insert(0, track(9))
            else if (change === "move") tracks.move(0, 3, 1)
            else tracks.modelReset()
            tryCompare(menu, "visible", false, 1000, change + ": row 2 holds another track now, so its menu cannot act")
            compare(tracks.lastAction, "", change)
        }
        menu.show(2, 100, 100)
        tryCompare(menu, "opened", true)
        tracks.setProperty(2, "plays", 7)
        wait(30)
        verify(menu.visible, "a value that changes in place keeps the row's track and its menu")
    }
    function test_the_cursor_follows_the_selected_track_through_a_reset() {
        list.rich = false
        for (let i = 1; i < 6; ++i) tracks.append(track(i))
        wait(30)
        const item = list.rows.itemAtIndex(3)
        mouseClick(item, item.cols.titleX + 3, item.height / 2)
        compare(list.cursorRow, 3)
        tracks.remove(0, 2)
        tracks.selectedAt = 1
        tracks.modelReset()
        compare(list.cursorRow, 1, "the cursor stays on its track, now at row 1")
        tracks.selectedAt = -1
        tracks.modelReset()
        compare(list.cursorRow, -1, "with no single selected track to follow the cursor clears")
    }
}
