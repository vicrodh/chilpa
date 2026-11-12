import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import app.chilpa.offline
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"
import "../../qml/Screens"

// The Offline music page: Music servers opens it as a page, it says what is
// kept and how it stands while downloading, after a cancel and with
// nothing, and it removes files only after both confirmations. The facade
// is the test double of `Sources`, which writes down what it was asked.
TestCase {
    id: test
    name: "OfflineMusicPage"
    when: windowShown
    visible: true
    width: 1200
    height: 800
    MusicServers { id: servers; width: 650; height: 300; model: [] }
    ServerMediaDeletion { id: host }
    Component { id: factory; OfflineMusicScreen {} }

    function listing(failed) {
        return {files: 3, size: "1.6 GiB", limit: "5120", limitSize: "5.0 GiB", usage: 0.32, full: false, inScope: 3, matching: 3, shown: 3,
            limits: {values: ["512", "5120", "unlimited"], options: ["512 MiB", "5 GiB", "Unlimited"]}, root: {folder: "", path: "file:///music/offline/"},
            servers: [{id: 7, name: "Living room", icon: "navidrome", files: 3, size: "1.6 GiB", folder: "", inherited: "file:///music/offline/Navidrome/"},
                      {id: 9, name: "Studio", icon: "jellyfin", files: 0, size: "0 B", folder: "file:///studio/", inherited: "file:///music/offline/Jellyfin/"}],
            artists: [{name: "Annenmaykantereit", albums: 1, tracks: 2}, {name: "Avantasia", albums: 1, tracks: 1}],
            rows: [{kind: "album", title: "Es ist Abend", artist: "Annenmaykantereit", tracks: 2, size: "554 MiB", state: failed ? "failed" : "complete", ids: [1, 2]},
                   {kind: "track", id: 1, title: "Tommi", artist: "Annenmaykantereit", number: 1, duration: "4:39", state: "complete"},
                   {kind: "track", id: 2, title: "Orangenlied", artist: "Annenmaykantereit", number: 2, duration: "4:00", state: failed ? "failed" : "complete", error: failed ? "The server answered 500" : ""},
                   {kind: "album", title: "The Scarecrow", artist: "Avantasia", tracks: 1, size: "61 MiB", state: "complete", ids: [3]},
                   {kind: "track", id: 3, title: "The Scarecrow", artist: "Avantasia", number: 4, duration: "11:12", state: "complete"}]}
    }
    function empty() {
        return {files: 0, size: "0 B", limit: "2048", limitSize: "2.0 GiB", usage: 0, inScope: 0, matching: 0, shown: 0, rows: [], artists: [], servers: [],
            limits: {values: ["2048", "unlimited"], options: ["2 GiB", "Unlimited"]}, root: {folder: "", path: "file:///music/offline/"}}
    }
    function init() {
        Offline.calls = []
        Offline.info = {count: 2}
        Offline.status = {running: false, total: 0}
        Offline.doc = listing(false)
        Offline.revision = 1
        Offline.loading = false
        Navigation.page = ""
    }
    function open() {
        const page = createTemporaryObject(factory, test, {width: test.width, height: test.height})
        waitForRendering(page)
        return page
    }
    function calls(action) { return Offline.calls.filter(call => call[0] === action) }
    function shown(item) { for (let at = item; at; at = at.parent) if (!at.visible) return false; return true }

    function test_music_servers_opens_the_page_and_no_dialog() {
        mouseClick(findChild(servers, "offlineDownloads"))
        compare(Navigation.page, "offlineMusic")
        compare(Navigation.selectedId, 0)
        compare(findChild(host, "serverOfflineDialog"), null)
        const page = open()
        compare(findChild(page, "offlineSelection"), null, "the page offers no menu's choice: menus act in place")
    }

    function test_with_nothing_kept_it_says_how_to_keep_some() {
        Offline.doc = empty()
        const page = open()
        tryCompare(findChild(page, "offlineEmpty"), "visible", true)
        compare(findChild(page, "offlineCount").text, "0 tracks")
        compare(findChild(page, "offlineUsage").text, "0 B of 2.0 GiB")
        verify(!findChild(page, "offlineRemoveAll").enabled)
        verify(!findChild(page, "offlineProgress").visible)
        compare(findChild(page, "offlinePoll").running, false, "a still page asks for nothing")
    }

    function test_kept_music_is_listed_by_record_under_its_artists() {
        const page = open()
        compare(findChild(page, "offlineCount").text, "3 tracks")
        compare(findChild(page, "offlineUsage").text, "1.6 GiB of 5.0 GiB")
        verify(findChild(page, "offlineUsageBar").visible)
        const records = findChild(page, "offlineRecords")
        compare(records.count, 5)
        verify(findChild(page, "offlineArtists").visible, "the rail at this width")
        verify(findChild(page, "offlineServer").visible, "two servers, a choice between them")
        compare(findChild(page, "offlineEmpty").visible, false)
        findChild(page, "offlineServer").picked("9")
        const asked = JSON.parse(calls("manager").pop()[2])
        compare(asked.server, 9)
        findChild(page, "offlineFailedOnly").clicked()
        compare(JSON.parse(calls("manager").pop()[2]).failed, true)
    }

    function test_downloading_shows_its_progress_and_blocks_other_jobs_until_cancelled() {
        const page = open()
        Offline.status = {running: true, total: 4, done: 1, failed: 0, title: "Tommi", phase: "downloading"}
        page.poll()
        verify(findChild(page, "offlineProgress").visible)
        compare(findChild(page, "offlinePoll").running, true)
        verify(findChild(page, "cancelOfflineDownload").enabled)
        verify(!findChild(page, "offlineRemoveAll").enabled, "one job at a time")
        verify(findChild(page, "offlineDownloadStatus").text.indexOf("1 of 4 downloaded") >= 0)
        waitForRendering(page)
        mouseClick(findChild(page, "cancelOfflineDownload"))
        verify(calls("cancel").length === 1)
        Offline.status = {running: false, total: 4, done: 1, failed: 0, cancelled: true}
        page.poll()
        compare(page.busy, false)
        verify(findChild(page, "offlineDownloadStatus").text.indexOf("Download cancelled.") >= 0)
        verify(calls("refresh").length > 0, "the end of a job reads the listing again")
        verify(findChild(page, "offlineRemoveAll").enabled)
    }

    function test_removing_everything_asks_both_confirmations() {
        const page = open()
        findChild(page, "offlineRemoveAll").clicked()
        verify(calls("removal_info").some(call => call[1] === "downloads"))
        const confirmation = findChild(page, "offlineRemovalConfirmation")
        tryCompare(confirmation, "opened", true)
        const accept = findChild(confirmation, "confirmationAccept")
        accept.clicked()
        compare(calls("remove").length, 0)
        findChild(confirmation, "destructiveAcknowledgement").checked = true
        tryCompare(accept, "enabled", true)
        accept.clicked()
        verify(calls("remove").some(call => call[2] === "fixture-removal"))
    }

    function test_a_track_checked_again_or_removed_by_its_row_and_by_the_bar() {
        Offline.doc = listing(true)
        const page = open()
        const records = findChild(page, "offlineRecords")
        tryVerify(() => records.itemAtIndex(2) !== null)
        const failed = records.itemAtIndex(2)
        verify(failed.row.error.length > 0)
        findChild(failed, "offlineRetry").clicked()
        verify(calls("start").some(call => call[1] === "tracks" && call[2] === "[2]"))
        Offline.status = {running: false, total: 0}
        page.status = {}
        findChild(records.itemAtIndex(0), "offlineRemove").clicked()
        verify(calls("removal_info").some(call => call[1] === "tracks" && call[2] === "[1,2]"), "a record's row acts on its tracks")
        findChild(page, "offlineRemovalConfirmation").close()
        failed.toggled()
        const bar = findChild(page, "offlineSelectionBar")
        tryCompare(bar, "visible", true)
        findChild(page, "offlineCheckAll").clicked()
        compare(Object.keys(page.checked).length, 3)
        findChild(page, "offlineRetryChecked").clicked()
        verify(calls("start").some(call => JSON.stringify(JSON.parse(call[2]).sort()) === "[1,2,3]"))
        findChild(page, "offlineClearChecks").clicked()
        tryCompare(bar, "visible", false)
    }

    function test_the_limit_goes_from_the_smallest_step_to_unlimited() {
        const page = open()
        const limit = findChild(page, "offlineLimit")
        compare(limit.value, "5120")
        limit.picked("unlimited")
        verify(calls("limit").some(call => call[1] === "unlimited"))
        const unlimited = listing(false)
        unlimited.limit = "unlimited"; unlimited.limitSize = ""; unlimited.usage = 0
        Offline.doc = unlimited
        Offline.revision = 2
        page.read()
        compare(findChild(page, "offlineUsage").text, "1.6 GiB · Unlimited")
        verify(!findChild(page, "offlineUsageBar").visible, "the disk is the limit")
    }

    function test_each_server_has_its_folder_and_the_general_one_comes_first() {
        const page = open()
        findChild(page, "offlineSections").activated(1)
        verify(shown(findChild(page, "offlineRootFolder")))
        const studio = findChild(page, "offlineServerFolder_9")
        verify(shown(studio))
        compare(studio.folder, "file:///studio/")
        const before = Offline.calls.length
        studio.chosen("")
        verify(calls("server_folder").some(call => call[1] === "9" && call[2] === ""))
        verify(Offline.calls.slice(before).some(call => call[0] === "manager"), "and the listing is asked for again")
    }

    function test_narrow_the_rail_becomes_a_choice() {
        const page = open()
        page.width = 560
        tryCompare(findChild(page, "offlineArtists"), "visible", false)
        verify(findChild(page, "offlineArtist").visible)
        findChild(page, "offlineArtist").picked("Avantasia")
        compare(JSON.parse(calls("manager").pop()[2]).artist, "Avantasia")
    }
}
