pragma Singleton
import QtQuick

// The mini player's bridge as the tests see it.
QtObject {
    property bool open: true
    property int surface: 2
    property bool backgroundBlur: false
    property bool queueHasTarget: true
    property int devSurface: -1
    // What the next key comes to, and what was asked.
    property string answer: ""
    property int entries: 0
    property int exits: 0
    property int likes: 0
    property int closes: 0
    signal closeAsked()
    function enter() { entries += 1; open = true }
    function exit() { exits += 1; open = false }
    function toggle() { if (open) exit(); else enter() }
    function toggleBackgroundBlur() { backgroundBlur = !backgroundBlur }
    function refreshQueueTarget() {}
    function keyPressed(key, modifiers, text) { return answer }
    function likePlaying() { likes += 1 }
    function closeApp() { closes += 1; exit(); closeAsked() }
    function reload() {}
}
