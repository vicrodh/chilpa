import QtQuick
import app.chilpa
import app.chilpa.folders
import app.chilpa.kit
import app.chilpa.query
import app.chilpa.devices

Item {
    id: smoke
    property var mainWindow: null
    property int phase: 0
    property bool stopped: false
    property string lastStatus: ""
    property var ids: []
    property int playlistId: 0
    property int navigationStep: 0
    property string storage: "SMOKE_STORAGE"
    property string folder: "SMOKE_FOLDER"
    property string profileName: "SMOKE_PROFILE"
    TrackListModel { id: tracks; kind: "tracks"; key: "album\u001foff\u001fDAP"; filterRev: Library.contentRev }
    SettingsPage { id: navigationSettings; section: "navigation" }
    function check(condition, message) { if (!condition) throw new Error(message) }
    function find(item, predicate) {
        if (predicate(item)) return item
        for (const child of item.children || []) { const found = find(child, predicate); if (found) return found }
        return null
    }
    function named(name) {
        const item = find(mainWindow.contentItem, item => item.objectName === name && item.visible)
        check(!!item, "missing control " + name)
        return item
    }
    function advance() {
        check(!Player.playing, "unexpected playback")
        if (phase === 0) {
            const component = Qt.createComponent("qrc:/qt/qml/app/chilpa/qml/Main.qml")
            check(component.status === Component.Ready, component.errorString())
            mainWindow = component.createObject(null)
            check(!!mainWindow, "real Main failed to construct")
            mainWindow.width = 1100; mainWindow.height = 800
            Qt.createQmlObject('import QtQuick; Rectangle { anchors.fill: parent; z: -100 }', mainWindow.contentItem).color = Qt.binding(() => mainWindow.color)
            LibraryFolders.add("SMOKE_MEDIA_ROOT")
            phase = 1
            return
        }
        if (phase === 1) {
            if (LibraryFolders.scanning || !tracks.ready || tracks.count < 2) return
            const notNow = find(mainWindow.contentItem, item => item.text === I18n.tr("Not now", I18n.rev) && typeof item.clicked === "function" && item.visible)
            if (notNow) notNow.clicked()
            tracks.selectAll()
            ids = JSON.parse(tracks.selectedTrackIdsJson())
            check(ids.length === 2, "expected exactly two native Library tracks: " + JSON.stringify(ids))
            playlistId = Playlists.createFull("DAP Daily", "Native sync fixture", "", "", 0, JSON.stringify(ids))
            check(playlistId > 0, "could not create the real local playlist")
            check(!JSON.parse(Playlists.addTrackIds(playlistId, JSON.stringify([ids[0]]), true)).rejected, "duplicate playlist occurrence rejected")
            check(SavedQueries.save("DAP Query", "title:DAP") === "", "saved query failed")
            Navigation.go("deviceSync", 0)
            phase = 2
            return
        }
        const status = JSON.parse(DeviceSync.snapshot || "{}")
        const document = JSON.parse(DeviceSync.catalog || "{}")
        if (status.phase !== lastStatus) {
            console.log("SMOKE device phase=" + phase + " snapshot=" + JSON.stringify(status))
            lastStatus = status.phase || ""
        }
        if (DeviceSync.busy || (!document.destinations && phase !== 8)) return
        check(!status.error, "device task failed: " + JSON.stringify(status.error))
        if (phase === 2) {
            const entry = find(mainWindow.contentItem, item => item.objectName === "deviceNavigation")
            check(!!entry, "device shortcut missing")
            if (navigationStep === 0) {
                check(entry.visible, "device shortcut should default on")
                navigationSettings.set("navigation.devices", "false")
                navigationStep = 1
                return
            }
            if (navigationStep === 1) {
                const hidden = find(mainWindow.contentItem, item => item.objectName === "deviceNavigation")
                check(hidden && !hidden.visible, "device shortcut did not respect the opt-out")
                navigationSettings.set("navigation.devices", "true")
                navigationStep = 2
                return
            }
            check(entry.visible, "device shortcut did not return")
            const destination = document.destinations.find(row => row.id === storage)
            check(!!destination && destination.writable, "expected writable storage missing: " + JSON.stringify(document.destinations))
            named("deviceDestination").picked(storage)
            if (storage === "mounted") {
                const form = find(mainWindow.contentItem, item => typeof item.chosenFolder === "string" && typeof item.createAsked === "function")
                check(!!form, "real pairing form missing")
                form.chosenFolder = "file://" + folder
            } else named("deviceManagedFolder").text = folder
            named("deviceProfileName").text = profileName
            named("deviceCreateProfile").clicked()
            phase = 3
        } else if (phase === 3) {
            check(status.profile && document.has_profiles, "profile was not saved")
            const option = document.options.find(row => row.name === "DAP Daily")
            check(!!option, "native playlist not offered")
            const row = find(mainWindow.contentItem, item => item.objectName === "deviceSelectionCheck" && (item.text === "DAP Daily" || item.parent.entry?.title === "DAP Daily"))
            if (!row) return
            row.clicked()
            phase = 4
        } else if (phase === 4) {
            check(document.options.find(row => row.name === "DAP Daily").checked, "native selection not checked")
            named("devicePreview").clicked()
            phase = 5
        } else if (phase === 5) {
            check(status.phase === "review" && status.plan && status.copies === 3 && status.removals === 0,
                  "first review should copy two tracks and one playlist: " + JSON.stringify(status))
            console.log("SMOKE device review rows=" + DeviceSync.pageRows)
            named("deviceSyncNow").clicked()
            phase = 6
        } else if (phase === 6) {
            check(status.phase === "done" && status.completed === 3, "native sync did not complete: " + JSON.stringify(status))
            named("devicePreview").clicked()
            phase = 7
        } else if (phase === 7) {
            check(status.phase === "review" && status.copies === 0 && status.updates === 0 && status.removals === 0 && status.unchanged === 3,
                  "repeat sync should have no writes: " + JSON.stringify(status))
            check(!named("deviceSyncNow").enabled, "no-change review enabled sync")
            console.log("SMOKE device no-change completed " + JSON.stringify(status))
            mainWindow.contentItem.grabToImage(function(result) { result.saveToFile("SMOKE_CAPTURE_PREFIX" + (storage === "mounted" ? "mounted" : "mtp") + ".png"); Navigation.go("library", 0) })
            phase = 8
        } else if (phase === 8) {
            if (Navigation.screen !== "library" || DeviceSync.catalog !== "") return
            Navigation.go("deviceSync", 0)
            phase = 9
        } else if (phase === 9) {
            if (!document.has_profiles) return
            check(status.phase !== "review" && !status.plan, "closed review remained applicable")
            console.log("SMOKE device real Main completed")
            stopped = true
            Qt.exit(0)
        }
    }
    Timer {
        interval: 300; repeat: true; running: !smoke.stopped
        onTriggered: { try { smoke.advance() } catch (error) { smoke.stopped = true; console.error("SMOKE FAIL device phase=" + smoke.phase + " " + error.message); Qt.exit(1) } }
    }
    Timer { interval: 300000; running: !smoke.stopped; onTriggered: { console.error("SMOKE FAIL device timeout phase=" + smoke.phase + " " + DeviceSync.snapshot); Qt.exit(1) } }
}
