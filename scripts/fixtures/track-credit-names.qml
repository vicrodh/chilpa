import QtQuick
import QtQuick.Controls.Basic
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

ApplicationWindow {
    id: window
    width: 1250; height: 500; visible: true
    color: Theme.color.surfaceWindow
    Component.onCompleted: ThemeModel.mode = "dark"
    property string expectedArtist: "EXPECTED_ARTIST"
    property string capturePath: "CAPTURE_PATH"
    property bool captured: false
    TrackListModel { id: tracks; kind: "source"; key: "app.test.credit-names native:playlist:1|tracks"; playingPath: Player.path }
    TrackList { id: table; anchors.fill: parent; model: tracks; rich: false; showAlbum: false }
    Timer {
        interval: 100; running: tracks.count === 0; repeat: true
        onTriggered: { Sources.refresh(); tracks.filterRev++ }
    }
    TestCase {
        id: tests
        name: "CanonicalCreditNames"
        when: window.visible
        function test_displayed_names_keep_canonical_casing_and_explicit_roles() {
            tryCompare(tracks, "count", 3, 6000)
            table.rows.forceLayout()
            const expected = [window.expectedArtist, "ABBA", "k.d. lang"]
            for (let index = 0; index < expected.length; index++) {
                tryVerify(() => table.rows.itemAtIndex(index)?.credits?.artist !== undefined)
                const row = table.rows.itemAtIndex(index)
                console.log("SMOKE credit row=" + index + " expected=" + expected[index] + " actual=" + row.credits.artist)
                compare(row.credits.artist, expected[index])
                compare(row.credits.links[0].name, expected[index])
                compare(row.credits.links[0].key, "app.test.credit-names native:artist:" + (index + 1))
                for (const name of ["Technical Person", "Backing Person", "Mastering Person"])
                    verify(!row.credits.html.includes(name))
                tryVerify(() => findChild(row, "trackPerformingArtists") !== null)
                verify(findChild(row, "trackPerformingArtists").text.includes(expected[index]))
            }
            const lead = table.rows.itemAtIndex(0)
            compare(lead.credits.featured, "Guest Artist")
            verify(findChild(lead, "titleCredit").text.includes("Guest Artist"))
            waitForRendering(table)
            verify(table.grabToImage(result => {
                verify(result.saveToFile(window.capturePath))
                window.captured = true
            }))
            tryCompare(window, "captured", true)
        }
        function cleanupTestCase() {
            console.log(qtest_results.failCount ? "SMOKE FAIL canonical credit names" : "SMOKE canonical credit names completed")
            Qt.exit(qtest_results.failCount ? 1 : 0)
        }
    }
    Timer { interval: 15000; running: true; onTriggered: Qt.exit(1) }
}
