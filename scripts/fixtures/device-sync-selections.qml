import QtQuick
import app.chilpa
import app.chilpa.folders
import app.chilpa.kit
import app.chilpa.devices

Item {
    id: smoke
    property var mainWindow: null
    property int phase: 0
    property int album: 0
    property bool stopped: false
    AlbumGridModel { id: albums; kind: "all"; contentRev: Library.contentRev }
    function check(value, message) { if (!value) throw new Error(message) }
    function find(item, name) {
        if (item.objectName === name && item.visible) return item
        for (const child of item.children || []) { const found = find(child, name); if (found) return found }
        return null
    }
    function named(name) { const item = find(mainWindow.contentItem, name); check(!!item, "missing " + name); return item }
    function add(index) {
        check(DeviceSync.useTracks(AlbumActions.deviceTracksJson(albums.albumId(index, albums.rev))), "selection rejected")
        Navigation.go("deviceSync", 0)
    }
    function advance() {
        check(!Player.playing, "unexpected playback")
        if (phase === 0) {
            const component = Qt.createComponent("qrc:/qt/qml/app/chilpa/qml/Main.qml")
            check(component.status === Component.Ready, component.errorString())
            mainWindow = component.createObject(null)
            check(!!mainWindow, "Main failed")
            mainWindow.width = 1100; mainWindow.height = 800
            Navigation.go("library", 0)
            LibraryFolders.add("SMOKE_MEDIA_ROOT")
            phase = 1
            return
        }
        if (phase === 1) {
            if (LibraryFolders.scanning || !albums.ready || albums.albums !== 2) return
            Navigation.go("albums", 0)
            add(0)
            phase = 2
            return
        }
        const status = JSON.parse(DeviceSync.snapshot || "{}")
        const doc = JSON.parse(DeviceSync.catalog || "{}")
        if (DeviceSync.busy) return
        check(!status.error, "device error")
        if (phase === 2) {
            if (!doc.destinations || !doc.options?.length) return
            check(doc.options.filter(row => row.checked).length === 1, "first album missing")
            named("deviceDestination").picked("mounted")
            named("deviceAddStorage").clicked()
            function choose(item) {
                if (typeof item.chosenFolder === "string") { item.chosenFolder = "file://SMOKE_FOLDER"; return true }
                for (const child of item.children || []) if (choose(child)) return true
                return false
            }
            check(choose(mainWindow.contentItem), "pairing folder missing")
            named("deviceProfileName").text = "Selection trips"
            named("deviceCreateProfile").clicked()
            phase = 3
        } else if (phase === 3) {
            if (!status.profile) return
            Navigation.go("albums", 0)
            phase = 4
        } else if (phase === 4) {
            check(DeviceSync.catalog === "", "closed table retained")
            add(1)
            phase = 5
        } else if (phase === 5) {
            if (!doc.options?.length) return
            check(doc.options.filter(row => row.checked).length === 2, "second trip lost first album")
            Navigation.go("albums", 0)
            phase = 6
        } else if (phase === 6) {
            albums.select(0, 1)
            albums.select(1, 1)
            phase = 7
        } else if (phase === 7) {
            if (AlbumActions.selectedCount !== 2) return
            const button = named("albumSelectionDevice")
            check(button.enabled, "local multiselect disabled")
            button.clicked()
            phase = 8
        } else if (phase === 8) {
            if (!doc.options?.length) return
            check(doc.options.filter(row => row.checked).length === 3, "multiselect did not append")
            check(!status.plan, "staging started a review")
            console.log("SMOKE selections real Main completed")
            stopped = true
            Qt.exit(0)
        }
    }
    Timer { interval: 250; repeat: true; running: !smoke.stopped; onTriggered: { try { smoke.advance() } catch (error) { smoke.stopped = true; console.error("SMOKE FAIL selections phase=" + smoke.phase + " " + error.message); Qt.exit(1) } } }
    Timer { interval: 60000; running: !smoke.stopped; onTriggered: { console.error("SMOKE FAIL selections timeout phase=" + smoke.phase); Qt.exit(1) } }
}
