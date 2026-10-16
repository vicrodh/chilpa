import QtQuick
import QtQuick.Window
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Window {
    id: window
    width: 624
    height: 224
    visible: true
    color: "#101820"
    property string outputDirectory: "OUTPUT_DIRECTORY"
    property string expectedBackend: "EXPECTED_BACKEND"
    property url localSource: "LOCAL_SOURCE"
    property url remoteSource: "REMOTE_SOURCE"
    property int readyFrames: 0
    property bool capturing: false

    Rectangle { anchors.fill: parent; color: window.color }
    ArtTile { id: local; x: 24; y: 24; width: 160; height: 160; radius: 24; edged: false; source: window.localSource }
    ArtTile { id: remote; x: 232; y: 24; width: 160; height: 160; radius: 24; edged: false; source: window.remoteSource }
    ArtTile { id: contained; x: 440; y: 24; width: 160; height: 160; radius: 24; edged: false; contain: true; source: local.ready ? window.localSource : "" }

    Timer {
        interval: 50
        repeat: true
        running: !window.capturing
        onTriggered: {
            const expected = window.expectedBackend === "vulkan" ? GraphicsInfo.Vulkan : GraphicsInfo.OpenGL
            if (window.GraphicsInfo.api !== expected) {
                console.error("SMOKE FAIL covers: requested GPU backend was not selected")
                Qt.exit(1)
                return
            }
            if (!local.ready || !remote.ready || !contained.ready)
                return
            if (++window.readyFrames < 8)
                return
            window.capturing = true
            window.contentItem.grabToImage(result => {
                if (!result.saveToFile(window.outputDirectory + "/covers.png")) {
                    console.error("SMOKE FAIL covers: cannot save rendered covers")
                    Qt.exit(1)
                    return
                }
                console.log("SMOKE covers completed: local, URL, contained; renderer=" + window.expectedBackend)
                Qt.quit()
            })
        }
    }
    Timer {
        interval: 10000
        running: true
        onTriggered: {
            console.error("SMOKE FAIL covers: timed out waiting for image pixels; local=" + local.ready
                + ", URL=" + remote.ready + ", contained=" + contained.ready)
            Qt.exit(1)
        }
    }
}
