import QtQuick
import app.chilpa
import app.chilpa.devices

Item {
    id: test
    property var mainWindow: null
    property int tick: 0
    property int cycles: 0
    property bool stopped: false
    property bool activePoll: false
    function check(condition, message) { if (!condition) throw new Error(message) }
    function find(item, predicate) {
        if (predicate(item)) return item
        for (const child of item.children || []) { const found = find(child, predicate); if (found) return found }
        return null
    }
    Component.onCompleted: {
        const component = Qt.createComponent("qrc:/qt/qml/app/chilpa/qml/Main.qml")
        if (component.status !== Component.Ready) { console.error(component.errorString()); Qt.exit(1); return }
        mainWindow = component.createObject(null)
        mainWindow.width = 1100; mainWindow.height = 800
        Navigation.go("deviceSync", 0)
    }
    function advance() {
        check(!Player.playing, "unexpected playback")
        const status = JSON.parse(DeviceSync.snapshot || "{}")
        check(!status.error, "device lifecycle error: " + JSON.stringify(status.error))
        const stage = tick % 8
        if (stage === 0) {
            check(!DeviceSync.busy, "device reopen did not settle")
            const document = JSON.parse(DeviceSync.catalog || "{}")
            check(document.has_profiles && document.options.length > 0, "device reopen lost its catalog")
            const destination = find(mainWindow.contentItem, item => item.objectName === "deviceDestination")
            check(!!destination, "storage picker missing")
            destination.picked("SMOKE_STORAGE")
            DeviceSync.filter("DAP")
            DeviceSync.filter("")
            const button = find(mainWindow.contentItem, item => item.objectName === "devicePreview")
            check(button && button.enabled, "preview unavailable after reopening")
            button.clicked()
        } else if (stage === 1) {
            check(!DeviceSync.busy && status.phase === "review", "review did not settle")
            check(status.copies === 0 && status.updates === 0 && status.removals === 0 && status.unchanged === 3,
                  "repeated review changed the managed files")
            Navigation.go("library", 0)
        } else if (stage === 2) {
            const router = find(mainWindow.contentItem, item => typeof item.committedRoute === "string")
            check(DeviceSync.catalog === "" && DeviceSync.pageRows === "", "closed surface retained visible payloads " + JSON.stringify({screen:Navigation.screen,pending:Navigation.pending,window:mainWindow.visibility,route:router?.route,committed:router?.committedRoute,phase:router?.phase,catalog:DeviceSync.catalog,page:DeviceSync.pageRows}))
            DeviceSync.filter("")
            DeviceSync.poll()
            DeviceSync.changePage(0)
            check(DeviceSync.catalog === "" && DeviceSync.pageRows === "", "late callback repopulated the closed surface")
            mainWindow.visible = false
        } else if (stage === 4) {
            check(DeviceSync.catalog === "" && !DeviceSync.busy, "hidden surface restarted device work")
            mainWindow.visible = true
            Navigation.go("settings", 0)
        } else if (stage === 5) {
            Navigation.go("library", 0)
        } else if (stage === 6) {
            Navigation.go("deviceSync", 0)
        } else if (stage === 7) {
            ++cycles
            console.log("SMOKE lifecycle cycle=" + cycles + " snapshot=" + DeviceSync.snapshot)
        }
        ++tick
        if (tick >= 120) {
            stopped = true
            console.log("SMOKE lifecycle real Main completed cycles=" + cycles + " seconds=1800")
            Qt.exit(0)
        }
    }
    Timer {
        interval: 15000; repeat: true; running: !test.stopped
        onTriggered: { try { test.advance() } catch (error) { test.stopped = true; console.error("SMOKE FAIL lifecycle tick=" + test.tick + " " + error.message); Qt.exit(1) } }
    }
}
