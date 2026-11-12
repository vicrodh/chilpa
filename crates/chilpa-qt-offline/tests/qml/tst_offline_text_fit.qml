import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import app.chilpa.offline
import "../../qml/Screens"

// The Offline music page says its summary, limit, progress, order,
// filters, rail, records, selection bar and folders in each language
// without running into each other, from a narrow window to a wide one,
// also a tenth wider than Inter.
TestCase {
    id: test
    name: "OfflineTextFit"
    when: windowShown && TextAudit.ready
    visible: true
    width: 1200
    height: 900

    Component { id: factory; OfflineMusicScreen {} }

    readonly property var listing: ({files: 1234, size: "48.6 GiB", limit: "51200", limitSize: "50.0 GiB", usage: 0.97, full: false,
        inScope: 1234, matching: 2400, shown: 2000, root: {folder: "", path: "file:///home/person/.local/share/Chilpa/OfflineFolder/"},
        limits: {values: ["512", "1024", "2048", "5120", "10240", "20480", "51200", "102400", "unlimited"],
                 options: ["512 MiB", "1 GiB", "2 GiB", "5 GiB", "10 GiB", "20 GiB", "50 GiB", "100 GiB", "Unlimited"]},
        servers: [{id: 7, name: "Living room server with a long name", icon: "navidrome", files: 1200, size: "47.9 GiB", folder: "", inherited: "file:///home/person/.local/share/Chilpa/OfflineFolder/Navidrome/"},
                  {id: 9, name: "Studio", icon: "jellyfin", files: 34, size: "700 MiB", folder: "file:///media/external/Offline%20music/", inherited: ""}],
        artists: [{name: "Annenmaykantereit", albums: 12, tracks: 154}, {name: "", albums: 1, tracks: 3}, {name: "Panteón Rococó", albums: 1, tracks: 17}],
        rows: [{kind: "album", title: "Es ist Abend und wir sitzen bei mir", artist: "Annenmaykantereit", tracks: 15, size: "554 MiB", state: "failed", ids: [1, 2]},
               {kind: "track", id: 1, title: "Weißhausstraße", artist: "Annenmaykantereit", number: 6, duration: "3:46", state: "complete"},
               {kind: "track", id: 2, title: "Heute Abend wird es regnen", artist: "Annenmaykantereit", number: 12, duration: "1:02:44", state: "failed", error: "There is not enough free space on the disk for this file."},
               {kind: "album", gone: true, title: "", artist: "", tracks: 1, size: "12 MiB", state: "complete", ids: []},
               {kind: "track", id: 0, title: "", artist: "", number: 0, duration: "0:00", state: "complete"}]})

    function cleanup() { I18n.choose("en") }

    function cases() {
        const out = []
        for (const row of TextAudit.cases())
            for (const width of [520, 760, 1200])
                out.push({ tag: row.tag + " " + width, language: row.language, scale: row.scale, width: width })
        return out
    }

    function surface(data) {
        I18n.choose(data.language)
        Offline.doc = test.listing
        Offline.revision = 1
        Offline.loading = false
        Offline.status = {running: true, total: 1234, done: 600, failed: 3, title: "Heute Abend wird es regnen", phase: "verifying",
            error: "The offline storage limit has been reached."}
        const page = createTemporaryObject(factory, test, { width: data.width, height: test.height })
        page.checked = ({1: true, 2: true})
        return page
    }

    function test_tracks_data() { return cases() }
    function test_tracks(data) {
        const page = surface(data)
        const found = TextAudit.check(test, page, data.scale)
        compare(found.length, 0, found.join("\n"))
    }

    function test_folders_data() { return cases() }
    function test_folders(data) {
        const page = surface(data)
        page.section = 1
        const found = TextAudit.check(test, page, data.scale)
        compare(found.length, 0, found.join("\n"))
    }

    function test_unlimited_and_empty_data() { return cases() }
    function test_unlimited_and_empty(data) {
        I18n.choose(data.language)
        Offline.doc = {files: 0, size: "0 B", limit: "unlimited", limitSize: "", usage: 0, inScope: 0, matching: 0, shown: 0, rows: [], artists: [], servers: [],
            limits: test.listing.limits, root: {folder: "", path: ""}}
        Offline.status = {running: false, total: 0}
        const page = createTemporaryObject(factory, test, { width: data.width, height: test.height })
        const found = TextAudit.check(test, page, data.scale)
        compare(found.length, 0, found.join("\n"))
    }
}
