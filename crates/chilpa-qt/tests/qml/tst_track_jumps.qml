import QtQuick
import QtTest
import app.chilpa
import "../../qml/Components"
import "../../../chilpa-qt-kit/qml/Theme"

// The Tracks screen's list: twenty thousand rows, grouped by letter, that a
// strip of letters jumps through and a wheel or a scrollbar sweeps. A model
// this cheap does not show the window's blank jumps (they came from the
// view's estimate of far rows, measured in the window itself); it keeps
// the landings exact and the long numbers whole.
TestCase {
    name: "TrackJumps"
    when: windowShown
    visible: true
    width: 1000; height: 700
    TrackListModel { id: tracks }
    TrackList { id: list; width: parent.width; height: parent.height; header: false; model: tracks; rich: false }
    Component {
        id: band
        Text { required property string section; width: list.width; height: Metrics.rowGroup; text: section }
    }
    readonly property int total: 20000
    function initTestCase() {
        const letters = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
        for (let i = 0; i < total; i++)
            tracks.append({ trackId: i + 1, rowKind: 0, number: i + 1, title: "Track " + i, artist: "Band", album: "Record",
                sourceName: "", sourceInfo: "{}", composer: "", duration: "3:00", plays: 0, quality: "FLAC", tier: 2,
                available: true, playing: false, selected: false, headerLabel: "", headerMeta: "", art: "",
                group: letters[Math.floor(i * 26 / total)] })
        list.rows.section.delegate = band
    }
    function view() { return list.rows }
    // The rows drawn where the view looks: built, with their contents.
    function shown() {
        const v = view()
        let found = 0
        for (let y = 0; y < v.height; y += Metrics.rowCompact) {
            const row = v.indexAt(Metrics.gap2, v.contentY + y)
            const item = row >= 0 ? v.itemAtIndex(row) : null
            if (item && item.visible && findChild(item, "trackRowBody")) found++
        }
        return found
    }
    function jumpAndCheck(row) {
        view().positionViewAtIndex(row, ListView.Beginning)
        tryVerify(() => shown() > 5, 2000, "rows are drawn after jumping to " + row + ", found " + shown())
        const top = view().indexAt(Metrics.gap2, view().contentY + 1)
        verify(Math.abs(top - row) <= 2, "the jump lands on its row: wanted " + row + ", top is " + top)
    }
    function test_letters_land_on_drawn_rows_with_sections() {
        list.rows.section.property = "group"
        for (const row of [1538, 12307, 1538, 19230, 2307, 14615])
            jumpAndCheck(row)
    }
    function test_letters_land_on_drawn_rich_rows_with_sections() {
        list.rich = true
        list.rows.section.property = "group"
        for (const row of [1538, 12307, 1538, 19230, 2307, 14615])
            jumpAndCheck(row)
        list.rich = false
    }
    function test_letters_land_on_drawn_rows_without_sections() {
        list.rows.section.property = ""
        for (const row of [1538, 12307, 1538, 19230, 2307, 14615])
            jumpAndCheck(row)
    }
    function test_a_five_digit_number_fits_its_column() {
        list.rows.section.property = ""
        jumpAndCheck(14615)
        const row = list.rows.itemAtIndex(14615)
        const number = findChild(row, "trackNumber")
        verify(number, "the row draws its number")
        verify(number.contentWidth <= number.width + 1, "14616 fits: " + number.contentWidth + " in " + number.width)
    }
    function test_a_fast_sweep_keeps_rows_drawn() {
        list.rows.section.property = "group"
        const v = view()
        v.positionViewAtBeginning()
        for (let step = 0; step < 60; step++) {
            v.contentY += v.height * 3
            wait(0)
        }
        tryVerify(() => shown() > 5, 2000, "rows are drawn after a sweep, found " + shown())
    }
}
