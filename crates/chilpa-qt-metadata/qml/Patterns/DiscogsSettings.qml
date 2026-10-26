import QtQuick
import app.chilpa.kit
import app.chilpa.metadata
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// I follow a pending account only while its settings page is actually shown.
Item {
    id: watcher
    required property var rows
    property bool current: false
    property var ownerWindow: Window.window
    readonly property var lifetime: ({alive:true})
    readonly property bool seen: current && ownerWindow?.visible === true && ownerWindow.visibility !== Window.Minimized
    // I leave construction and model notifications before updating another model.
    function read(life) { if (life.alive && seen) DiscogsAccount.refresh() }
    function reload(life) { if (life.alive && seen) rows.refresh() }
    function schedule() { if (seen) Qt.callLater(watcher.read,lifetime) }
    onSeenChanged: schedule()
    Component.onCompleted: schedule()
    Component.onDestruction: lifetime.alive = false
    Connections {
        target: Prefs
        function onRevChanged() { watcher.schedule() }
    }
    Connections {
        target: DiscogsAccount
        function onRevisionChanged() { if (watcher.seen) Qt.callLater(watcher.reload,watcher.lifetime) }
    }
    Timer {
        objectName: "discogsSettingsRefresh"
        interval: Motion.refresh; repeat: true
        running: watcher.seen && DiscogsAccount.busy
        onTriggered: DiscogsAccount.refresh()
    }
}
