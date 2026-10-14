import QtQuick
import QtTest
import "../../qml/Components"
import "../../../chilpa-qt-kit/qml/Primitives"
import "../../../chilpa-qt-kit/qml/Components"
import "../../../chilpa-qt-kit/qml/Theme"

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
        detail.versions = JSON.stringify([
            { id: 1, label: "Local", source: { local: true } },
            { id: 2, label: "Home", source: { local: false } }
        ])
        detail.selectedId = 1
        wait(0)
    }

    function artistY(x) {
        for (let y = 0; y < row.height; ++y)
            if (row.containsArtist(x, y)) return y
        return -1
    }
    function containsAlbum(x, y) {
        const label = row.albumText
        const point = label.mapFromItem(row, x, y)
        return !row.header && label.visible && point.x >= 0 && point.y >= 0
            && point.x < Math.min(label.width, label.contentWidth) && point.y < label.height
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

    function test_album_link_and_source_column_in_both_densities() {
        for (const rich of [false, true]) {
            columns.rich = rich
            columns.showSource = true
            row.sourceName = "Remote source"
            wait(0)
            verify(containsAlbum(columns.albumX + 2, row.height / 2))
            verify(!containsAlbum(columns.albumX + columns.albumW - 1, row.height / 2))
            compare(columns.playsX, columns.sourceX + columns.sourceWidth)
            verify(columns.sourceWidth > 0)
            row.rowKind = 1
            verify(!containsAlbum(columns.albumX + 2, row.height / 2))
            row.rowKind = 0
        }
        columns.showSource = false
    }

    // A header is a divider, with the group's cover when it has one, or it
    // stands for a record among several, with its cover and its artist.
    function test_a_header_is_a_divider_or_stands_for_a_record() {
        row.rowKind = 1
        row.headerLabel = "Disc 2 \u2014 Live"
        row.headerMeta = "9 songs"
        row.art = ""
        wait(0)
        const head = findChild(row, "groupHeader")
        verify(head, "a header row builds its header")
        verify(row.header && !row.record)
        compare(row.height, Metrics.rowGroup)
        verify(!head.record && !head.covered)
        compare(head.label, "Disc 2 \u2014 Live")
        row.art = "file:///covers/disc-2.jpg"
        wait(0)
        verify(head.covered, "a disc with its own cover shows it")

        row.rowKind = 2
        row.headerLabel = "Box \u2014 Live"
        row.headerMeta = "1990 \u00b7 Disc 2 \u00b7 9 tracks"
        wait(0)
        verify(row.header && row.record)
        compare(row.height, Metrics.rowRecord)
        const record = findChild(row, "groupHeader")
        verify(record.record)
        compare(record.artist, row.artist)
        compare(record.width, row.width - columns.indexX, "the list positions group actions over the header")

        row.rowKind = 0
        row.art = ""
        wait(0)
        compare(findChild(row, "groupHeader"), null, "a track builds no header")
    }

    // The fixed columns close the row in one order everywhere: time, stars,
    // format, and then the heart and the menu.
    function test_a_row_ends_with_time_rating_format_and_its_actions() {
        for (const rich of [false, true]) {
            columns.rich = rich
            columns.showSource = true
            columns.showPlays = true
            wait(0)
            compare(columns.timeX, columns.playsX + columns.plays)
            verify(columns.ratingX > columns.timeX + columns.time, "the stars do not touch the time")
            compare(columns.qualityX, columns.ratingX + columns.rating)
            compare(columns.moreX, columns.qualityX + columns.quality)
            compare(columns.moreX + columns.more, columns.width - columns.pad)
            columns.showRating = false
            wait(0)
            verify(columns.qualityX > columns.timeX + columns.time, "without stars the format follows the time")
            compare(columns.qualityX, columns.ratingX)
            columns.showRating = true
        }
        columns.showSource = false
        columns.showPlays = false
    }

    function test_version_selector_preserves_source_and_selection() {
        compare(selector.item.values.length, 2)
        compare(selector.item.current, 0)
        compare(selector.item.sources[0].local, true)
        compare(selector.item.sources[1].local, false)
        selector.item.picked("2")
        compare(detail.selectedId, 2)
        compare(selector.item.current, 1)
    }

    function test_single_version_builds_no_field_or_menu() {
        detail.versions = JSON.stringify([{ id: 1, label: "Local", detail: "An edition" }])
        compare(selector.active, false)
        compare(selector.visible, false)
        compare(selector.item, null)
        wait(0)
        compare(findChild(selector, "selectOptions"), null)
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
