import QtQuick
import QtTest
import "../../qml/Components"
import "../../qml/Primitives"

TestCase {
    name: "AlbumInteractions"
    when: windowShown
    visible: true
    width: 1000
    height: 400

    TrackColumns { id: columns; width: 1000; rich: true; showArt: false }
    TrackRow {
        id: row
        cols: columns
        index: 0; rowKind: 0; number: 1; title: "A track"; artist: "Band"
        album: "Album"; duration: "3:00"; plays: 0; quality: "FLAC"; tier: 2
        available: true; trackId: 1
        playing: false; selected: false; headerLabel: ""; headerMeta: ""; art: ""
    }
    QtObject {
        id: detail
        property string versions: JSON.stringify([
            { id: 1, label: "Local · FLAC 24/96 · 13 tracks", source: { local: true }, detail: "Version one" },
            { id: 2, label: "Home · FLAC 16/44.1 · 13 tracks", source: { local: false }, detail: "Version two" }
        ])
        property int selectedId: 1
        function selectVersion(value) { selectedId = Number(value) }
    }
    AlbumVersionSelect { id: selector; y: 100; detail: detail }
    Menu {
        id: versionsMenu
        widthLimit: 560
        entries: [{ text: "Local library · FLAC · 24-bit / 44.1 kHz · 13 tracks", sourceInfo: { local: true }, detail: "A Hard Day's Night" }]
    }

    function init() {
        columns.rich = true
        columns.width = 1000
        row.rowKind = 0
        row.artist = "Band"
        detail.selectedId = 1
        wait(0)
    }

    function artistY(x) {
        for (let y = 0; y < row.height; ++y)
            if (row.containsArtist(x, y)) return y
        return -1
    }

    function test_rich_link_is_only_the_rendered_text() {
        const x = columns.titleX + 2
        const y = artistY(x)
        verify(y >= 0, "the artist text remains a link")
        verify(!row.containsArtist(columns.titleX + columns.titleW - 20, y), "blank title-column space plays the track")
        verify(!row.containsArtist(x, 0), "the top padding is not a link")
        verify(!row.containsArtist(x, row.height - 1), "the bottom padding is not a link")
        row.artist = ""
        compare(artistY(x), -1)
    }

    function test_compact_link_and_elision() {
        columns.rich = false
        wait(0)
        const x = columns.artistX + 2
        const y = artistY(x)
        verify(y >= 0)
        verify(!row.containsArtist(columns.artistX + columns.artistW - 20, y))
        row.artist = "An artist name long enough to fill the entire column and become elided"
        wait(0)
        verify(!row.containsArtist(columns.artistX + columns.artistW, y))
        row.rowKind = 1
        compare(artistY(x), -1, "a disc header is never an artist link")
    }

    function test_version_selector_preserves_source_and_selection() {
        compare(selector.values.length, 2)
        compare(selector.current, 0)
        compare(selector.sources[0].local, true)
        compare(selector.sources[1].local, false)
        selector.picked("2")
        compare(detail.selectedId, 2)
        compare(selector.current, 1)
    }

    function test_version_menu_measures_after_its_source_badge_and_text() {
        versionsMenu.open()
        tryCompare(versionsMenu, "opened", true)
        let rows = 0
        for (let item of versionsMenu.entryItems) {
            if (item.entry !== undefined) {
                ++rows
                verify(!item.cut, "the track count must fit without a tooltip")
            }
        }
        compare(rows, 1)
        versionsMenu.close()
    }
}
