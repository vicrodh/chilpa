import QtQuick
import QtTest
import app.chilpa.folders
import "../../qml/Components"

TestCase {
    name: "ScanStatus"
    when: windowShown
    visible: true
    width: 1100; height: 600
    ScanProgress { id: banner; width: 850 }
    FolderRow {
        id: folder
        y: 300; width: 850
        path: "/music"; name: "Music"; lastScan: undefined; reachable: true; scanPolicy: "manual"
        onRescan: LibraryFolders.rescan(path)
        onPolicyChosen: policy => LibraryFolders.choosePolicy(path, policy)
    }
    function init() {
        LibraryFolders.scanning = false
        LibraryFolders.scanningPath = ""
        LibraryFolders.progress = 0
        LibraryFolders.phase = "idle"
        LibraryFolders.status = ""
        LibraryFolders.canResume = false
        LibraryFolders.unreachable = false
        LibraryFolders.action = ""
        LibraryFolders.readCount = 2
        LibraryFolders.sameCount = 10
        LibraryFolders.missingCount = 0
        LibraryFolders.unreadableCount = 0
        folder.reachable = true
        folder.width = 850
    }
    function title() { return findChild(banner, "scanTitle").text }
    function press(name) {
        const control = findChild(banner, name)
        verify(waitForRendering(banner))
        mouseClick(control)
    }
    function test_idle_has_no_banner_or_reserved_height() {
        compare(banner.visible, false)
        compare(banner.implicitHeight, 0)
    }
    function test_scan_stages_and_current_root_are_explicit_and_stop_is_cooperative() {
        LibraryFolders.scanning = true
        LibraryFolders.phase = "discovering"
        LibraryFolders.scanningPath = "/music"
        compare(banner.visible, true)
        compare(title(), "Looking for files…")
        compare(findChild(banner, "scanRoot").text, "/music")
        LibraryFolders.phase = "reading"
        LibraryFolders.done = 25; LibraryFolders.total = 100; LibraryFolders.progress = 0.25
        compare(title(), "Reading 25 of 100 files")
        const fill = findChild(banner, "scanProgressFill")
        compare(fill.width / fill.parent.width, 0.25)
        press("stopLibraryScan")
        compare(LibraryFolders.action, "stop")
        LibraryFolders.phase = "stopping"
        compare(findChild(banner, "stopLibraryScan").enabled, false)
        compare(title(), "Stopping after the current file…")
        LibraryFolders.phase = "finalizing"
        compare(title(), "Updating the library…")
    }
    function test_interruption_offers_continue_and_retains_the_previous_work_summary() {
        LibraryFolders.phase = "interrupted"
        LibraryFolders.canResume = true
        compare(title(), "Library check interrupted")
        verify(findChild(banner, "scanResumeHint").visible)
        verify(findChild(banner, "scanSummary").text.includes("2 read, 10 unchanged"))
        verify(!findChild(banner, "dismissLibraryScan").visible)
        press("resumeLibraryScan")
        compare(LibraryFolders.action, "resume")
        verify(findChild(banner, "scanResumeHint").text.includes("Automatic checks are paused"))
        press("dismissPendingLibraryScan")
        compare(LibraryFolders.action, "dismiss")
    }
    function test_a_failure_is_never_shown_as_completion_and_can_retry() {
        LibraryFolders.phase = "failed"
        LibraryFolders.status = "The folder is unavailable."
        compare(title(), "Library check failed")
        compare(findChild(banner, "scanError").text, "The folder is unavailable.")
        press("retryLibraryScan")
        compare(LibraryFolders.action, "all")
        press("dismissLibraryScan")
        compare(LibraryFolders.action, "dismiss")
    }
    function test_an_unavailable_root_is_reported_without_claiming_missing_tracks_were_deleted() {
        LibraryFolders.phase = "complete"
        LibraryFolders.unreachable = true
        const notice = findChild(banner, "scanUnavailable")
        verify(notice.visible)
        verify(notice.text.includes("remain in the library"))
    }
    function test_policy_choices_are_per_folder_and_manual_check_can_retry_a_missing_mount() {
        const policy = findChild(folder, "folderCheckPolicy")
        compare(policy.values, ["manual", "startup", "12h", "24h", "weekly"])
        policy.picked("12h")
        compare(LibraryFolders.actionPath, "/music")
        compare(LibraryFolders.chosenPolicy, "12h")
        folder.reachable = false
        const check = findChild(folder, "checkFolderChanges")
        verify(check.enabled)
        mouseClick(check)
        compare(LibraryFolders.action, "check")
        compare(LibraryFolders.actionPath, "/music")
        LibraryFolders.scanning = true
        compare(check.enabled, false)
        LibraryFolders.scanning = false
        LibraryFolders.canResume = true
        compare(check.enabled, false)
    }
    function test_policy_controls_wrap_inside_a_narrow_settings_view() {
        folder.width = 510
        verify(waitForRendering(folder))
        for (const name of ["folderCheckPolicy", "checkFolderChanges"]) {
            const control = findChild(folder, name)
            const at = control.mapToItem(folder, control.width, control.height)
            verify(at.x <= folder.width && at.y <= folder.implicitHeight, name + ": " + at.x + "," + at.y + " / " + folder.width + "," + folder.implicitHeight)
        }
    }
}
