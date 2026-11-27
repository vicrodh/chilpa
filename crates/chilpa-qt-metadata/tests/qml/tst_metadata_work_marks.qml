import QtQuick
import QtTest
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Patterns"

// The editor's first column marks each track's work: the same work, the same
// colours; another work, others; no work, no mark. Its name shows on hover,
// and a double click opens the track's fields with the keyboard in its work.
TestCase {
    id: test
    name: "MetadataWorkMarks"
    when: windowShown
    visible: true
    width: 1100; height: 700

    function makeField(name, value) { return { name: name, value: value, mixed: false, dirty: false, overridden: false, editable: true, invalid: "" } }
    function makeTrack(id, title, work) {
        const fields = [makeField("title", title), makeField("artist_credit", "Orchestra"), makeField("track_number", String(id)), makeField("disc_number", "1"), makeField("composer", "Someone")]
        if (work !== "") fields.push(makeField("work", work))
        return { id: id, file: "0" + id + ".flac", inside_a_disc: false, fields: fields }
    }

    MetadataTracks {
        id: tracks
        width: test.width
        height: test.height
        doc: ({ focus: 0, save: {}, tracks: [makeTrack(1, "Allegro", "Symphony No. 5"), makeTrack(2, "Andante", "Symphony No. 5"),
                                               makeTrack(3, "Kyrie", "Requiem"), makeTrack(4, "Encore", "")] })
    }

    // The work cell of the row that shows track `index`.
    function cellOf(index) {
        const walk = item => {
            if (!item) return null
            if (item.objectName === "metadataWorkCell") {
                let row = item.parent
                while (row && row.track === undefined) row = row.parent
                if (row && row.track && row.track.id === index + 1) return item
            }
            for (const child of item.children) { const hit = walk(child); if (hit) return hit }
            return null
        }
        return walk(tracks)
    }
    function markOf(index) { return findChild(cellOf(index), "metadataWorkMark") }

    function test_one_work_one_mark_another_work_another() {
        tryVerify(() => cellOf(3) !== null)
        verify(markOf(0).visible && markOf(1).visible && markOf(2).visible)
        verify(!markOf(3).visible, "a track of no work has no mark")
        compare(markOf(0).color, markOf(1).color, "the same work, the same colours")
        verify(markOf(0).color !== markOf(2).color, "another work, other colours")
        compare(markOf(0).color, Qt.color(Theme.workMarks[0].bg), "works are numbered in the order they first appear")
        compare(markOf(2).color, Qt.color(Theme.workMarks[1].bg))
    }

    function test_the_work_s_name_shows_on_hover() {
        tryVerify(() => cellOf(2) !== null)
        mouseMove(cellOf(2), cellOf(2).width / 2, cellOf(2).height / 2)
        tryCompare(TipState, "text", "Requiem")
        mouseMove(tracks, 1, 1)
    }

    function test_a_double_click_opens_the_track_s_work_ready_to_type() {
        tryVerify(() => cellOf(2) !== null)
        mouseDoubleClickSequence(cellOf(2), cellOf(2).width / 2, cellOf(2).height / 2)
        tryCompare(tracks, "detailsOpen", true)
        compare(tracks.chosen, 2)
        tryVerify(() => {
            const focused = test.Window.activeFocusItem
            let item = focused
            while (item && item.name === undefined) item = item.parent
            return item && item.name === "work"
        }, 2000, "the work field has the keyboard")
    }

    function test_sixteen_marks_tell_works_apart() {
        compare(Theme.workMarks.length, 16)
        const fills = Theme.workMarks.map(mark => mark.bg)
        compare(new Set(fills).size, 16, "no two marks share a fill")
    }
}
