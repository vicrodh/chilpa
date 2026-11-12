import QtQuick
import QtQuick.Window
import app.chilpa
import app.chilpa.kit
import app.chilpa.offline
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// The window's one look at offline work: while anything waits or runs it
// reads every track's state again a few times a second, so each row says
// how its own track goes wherever the listener is; idle or minimized it
// does nothing. A library that changed underneath is read again once.
Item {
    id: activity
    property bool listening: true
    readonly property bool shown: !Window.window || (Window.window.visible && Window.window.visibility !== Window.Minimized)

    Timer {
        objectName: "offlineActivityPoll"
        interval: Motion.poll
        repeat: true
        running: activity.listening && activity.shown && Offline.active
        onTriggered: Offline.refresh(false)
    }
    Connections {
        target: Library
        enabled: activity.listening
        function onRemoteRevChanged() { Offline.refresh(true) }
    }
}
