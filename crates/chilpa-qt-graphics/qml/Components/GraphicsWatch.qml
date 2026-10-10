import QtQuick
import app.chilpa.graphics

// QBZ's renderer probe and frame-liveness watchdog (Main.qml:300-354).
//
// The probe is the one place that answers what actually draws: the renderer
// preference runs before the application object and knows only what was
// asked for, and a driver that refuses it lands somewhere else. The api is
// said by name, so Rust never depends on GraphicsInfo's numbers.
//
// The watchdog disarms the startup sentinels on frames, not on a window: a
// backend can create a window and never present, which looks like a healthy
// start until the pane is frozen. It counts the window's swaps for the
// liveness window, reports once and lets the window go. Its first swap also
// tells the crash chain that the window showed what it restored.
Item {
    id: watch
    required property Window window
    width: 0
    height: 0

    readonly property int api: GraphicsInfo.api
    function report() {
        let name = "unknown"
        switch (watch.api) {
        case GraphicsInfo.Software: name = "software"; break
        case GraphicsInfo.OpenGL: name = "opengl"; break
        case GraphicsInfo.Direct3D11: name = "d3d11"; break
        case GraphicsInfo.Direct3D12: name = "d3d12"; break
        case GraphicsInfo.Vulkan: name = "vulkan"; break
        case GraphicsInfo.Metal: name = "metal"; break
        case GraphicsInfo.Null: name = "null"; break
        }
        Graphics.reportRendererApi(name)
    }
    onApiChanged: report()
    Component.onCompleted: report()

    property int frames: 0
    property bool reported: false
    Connections {
        target: watch.reported ? null : watch.window
        function onFrameSwapped() {
            if (watch.frames === 0)
                Graphics.startupShown()
            watch.frames += 1
        }
    }
    Timer {
        interval: Graphics.livenessWindow
        running: true
        onTriggered: {
            watch.reported = true
            Graphics.reportFrameLiveness(watch.frames)
        }
    }
}
