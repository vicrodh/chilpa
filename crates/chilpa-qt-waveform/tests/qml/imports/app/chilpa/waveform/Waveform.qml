pragma Singleton
import QtQuick

// The waveform's bridge as the tests see it: a document the test writes.
QtObject {
    property bool open: false
    property int columns: 0
    property var values: []
    property var lows: []
    property real analyzed: 0
    property bool complete: false
    property int refreshes: 0
    function refresh() { refreshes += 1 }
}
