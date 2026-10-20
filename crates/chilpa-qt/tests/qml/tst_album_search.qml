import QtQuick
import QtTest
import app.chilpa
import "../../qml/Screens"

// A record's rows are found by what is typed above them, once the typing
// stops, and a new record starts with nothing typed.
TestCase {
    name: "AlbumTrackSearch"
    when: windowShown
    visible: true
    width: 1200; height: 800

    AlbumScreen { id: screen; width: 1200; height: 800 }

    function test_what_is_typed_reaches_the_rows_and_a_new_record_clears_it() {
        const field = findChild(screen, "albumTrackSearch")
        const rows = findChild(screen, "albumTracks")
        verify(field); verify(rows)
        field.text = "birks"
        tryCompare(rows, "key", "birks")
        field.text = "  birks of  "
        tryCompare(rows, "key", "birks of", 2000, "trimmed once the typing stops")
        screen.albumId = 42
        compare(field.text, "")
        tryCompare(rows, "key", "")
    }
}
