pragma Singleton
import QtQuick

// The window's kept place, as a test states it, and every frame the window
// reported.
QtObject {
    property int width: 1280
    property int height: 800
    property int left: 0
    property int top: 0
    property bool placed: false
    property bool maximized: false
    property int smallestWidth: 960
    property int smallestHeight: 600
    property var kept: []

    function keep(frame, width, height, left, top, placed) {
        kept.push([frame, width, height, left, top, placed])
        keptChanged()
    }
}
