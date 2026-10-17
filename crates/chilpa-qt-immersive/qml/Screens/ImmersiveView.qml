import QtQuick
import QtQuick.Window
import app.chilpa
import app.chilpa.ambient
import app.chilpa.immersive
import app.chilpa.graphics
import "../Patterns"
import "qrc:/qt/qml/app/chilpa/ambient/qml/Components" as Fields

// I keep window ownership and renderer availability alive between openings.
// I release the panels, images and effects when the view closes.
Item {
    id: view
    property var window: null
    property var controls: null
    readonly property bool shaders: GraphicsInfo.api !== GraphicsInfo.Software && GraphicsInfo.api !== GraphicsInfo.Null
    readonly property bool windowShowing: window === null || (window.visibility !== Window.Minimized && window.visibility !== Window.Hidden)
    readonly property bool presented: Immersive.open && windowShowing
    readonly property var content: body.item
    property bool wasFullScreen: false
    visible: Immersive.open
    focus: Immersive.open
    Keys.forwardTo: body.item ? [body.item] : []

    Binding { target: view.controls; property: "immersiveOpen"; value: Immersive.open; when: view.controls !== null }
    Connections {
        target: view.controls
        ignoreUnknownSignals: true
        function onImmersiveAsked() { Immersive.toggle() }
    }
    onShadersChanged: Immersive.setAvailable(Graphics.gpuTier && shaders)
    Connections {
        target: Graphics
        function onGpuTierChanged() { Immersive.setAvailable(Graphics.gpuTier && view.shaders) }
    }
    onPresentedChanged: Visualizer.setPresented(presented)
    Component.onCompleted: {
        Immersive.setAvailable(Graphics.gpuTier && shaders)
        Visualizer.setPresented(presented)
        if (Immersive.devOpen) Immersive.open = true
        if (Immersive.open) wasFullScreen = window !== null && window.visibility === Window.FullScreen
    }
    Component.onDestruction: Visualizer.setPresented(false)
    Connections {
        target: Immersive
        function onSceneChanged() { if (Immersive.open && Immersive.scene === 4) Visualizer.setCursor(Player.path, Player.position, Player.duration) }
        function onOpenChanged() {
            if (Immersive.open) {
                view.wasFullScreen = view.window !== null && view.window.visibility === Window.FullScreen
            } else {
                if (!view.wasFullScreen && view.window !== null && view.window.visibility === Window.FullScreen)
                    view.window.visibility = Window.Windowed
                if (view.window !== null) view.window.contentItem.forceActiveFocus()
            }
        }
    }
    Fields.CoverColors { consumer: "immersive"; active: view.presented }
    Loader {
        id: body
        anchors.fill: parent
        active: Immersive.open
        sourceComponent: ImmersiveContent { window: view.window; controls: view.controls; shaders: view.shaders }
    }
}
