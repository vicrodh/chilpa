import QtQuick
import QtTest
import app.chilpa
import app.chilpa.controls
import "../../../chilpa-qt-kit/qml/Theme"
import "../../qml/Components"

// The information modal in sections: each one only when there is something
// to put in it, in columns that give way on a narrow window, with where the
// file or the server entry is said in its own section.
TestCase {
    id: test
    name: "TrackInfoModal"
    when: windowShown
    visible: true
    width: 1000
    height: 800

    Component { id: modalFactory; TrackInfoModal {} }
    Item { id: narrow; width: 420; height: 700 }

    function answer(file) {
        return {loading: false, origin: "", link: "", heading: {title: "Chosen", album: "Record", artist: "Band"},
            facts: [{label: "Composer", value: "Writer", group: "details"}, {label: "Label", value: "House", group: "legal"},
                {label: "Copyright", value: "℗ 2026 House", group: "legal"}, {label: "ISRC", value: "XX0000000001", group: "legal"}],
            technical: [{label: "Format", value: "FLAC"}, {label: "Bit depth", value: "24"}, {label: "Sample rate", value: "96 kHz"},
                {label: "Channels", value: "2"}, {label: "Duration", value: "4:01"}],
            credits: [{name: "Player", roles: "Guitar"}, {name: "Singer", roles: "Vocals"}, {name: "Engineer", roles: "Mixing"}, {name: "Producer", roles: "Producer"}],
            file: file, copy_basic: "Chosen - Record - Band", copy_full: "Chosen\nRecord\nBand"}
    }
    function init() {
        Player.path = "/music/Playing.flac"
        Player.title = "Playing"
        NowAbout.trackAnswer = ({loading: false, facts: [], credits: []})
        NowAbout.trackAnswers = ({"/music/Chosen.flac": answer({title: "File", value: "/music/Chosen.flac", note: ""})})
        NowAbout.albumAnswer = ({loading: false, facts: [], credits: []})
    }
    function open(path, title, host) {
        const modal = createTemporaryObject(modalFactory, test, Object.assign({subject: path, subjectTitle: title}, host ? {parent: host} : {}))
        verify(modal)
        modal.open()
        tryCompare(modal, "opened", true)
        return modal
    }

    function test_a_local_track_shows_each_section_in_three_columns() {
        const modal = open("/music/Chosen.flac", "Chosen")
        const technical = findChild(modal, "trackInfoTechnical")
        tryCompare(technical, "visible", true)
        compare(findChild(modal, "trackInfoTitle").text, "Chosen")
        compare(findChild(modal, "trackInfoAlbum").text, "Record")
        compare(findChild(modal, "trackInfoArtist").text, "Band")
        compare(technical.cells.map(cell => cell.label), ["Format", "Bit depth", "Sample rate", "Channels", "Duration"])
        compare(findChild(modal, "trackInfoDetails").cells.map(cell => cell.label), ["Composer"])
        compare(findChild(modal, "trackInfoLegal").cells.map(cell => cell.label), ["Label", "Copyright", "ISRC"])
        const credits = findChild(modal, "trackInfoCredits")
        compare(credits.cells.length, 4)
        compare(credits.columns, 3, "three columns at the modal's own width")
        tryCompare(findChild(findChild(modal, "trackInfoMedia"), "infoSectionCode"), "text", "/music/Chosen.flac")
        compare(findChild(modal, "trackInfoReview").visible, false)
        compare(findChild(modal, "trackInfoNothing").visible, false)
    }

    function test_a_missing_file_and_a_server_entry_are_said_in_media_info_and_empty_sections_are_left_out() {
        for (const file of [{title: "File", value: "/music/Gone.flac", note: "Not found on disk"}, {title: "Server item", value: "provider/home/7", note: ""}]) {
            NowAbout.trackAnswers = ({"/x": Object.assign(answer(file), {technical: [], credits: []})})
            const modal = open("/x", "X")
            const media = findChild(modal, "trackInfoMedia")
            tryVerify(() => findChild(media, "infoSectionCode") !== null)
            tryCompare(findChild(media, "infoSectionCode"), "text", file.value)
            compare([media.cells[0].label, media.cells[0].note], [file.title, file.note])
            compare(findChild(modal, "trackInfoTechnical").visible, false, "nothing technical is known: no empty section")
            compare(findChild(modal, "trackInfoCredits").visible, false)
            modal.close()
        }
    }

    function test_the_album_part_has_its_review_and_no_media_or_technical_data() {
        NowAbout.albumAnswer = ({loading: false, description: "A review", facts: [{label: "Label", value: "House", group: "legal"}], credits: []})
        const modal = open("/music/Chosen.flac", "Chosen")
        findChild(modal, "trackInfoSections").part = "album"
        tryCompare(findChild(modal, "trackInfoReview"), "visible", true)
        compare(findChild(modal, "trackInfoMedia").visible, false)
        compare(findChild(modal, "trackInfoTechnical").visible, false)
        compare(findChild(modal, "trackInfoLegal").cells.length, 1)
        compare(findChild(modal, "trackInfoTitle").text, "Chosen", "the heading stays the track's")
    }

    function test_columns_give_way_on_a_narrow_window() {
        const modal = open("/music/Chosen.flac", "Chosen", narrow)
        const technical = findChild(modal, "trackInfoTechnical")
        tryCompare(technical, "visible", true)
        verify(technical.columns < 3, "columns: " + technical.columns)
        verify(technical.columns >= 1)
    }

    function test_copy_takes_the_line_or_everything() {
        const modal = open("/music/Chosen.flac", "Chosen")
        tryCompare(findChild(modal, "trackInfoCopy"), "visible", true)
        const menu = findChild(modal, "trackInfoCopyMenu")
        const clipboard = findChild(modal, "trackInfoClipboard")
        menu.chosen(0)
        compare(clipboard.text, "Chosen - Record - Band")
        menu.chosen(1)
        compare(clipboard.text, "Chosen\nRecord\nBand")
    }

    function test_without_a_subject_it_speaks_of_what_plays() {
        Player.album = "Live Record"
        Player.artist = "Live Band"
        const modal = open("", "")
        compare(findChild(modal, "trackInfoTitle").text, "Playing")
        compare(findChild(modal, "trackInfoAlbum").text, "Live Record")
        tryCompare(NowAbout, "askedPath", "/music/Playing.flac")
        compare(findChild(modal, "trackInfoCopy").visible, false, "nothing to copy without a heading")
    }
}
