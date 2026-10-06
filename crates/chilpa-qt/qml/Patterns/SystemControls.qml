import QtQuick
import com.blitzfc.chilpa
import com.blitzfc.chilpa.kit
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"

// On a Mac the window keeps the system's three controls, which float over
// the tab bar's left. Nothing is drawn here: this says how much of the bar
// they take, and has them put at the height of the bar's buttons, where
// AppKit parks them higher. It asks until there is a window to do it to,
// and again whenever the window changes state, which is when AppKit lays
// its title bar out anew and parks them back.
Item {
    id: controls

    property Window window

    readonly property bool kept: Qt.platform.os === "osx"
    // The middle of the bar's buttons, under the window's top.
    readonly property real middle: Metrics.tabBarTop + Metrics.button28 / 2 // derived: half a button under the bar's margin
    // What the system says they take, where it says so.
    property real safeLeft: 0
    readonly property real room: kept ? Math.max(safeLeft, Metrics.windowControlsRoom) : 0

    function place() {
        again.tries = 0
        again.restart()
    }

    // The safe area is read only where there is one to read.
    Loader {
        active: controls.kept && controls.window !== null
        sourceComponent: Item {
            Component.onCompleted: controls.safeLeft = Qt.binding(() => controls.window.SafeArea.margins.left)
        }
    }

    Timer {
        id: again

        property int tries: 0

        interval: Motion.systemControlsRetry
        repeat: true
        running: controls.kept
        onTriggered: {
            tries++
            if (ThemeModel.centerWindowControls(controls.middle) || tries >= Metrics.systemControlsTries)
                running = false
        }
    }

    Connections {
        target: controls.window
        enabled: controls.kept
        // A hidden or minimized window has nothing laid out to move.
        function onVisibilityChanged() {
            const v = controls.window.visibility
            if (v !== Window.Hidden && v !== Window.Minimized)
                controls.place()
        }
    }
}
