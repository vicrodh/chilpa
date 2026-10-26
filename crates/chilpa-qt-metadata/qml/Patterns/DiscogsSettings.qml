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
    readonly property bool seen: current && ownerWindow?.visible === true && ownerWindow.visibility !== Window.Minimized
    onSeenChanged: if (seen) DiscogsAccount.refresh()
    Component.onCompleted: if (seen) DiscogsAccount.refresh()
    Connections {
        target: Prefs
        function onRevChanged() { if (watcher.seen) DiscogsAccount.refresh() }
    }
    Connections {
        target: DiscogsAccount
        function onRevisionChanged() { if (watcher.seen) watcher.rows.refresh() }
    }
    Timer {
        objectName: "discogsSettingsRefresh"
        interval: Motion.refresh; repeat: true
        running: watcher.seen && DiscogsAccount.busy
        onTriggered: DiscogsAccount.refresh()
    }
}
