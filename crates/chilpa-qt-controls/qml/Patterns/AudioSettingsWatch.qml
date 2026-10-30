import QtQuick
import app.chilpa.controls

// The audio settings rows follow the outputs as listed only while they are
// seen: the listing is watched from the moment the page is shown to the
// moment it is left, and the rows are rebuilt when it lands or changes.
Item {
    id: watcher
    required property var rows
    property bool current: false
    property var ownerWindow: Window.window
    readonly property bool seen: current && ownerWindow?.visible === true && ownerWindow.visibility !== Window.Minimized
    onSeenChanged: AudioOutput.watchOutputs(seen)
    Component.onCompleted: if (seen) AudioOutput.watchOutputs(true)
    Component.onDestruction: if (seen) AudioOutput.watchOutputs(false)
    Connections {
        target: AudioOutput
        function onOutputSelected() { if (watcher.seen) watcher.rows.refresh() }
        function onOutputsListed() { if (watcher.seen) watcher.rows.refresh() }
    }
}
