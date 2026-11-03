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
    Item { id: menuButton; x: parent.width - width - 16; y: 8; width: 32; height: 32
        UpdatesIndicator { id: inbox; anchors.top: parent.top; anchors.right: parent.right; launchChecked: true } }
    SignalSpy { id: quits; target: inbox; signalName: "quitAsked" }
    function state(phase, busy, progress) {
        Updates.snapshot = JSON.stringify({supported:true,can_install:true,phase:phase,busy:busy,progress:progress,automatic:true})
        wait(0)
        waitForRendering(panel)
    }
    function init() { Updates.calls = []; Updates.restarts = false; PluginUpdates.calls = []; PluginUpdates.snapshot = "{}"; inbox.dismissed = ""; quits.clear(); state("idle", false, 0) }
    function offer(phase) {
        Updates.snapshot = JSON.stringify({supported:true,can_install:true,phase:phase,busy:false,progress:0,automatic:true,current_version:"0.1.1",version:"0.1.2"})
        wait(0)
    }
    function test_the_panel_names_the_running_and_the_offered_versions() {
        offer("available")
        compare(findChild(panel,"currentVersion").text, "0.1.1")
        compare(findChild(panel,"availableVersion").text, "0.1.2")
        verify(findChild(panel,"availableVersion").visible)
        Updates.snapshot = JSON.stringify({supported:true,can_install:true,phase:"current",current_version:"0.1.2"})
        compare(findChild(panel,"currentVersion").text, "0.1.2")
        verify(!findChild(panel,"availableVersion").visible)
    }
    function test_an_unread_version_comes_out_of_the_menu_and_names_both_versions() {
        const bubble = findChild(inbox, "updateBubble")
        tryVerify(() => !bubble.visible)
        offer("available")
        tryVerify(() => bubble.opened)
        compare(findChild(bubble.contentItem, "noticeTitle").text, "Chilpa 0.1.2 is available")
        compare(findChild(bubble.contentItem, "noticeCurrent").text, "Current version: 0.1.1")
        // Under the menu's button, its right edge on the button's.
        const corner = bubble.contentItem.mapToItem(menuButton, bubble.contentItem.width, 0)
        fuzzyCompare(corner.x, menuButton.width, 1)
        verify(corner.y > menuButton.height)
        mouseClick(findChild(bubble.contentItem, "noticeDismiss"))
        tryVerify(() => !bubble.visible)
        offer("ready")
        tryVerify(() => bubble.opened)
        compare(findChild(bubble.contentItem, "noticeTitle").text, "Chilpa 0.1.2 is ready")
        mouseClick(findChild(bubble.contentItem, "noticeView"))
        compare(PluginUpdates.calls[0], ["updates_app_read", {identity:"0.1.2:ready"}])
        const details = findChild(inbox, "appUpdateDetails")
        tryVerify(() => details.opened)
        details.close()
        tryVerify(() => !details.visible)
    }
    function test_a_started_restart_asks_the_window_to_quit_chilpa() {
        state("ready", false, 1)
        mouseClick(findChild(panel,"restartUpdate"))
        compare(Updates.calls, ["restart"])
        compare(quits.count, 0)
        Updates.restarts = true
        mouseClick(findChild(panel,"restartUpdate"))
        compare(quits.count, 1)
    }
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
    function test_a_tarball_or_mac_install_updates_itself_and_a_rate_limit_is_named() {
        Updates.snapshot = JSON.stringify({supported:true,can_install:true,kind:"tarball",phase:"available"})
        wait(0)
        verify(findChild(panel,"downloadUpdate").visible)
        verify(!findChild(panel,"openUpdateRelease").visible)
        Updates.snapshot = JSON.stringify({supported:true,can_install:false,phase:"error",reason:"rate_limited"})
        verify(findChild(panel,"updateStatus").text.includes("GitHub"))
        Updates.snapshot = JSON.stringify({supported:true,can_install:true,phase:"error",reason:"read_only"})
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
