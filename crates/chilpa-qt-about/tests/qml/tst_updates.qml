import QtQuick
import QtTest
import app.chilpa.about
import "../../qml/Components"

TestCase {
    name: "AppImageUpdates"
    when: windowShown
    visible: true
    width: 640; height: 400
    UpdatesPanel { id: panel; width: parent.width }
    function state(phase, busy, progress) {
        Updates.snapshot = JSON.stringify({supported:true,phase:phase,busy:busy,progress:progress,automatic:true})
        wait(0)
        waitForRendering(panel)
    }
    function init() { Updates.calls = []; state("idle", false, 0) }
    function test_manual_check_and_download_require_a_click() {
        mouseClick(findChild(panel,"checkUpdates"))
        compare(Updates.calls, ["check"])
        state("available", false, 0)
        verify(findChild(panel,"downloadUpdate").visible)
        compare(Updates.calls, ["check"])
        mouseClick(findChild(panel,"downloadUpdate"))
        compare(Updates.calls, ["check", "download"])
    }
    function test_progress_cancellation_and_restart_have_separate_states() {
        state("downloading", true, 0.42)
        compare(findChild(panel,"updateProgress").value, 0.42)
        verify(!findChild(panel,"checkUpdates").enabled)
        mouseClick(findChild(panel,"cancelUpdate"))
        compare(Updates.calls, ["cancel"])
        state("ready", false, 1)
        verify(findChild(panel,"restartUpdate").visible)
        verify(!findChild(panel,"cancelUpdate").visible)
    }
    function test_other_installations_hide_the_appimage_controls() {
        Updates.snapshot = JSON.stringify({supported:false,phase:"idle"})
        wait(0)
        compare(panel.visible, false)
    }
}
