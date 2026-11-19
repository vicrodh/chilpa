import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// One thing the window reads before it takes a click (`StartupSplash`): a
// bar that fills as it goes, or sweeps while how far it is cannot be told,
// full once it is done, and a small line that says what it is.
Column {
    id: task

    property string text
    // From 0 to 1; below 0 while how far it is cannot be told.
    property real progress: -1
    property bool done: false

    spacing: Metrics.gap4

    Rectangle {
        id: track
        width: parent.width
        height: Metrics.progress
        radius: 1
        color: Theme.color.strokeHairline
        clip: true

        Rectangle {
            objectName: "startupTaskFill"
            visible: task.done || task.progress >= 0
            width: track.width * (task.done ? 1 : Math.min(1, task.progress))
            height: parent.height
            radius: 1
            color: Theme.color.accent
            Behavior on width { NumberAnimation { duration: Motion.fast } }
        }
        Rectangle {
            id: sweep
            objectName: "startupTaskSweep"
            visible: !task.done && task.progress < 0
            width: track.width / 3 // derived: a third of the track crosses it
            height: parent.height
            radius: 1
            color: Theme.color.accent
            // Only while it shows: nothing hidden pays for a frame.
            NumberAnimation on x { running: sweep.visible; loops: Animation.Infinite; from: -sweep.width; to: track.width; duration: Motion.busyLoop }
        }
    }
    Text {
        objectName: "startupTaskText"
        width: parent.width
        horizontalAlignment: Text.AlignHCenter
        text: task.text
        wrapMode: Text.Wrap
        font.pointSize: Type.micro
        color: Theme.color.textSecondary
    }
}
