import QtQuick
import app.chilpa
import app.chilpa.devices

Item {
    id: test
    property var mainWindow: null
    property int phase: 0
    property int cycles: 0
    property double started: Date.now()
    property bool stopped: false
    property string token: ""
    property string selected: ""
    property var ids: []
    property double duration: Number("SMOKE_SECONDS") * 1000
    property double waitingSince: 0
    TrackListModel { id: tracks; kind: "tracks"; key: "album\u001foff\u001fDAP"; filterRev: Library.contentRev }

    function check(condition, message) { if (!condition) throw new Error(message) }
    function find(item, predicate) {
        if (predicate(item)) return item
        for (const child of item.children || []) { const found = find(child,predicate); if (found) return found }
        return null
    }
    function named(name) {
        const item = find(mainWindow.contentItem,item => item.objectName === name && item.visible)
        check(!!item,"missing control " + name)
        return item
    }
    function document() { return JSON.parse(DeviceSync.browser || "{}") }
    function openRow(id) {
        const button = find(mainWindow.contentItem,item => item.objectName === "deviceMusicOpen" && item.visible && item.parent.entry?.id === id)
        if (!button) {
            if (!waitingSince) waitingSince = Date.now()
            check(Date.now()-waitingSince < 10000,"missing music row " + id)
            return false
        }
        waitingSince = 0
        button.clicked()
        return true
    }
    Component.onCompleted: {
        const component = Qt.createComponent("qrc:/qt/qml/app/chilpa/qml/Main.qml")
        if (component.status !== Component.Ready) { console.error(component.errorString()); Qt.exit(1); return }
        mainWindow = component.createObject(null)
        mainWindow.width = 1100; mainWindow.height = 800
        Navigation.go("deviceSync",0)
    }
    function advance() {
        check(!Player.playing,"unexpected playback")
        if (DeviceSync.busy) {
            if (phase === 21) DeviceSync.poll()
            if (DeviceSync.busy) return
        }
        const status = JSON.parse(DeviceSync.snapshot || "{}")
        const catalog = JSON.parse(DeviceSync.catalog || "{}")
        if (phase !== 0) check(!status.error,"browser task failed: " + JSON.stringify(status.error))
        if (phase === 0) {
            if (!catalog.has_profiles || !tracks.ready || tracks.count !== 2) return
            named("deviceDestination").picked("SMOKE_STORAGE")
            tracks.selectAll(); ids = JSON.parse(tracks.selectedTrackIdsJson())
            selected = catalog.options.find(row => row.checked).id
            named("devicePreview").clicked()
            phase = 1
        } else if (phase === 1) {
            check(status.phase === "review" && status.copies === 0 && status.unchanged === 3,"seed review changed files")
            token = status.plan
            named("deviceSections").activated(0)
            phase = 2
        } else if (phase === 2) {
            if (!openRow(selected)) return
            phase = 3
        } else if (phase === 3) {
            const view = document()
            check(view.rows.length === 1 && view.rows[0].title === "DAP Native Smoke","selection lost its album name")
            if (!openRow(view.rows[0].id)) return
            phase = 4
        } else if (phase === 4) {
            check(document().rows.length === 3,"playlist duplicate occurrences were hidden")
            named("deviceBrowserSearch").text = "DAP First"
            check(document().rows.length === 2,"track filter lost playlist occurrences")
            named("deviceBrowserSearch").text = ""
            named("deviceBrowserBack").clicked()
            phase = 5
        } else if (phase === 5) {
            named("deviceBrowserBack").clicked()
            phase = 6
        } else if (phase === 6) {
            check(JSON.parse(DeviceSync.snapshot).plan === token,"browsing invalidated the preview")
            named("deviceSelectionSearch").text = "Missing record"
            DeviceSync.useTracks(JSON.stringify(ids))
            phase = 7
        } else if (phase === 7) {
            const option = catalog.options.find(row => row.id.startsWith("tracks:"))
            check(option && option.name === "DAP Native Smoke" && option.checked,"album batch still has a generic name")
            check(option.description.includes("Sync Fixture"),"batch artist missing")
            check(named("deviceSelectionSearch").text === "","incoming album remained behind the old filter")
            if (!openRow(option.id)) return
            phase = 8
        } else if (phase === 8) {
            const view = document()
            check(view.rows.length === 1,"album batch root is incomplete")
            if (!openRow(view.rows[0].id)) return
            phase = 9
        } else if (phase === 9) {
            check(document().rows.length === 2,"album batch lost tracks")
            named("deviceBrowserBack").clicked()
            phase = 10
        } else if (phase === 10) {
            named("deviceBrowserBack").clicked()
            named("deviceSections").activated(1)
            phase = 11
        } else if (phase === 11) {
            const view = document()
            check(view.recorded && view.notice && view.rows.length === 2,"synced history is incomplete")
            named("deviceBrowserView").picked("artists")
            phase = 12
        } else if (phase === 12) {
            const view = document()
            check(view.view === "artists" && view.rows.length === 1,"artist category is incomplete")
            if (!openRow(view.rows[0].id)) return
            phase = 13
        } else if (phase === 13) {
            const view = document()
            check(view.rows.length === 1 && view.rows[0].title === "DAP Native Smoke","artist albums are incomplete")
            if (!openRow(view.rows[0].id)) return
            phase = 14
        } else if (phase === 14) {
            check(document().rows.length === 2,"artist album lost tracks")
            named("deviceBrowserBack").clicked()
            phase = 15
        } else if (phase === 15) {
            named("deviceBrowserBack").clicked()
            named("deviceBrowserView").picked("albums")
            phase = 16
        } else if (phase === 16) {
            check(document().rows.length === 1,"album category includes unrelated collections")
            named("deviceBrowserView").picked("tracks")
            phase = 17
        } else if (phase === 17) {
            check(document().rows.length === 2,"track category lost media")
            named("deviceBrowserView").picked("playlists")
            phase = 18
        } else if (phase === 18) {
            const view = document()
            check(view.rows.length === 1 && view.rows[0].kind === "playlist" && view.rows[0].open,"playlist category is incomplete")
            if (!openRow(view.rows[0].id)) return
            phase = 19
        } else if (phase === 19) {
            const rows = document().rows
            check(rows.length === 3 && rows[0].path === rows[2].path && rows[0].id !== rows[2].id,"device playlist lost its duplicate occurrence")
            check(rows[2].number === 3,"playlist order is missing")
            named("deviceBrowserBack").clicked()
            phase = 20
        } else if (phase === 20) {
            if (!openRow(document().rows[0].id)) return
            Navigation.go("library",0)
            phase = 21
        } else if (phase === 21) {
            check(DeviceSync.catalog === "" && DeviceSync.pageRows === "" && DeviceSync.browser === "","closed surface retained a payload")
            DeviceSync.filterBrowser("late"); DeviceSync.browserView("tracks"); DeviceSync.browserPage(0); DeviceSync.openBrowserAlbum("album:0"); DeviceSync.browse("recorded"); DeviceSync.poll()
            check(DeviceSync.browser === "" && !DeviceSync.busy,"late callback rebuilt the closed browser")
            mainWindow.visible = false
            phase = 22
        } else if (phase === 22) {
            check(DeviceSync.browser === "" && !DeviceSync.busy,"hidden surface restarted work")
            mainWindow.visible = true
            ++cycles
            console.log("SMOKE browser cycle=" + cycles + " seconds=" + (Date.now()-started)/1000)
            if (Date.now()-started >= duration) {
                stopped = true
                console.log("SMOKE browser real Main completed cycles=" + cycles)
                Qt.exit(0)
                return
            }
            Navigation.go("deviceSync",0)
            phase = 0
        }
    }
    Timer {
        interval: test.phase >= 21 && test.duration >= 1800000 ? 60000 : 300
        repeat: true
        running: !test.stopped
        onTriggered: { try { test.advance() } catch(error) { test.stopped = true; console.error("SMOKE FAIL browser phase=" + test.phase + " " + error.message); Qt.exit(1) } }
    }
}
