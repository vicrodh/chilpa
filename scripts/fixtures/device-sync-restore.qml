import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.devices

Item {
    id: smoke
    property var mainWindow: null
    property int phase: 0
    property bool stopped: false
    property string queryId: ""
    function check(condition, message) { if (!condition) throw new Error(message) }
    function find(item, predicate) {
        if (predicate(item)) return item
        for (const child of item.children || []) { const match = find(child, predicate); if (match) return match }
        return null
    }
    function named(name) {
        const item = find(mainWindow.contentItem, row => row.objectName === name && row.visible)
        check(!!item, "missing control " + name)
        return item
    }
    function music() {
        const control = find(mainWindow.contentItem, row => row.objectName === "deviceSections" || (row.options && row.options.length === 2 && row.options[0].text === I18n.tr("Music", I18n.rev) && typeof row.activated === "function"))
        check(!!control, "Music segment missing")
        control.activated(0)
    }
    function advance() {
        check(!Player.playing, "unexpected playback")
        if (phase === 0) {
            const component = Qt.createComponent("qrc:/qt/qml/app/chilpa/qml/Main.qml")
            check(component.status === Component.Ready, component.errorString())
            mainWindow = component.createObject(null)
            mainWindow.width = 1100; mainWindow.height = 800
            Qt.createQmlObject('import QtQuick; Rectangle { anchors.fill: parent; z: -100 }', mainWindow.contentItem).color = Qt.binding(() => mainWindow.color)
            Navigation.go("deviceSync", 0)
            phase = 1
            return
        }
        const status = JSON.parse(DeviceSync.snapshot || "{}")
        const document = JSON.parse(DeviceSync.catalog || "{}")
        if (DeviceSync.busy || !document.has_profiles) return
        check(!status.error, "native restore failed: " + JSON.stringify(status.error))
        if (phase === 1) {
            check(document.profiles.some(row => row.name === "SMOKE_PROFILE"), "saved profile missing after restart")
            named("deviceDestination").picked("SMOKE_STORAGE")
            named("devicePreview").clicked()
            phase = 2
        } else if (phase === 2) {
            if (status.removals === 1 && status.copies === 0 && status.unchanged === 3) {
                named("deviceApproveRemovals").click()
                check(named("deviceSyncNow").enabled, "cleanup approval failed")
                named("deviceSyncNow").clicked()
                phase = 12
                return
            }
            check(status.phase === "review" && status.unchanged === 3 && document.change_count === 0, "restart recopy: " + JSON.stringify(status))
            music()
            phase = 3
        } else if (phase === 3) {
            const query = document.options.find(row => row.name === "DAP Query")
            check(!!query, "saved Refine query missing")
            queryId = query.id
            const row = find(mainWindow.contentItem, item => item.objectName === "deviceSelectionCheck" && (item.text === "DAP Query" || item.parent.entry?.title === "DAP Query"))
            if (!row) return
            row.clicked()
            named("devicePreview").clicked()
            phase = 4
        } else if (phase === 4) {
            check(status.phase === "review" && status.copies === 1 && status.unchanged === 3 && status.removals === 0,
                  "query did not share the selected media: " + JSON.stringify(status))
            named("deviceSyncNow").clicked()
            phase = 5
        } else if (phase === 5) {
            check(status.phase === "done", "query sync did not complete")
            music()
            phase = 6
        } else if (phase === 6) {
            const row = find(mainWindow.contentItem, item => item.objectName === "deviceSelectionCheck" && (item.text === "DAP Query" || item.parent.entry?.title === "DAP Query"))
            if (!row) return
            row.clicked()
            if (!status.remove_unselected) named("deviceRemoveUnselected").click()
            named("devicePreview").clicked()
            phase = 7
        } else if (phase === 7) {
            check(status.phase === "review" && status.copies === 0 && status.updates === 0 && status.removals === 1 && status.unchanged === 3,
                  "deselecting query did not preserve shared media: " + JSON.stringify(status))
            check(!named("deviceSyncNow").enabled, "removal did not require approval")
            named("deviceApproveRemovals").click()
            phase = 8
        } else if (phase === 8) {
            check(named("deviceSyncNow").enabled, "approval did not enable the exact review")
            named("deviceSyncNow").clicked()
            phase = 9
        } else if (phase === 9) {
            check(status.phase === "done" && status.removals === 1, "approved removal did not complete")
            named("devicePreview").clicked()
            phase = 10
        } else if (phase === 12) {
            check(status.phase === "done", "cleanup did not complete")
            named("devicePreview").clicked()
            phase = 2
        } else if (phase === 10) {
            check(status.phase === "review" && document.change_count === 0 && status.unchanged === 3, "removal changed the selected media")
            console.log("SMOKE device restart query and approved removal completed " + DeviceSync.snapshot)
            mainWindow.contentItem.grabToImage(function(result) { result.saveToFile("SMOKE_CAPTURE"); smoke.stopped = true; Qt.exit(0) })
            phase = 11
        }
    }
    Timer {
        interval: 300; repeat: true; running: !smoke.stopped
        onTriggered: { try { smoke.advance() } catch (error) { smoke.stopped = true; console.error("SMOKE FAIL restore phase=" + smoke.phase + " " + error.message); Qt.exit(1) } }
    }
    Timer { interval: 180000; running: !smoke.stopped; onTriggered: { console.error("SMOKE FAIL restore timeout phase=" + smoke.phase + " " + DeviceSync.snapshot); Qt.exit(1) } }
}
