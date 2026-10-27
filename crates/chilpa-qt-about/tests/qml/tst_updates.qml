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
        Updates.snapshot = JSON.stringify({supported:true,can_install:true,phase:phase,busy:busy,progress:progress,automatic:true})
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
    function test_manual_installations_check_and_offer_only_the_release_page() {
        Updates.snapshot = JSON.stringify({supported:true,can_install:false,phase:"available",release_url:"https://example.invalid/release"})
        wait(0)
        verify(panel.visible)
        verify(!findChild(panel,"downloadUpdate").visible)
        verify(findChild(panel,"openUpdateRelease").visible)
        verify(findChild(panel,"checkUpdates").enabled)
        Updates.snapshot = JSON.stringify({supported:true,can_install:false,phase:"error"})
        verify(!findChild(panel,"updateStatus").text.includes("AppImage"))
    }
    function test_offline_controls_do_not_offer_network_actions() {
        Updates.snapshot = JSON.stringify({supported:true,can_install:true,phase:"available",offline:true})
        verify(!findChild(panel,"checkUpdates").enabled)
        verify(!findChild(panel,"downloadUpdate").enabled)
        Updates.snapshot = JSON.stringify({supported:true,can_install:false,phase:"available",offline:true,release_url:"https://example.invalid/release"})
        verify(!findChild(panel,"openUpdateRelease").enabled)
    }
    function test_hidden_panels_stop_polling() {
        state("checking", true, 0)
        const timer = findChild(panel,"updatePolling")
        verify(timer.running)
        panel.parent.visible = false
        verify(!timer.running)
        panel.parent.visible = true
    }
    function test_other_installations_hide_the_appimage_controls() {
        Updates.snapshot = JSON.stringify({supported:false,phase:"idle"})
        wait(0)
        compare(panel.visible, false)
    }
}
