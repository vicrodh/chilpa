pragma Singleton
import QtQuick

// The clock as the tests see it: it ticks while something holds it.
QtObject {
    property int ms: 0
    property int period: 33
    property int held: 0
    property bool automatic: true
    function hold() { held += 1 }
    function release() { held = Math.max(0, held - 1) }
    // The slow holds are counted apart too, so a test can tell them.
    property int slowHeld: 0
    function holdSlow() { slowHeld += 1; hold() }
    function releaseSlow() { slowHeld = Math.max(0, slowHeld - 1); release() }
    readonly property Timer clock: Timer {
        interval: period
        repeat: true
        running: automatic && held > 0
        onTriggered: ms += period
    }
}
