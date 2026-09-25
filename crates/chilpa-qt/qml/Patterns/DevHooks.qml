import QtQuick
import com.blitzfc.chilpa
import "../Theme"

// The measuring hooks, inert unless an environment variable asks for them
// (see bridge/dev.rs): a frame-time recorder, a screenshot, a start route.
Item {
    id: hooks

    required property Window window

    Connections {
        target: hooks.window
        enabled: Dev.frames
        function onFrameSwapped() { Dev.frame() }
    }

    Timer {
        running: Dev.shot !== ""
        interval: Metrics.shotDelay
        onTriggered: hooks.window.contentItem.grabToImage(function(result) {
            result.saveToFile(Dev.shot)
            Qt.quit()
        })
    }

    Component.onCompleted: {
        if (Dev.route !== "")
            Navigation.go(Dev.route, Dev.routeId)
    }
}
