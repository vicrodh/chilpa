pragma Singleton
import QtQuick

// The clock as the tests see it: it ticks while something holds it.
QtObject {
    property int ms: 0
    property int period: 33
    property int held: 0
    function hold() { held += 1 }
    function release() { held = Math.max(0, held - 1) }
    readonly property Timer clock: Timer {
        interval: 33
        repeat: true
        running: held > 0
        onTriggered: ms += 33
    }
}
