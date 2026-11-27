import QtQuick
import QtTest
import app.chilpa
import app.chilpa.server.playback
import "qrc:/qt/qml/app/chilpa/server/playback/qml/Components"

// #51: what one server is told. Each switch is kept at once and read back
// from the facade; a server whose reports count the play has no count apart.
TestCase {
    id: test
    name: "ServerPlayback"
    when: windowShown
    visible: true
    width: 960
    height: 700
    ServerPlaybackDialog { id: dialog }
    function init() {
        ServerPlayback.calls = []
        ServerPlayback.state = { name: "Navidrome lab", kind: "subsonic", reports: true, counts_apart: true, report: true, count: true, mark: 50 }
    }
    function cleanup() { dialog.close(); tryCompare(dialog, "visible", false) }
    function field(name) { return findChild(dialog.contentItem, name) }

    function test_each_switch_is_its_own_and_kept_at_once() {
        dialog.show(3)
        tryCompare(dialog, "opened", true)
        compare(field("serverPlaybackName").text, "Navidrome lab")
        const report = field("serverReportPlayback")
        const count = field("serverCountPlays")
        verify(report.checked && count.checked && count.visible)
        report.toggled(false)
        compare(ServerPlayback.calls, [[3, "reportPlayback", "false"]])
        verify(!report.checked, "read back from the facade")
        verify(count.checked, "the count is its own switch")
        const mark = field("serverPlayMark")
        verify(mark.visible)
        mark.released(3)
        compare(ServerPlayback.calls[1], [3, "playMark", "10"], "a play counts from a tenth of the track")
        compare(field("serverPlayMarkValue").text, "10%")
        count.toggled(false)
        verify(!mark.visible, "no mark without a count")
    }

    function test_a_server_whose_reports_count_has_no_count_apart() {
        ServerPlayback.state = { name: "Jellyfin lab", kind: "jellyfin", reports: true, counts_apart: false, report: true, count: true, mark: 50 }
        dialog.show(4)
        tryCompare(dialog, "opened", true)
        verify(!field("serverCountPlays").parent.visible, "its reports count the play")
        verify(!field("serverPlayMark").visible)
        verify(field("serverReportPlayback").checked)
    }
}
