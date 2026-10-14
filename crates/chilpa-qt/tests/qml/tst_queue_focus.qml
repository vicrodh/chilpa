import QtQuick
import QtTest
import app.chilpa
import "qrc:/qt/qml/app/chilpa/controls/qml/Screens"

TestCase {
    id: test
    name: "QueueFocus"
    when: windowShown
    visible: true
    width: 1100; height: 700
    ListenListScreen { id: page; anchors.fill: parent }
    property var tracks
    property var rows
    function init() {
        ListenList.command("select", "1", "")
        page.cancelFollow()
        page.typed = ""
        tracks = findChild(page, "queueTracks")
        rows = findChild(page, "trackListView")
        verify(tracks && rows)
        tracks.clear()
        for (let index = 0; index < 80; ++index)
            tracks.append({trackId:index + 1,rowKind:0,number:index + 1,title:"Song " + index,
                artist:"Artist",album:"Album",sourceName:"",duration:"3:00",plays:0,
                quality:"FLAC",tier:2,available:true,playing:index === 40,selected:false,
                headerLabel:"",headerMeta:"",art:""})
        tryCompare(page, "playingRow", 40)
        page.goToTop()
        waitForRendering(page)
    }
    function centered(index) {
        const item = rows.itemAtIndex(index)
        return !!item && Math.abs(item.y + item.height / 2 - rows.contentY - rows.height / 2) < 2
    }
    function clickFocus() {
        const button = findChild(page, "queueFocusPlaying")
        verify(button.enabled)
        mouseClick(button, button.width / 2, button.height / 2)
        tryCompare(page, "followPlaying", true)
        tryVerify(() => centered(40))
    }
    function test_focus_centers_an_offscreen_track_and_top_cancels_follow() {
        verify(rows.atYBeginning)
        clickFocus()
        const top = findChild(page, "queueBackToTop")
        mouseClick(top, top.width / 2, top.height / 2)
        tryCompare(page, "followPlaying", false)
        tryVerify(() => rows.atYBeginning)
    }
    function test_follow_tracks_a_new_occurrence_without_a_path_change() {
        clickFocus()
        tracks.setProperty(40, "playing", false)
        tracks.setProperty(50, "playing", true)
        ++Player.queueRev
        tryCompare(page, "playingRow", 50)
        tryVerify(() => centered(50))
        verify(page.followPlaying)
    }
    function test_wheel_scroll_yields_to_the_user() {
        clickFocus()
        mouseWheel(rows, rows.width / 2, rows.height / 2, 0, -120)
        tryCompare(page, "followPlaying", false)
    }
    function test_another_queue_and_no_playing_track_cancel_follow() {
        clickFocus()
        ListenList.command("select", "2", "")
        tryCompare(page, "followPlaying", false)
        tracks.setProperty(40, "playing", false)
        page.refreshPlayingRow()
        compare(page.playingRow, -1)
        verify(!findChild(page, "queueFocusPlaying").enabled)
    }
}
