import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.folders
import app.chilpa.devices

Item {
    id: smoke
    property var mainWindow: null
    property int phase: 0
    property bool stopped: false
    property double cancelAt: 0
    property double applyAt: 0
    TrackListModel { id: tracks; kind: "tracks"; key: "album\u001foff\u001fDAP Large"; filterRev: Library.contentRev }
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
        const control = find(mainWindow.contentItem, row => row.options && row.options.length === 2 && row.options[0].text === I18n.tr("Music", I18n.rev) && typeof row.activated === "function")
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
            LibraryFolders.add("SMOKE_LARGE_ROOT")
            Navigation.go("deviceSync", 0)
            phase = 1
            return
        }
        const status = JSON.parse(DeviceSync.snapshot || "{}")
        const document = JSON.parse(DeviceSync.catalog || "{}")
        if (phase === 4) {
            if (status.phase === "transferring" && parseFloat(status.transferred || "0") > 0) {
                cancelAt = Date.now()
                console.log("SMOKE cancel requested " + DeviceSync.snapshot)
                named("deviceCancel").clicked()
                phase = 5
            } else check(status.phase !== "done", "upload finished before cancellation could be exercised")
            return
        }
        if (DeviceSync.busy || !document.has_profiles) return
        if (phase !== 5 && phase !== 6) check(!status.error, "native task failed: " + JSON.stringify(status.error))
        if (phase === 1) {
            if (LibraryFolders.scanning || !tracks.ready) return
            tracks.selectAll()
            const ids = JSON.parse(tracks.selectedTrackIdsJson())
            if (ids.length === 0) return
            check(ids.length === 1, "expected exactly one large fixture track")
            const notNow = find(mainWindow.contentItem, item => item.text === I18n.tr("Not now", I18n.rev) && typeof item.clicked === "function" && item.visible)
            if (notNow) notNow.clicked()
            named("deviceDestination").picked("SMOKE_STORAGE")
            DeviceSync.useTracks(JSON.stringify(ids))
            phase = 2
        } else if (phase === 2) {
            check(document.options.some(row => row.id === "library-selection" && row.checked), "Library selection did not reach sync")
            named("devicePreview").clicked()
            phase = 3
        } else if (phase === 3) {
            check(status.phase === "review" && status.copies === 2 && status.unchanged === 3 && status.removals === 0,
                  "unexpected cancellation review " + DeviceSync.snapshot)
            named("deviceSyncNow").clicked()
            phase = 4
        } else if (phase === 5) {
            check(status.phase === "cancelled" && status.error && status.error.recovery && !status.plan,
                  "cancel did not preserve recovery " + DeviceSync.snapshot)
            console.log("SMOKE cancel settled milliseconds=" + (Date.now()-cancelAt) + " " + DeviceSync.snapshot)
            phase = 6
            mainWindow.contentItem.grabToImage(function(result) {
                result.saveToFile("SMOKE_CAPTURE")
                named("deviceRecover").clicked()
            })
        } else if (phase === 6) {
            if (status.phase === "cancelled") return
            check(!status.error && status.phase === "review" && status.copies === 2 && status.unchanged === 3,
                  "native recovery did not restore review " + DeviceSync.snapshot)
            applyAt = Date.now()
            named("deviceSyncNow").clicked()
            phase = 7
        } else if (phase === 7) {
            check(status.phase === "done" && status.completed === 2, "recovered transfer failed")
            console.log("SMOKE recovered large transfer milliseconds=" + (Date.now()-applyAt) + " " + DeviceSync.snapshot)
            music()
            phase = 8
        } else if (phase === 8) {
            const row = find(mainWindow.contentItem, item => item.objectName === "deviceSelectionCheck" && item.text === I18n.tr("Library selection", I18n.rev))
            if (!row) return
            row.click()
            if (!status.remove_unselected) named("deviceRemoveUnselected").click()
            named("devicePreview").clicked()
            phase = 9
        } else if (phase === 9) {
            check(status.phase === "review" && status.removals === 2 && status.unchanged === 3 && status.copies === 0,
                  "cleanup did not preserve the shared selected files " + DeviceSync.snapshot)
            check(!named("deviceSyncNow").enabled, "cleanup skipped approval")
            named("deviceApproveRemovals").click()
            named("deviceSyncNow").clicked()
            phase = 10
        } else if (phase === 10) {
            check(status.phase === "done", "approved fixture cleanup failed")
            named("devicePreview").clicked()
            phase = 11
        } else if (phase === 11) {
            check(document.change_count === 0 && status.unchanged === 3, "post-recovery repeat changed files")
            console.log("SMOKE device cancel recovery and cleanup completed " + DeviceSync.snapshot)
            stopped = true
            Qt.exit(0)
        }
    }
    Timer {
        interval: 100; repeat: true; running: !smoke.stopped
        onTriggered: { try { smoke.advance() } catch (error) { smoke.stopped = true; console.error("SMOKE FAIL cancellation phase=" + smoke.phase + " " + error.message); Qt.exit(1) } }
    }
    Timer { interval: 300000; running: !smoke.stopped; onTriggered: { console.error("SMOKE FAIL cancellation timeout phase=" + smoke.phase + " " + DeviceSync.snapshot); Qt.exit(1) } }
}
