pragma Singleton
import QtQuick
QtObject {
    property string sleepLeft: ""
    property int loopState: 0
    property double loopStart: 0
    property double loopEnd: 0
    property int asked: 0
    function refresh() {}
    function sleepIn(minutes) { asked = minutes; sleepLeft = minutes > 0 ? minutes + "m" : "" }
    function markLoop() { loopState = (loopState + 1) % 3 }
    function clearLoop() { loopState = 0 }
    // Whether a close hides to the tray, as a test states it.
    property bool trayClose: false
    property int relabeled: 0
    function closesToTray() { return trayClose }
    function relabelTray() { relabeled++ }
}
