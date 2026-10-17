import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.mini
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/ambient/qml/Components" as Fields
import "qrc:/qt/qml/app/chilpa/ambient/qml/Patterns" as Backdrop
import "../Patterns"
import "../Theme"

// Where the mini player lives in the main window (02-mini D8). Its window
// is made the first time it is asked for and kept for the life of the
// process, hidden between uses. Opening the mini player shows and raises
// it and hides the main window LAST, so nothing flashes blank; closing it
// hides it and brings the main window back, raised, where it was. The
// window's controls say when a key or the desktop asked for the mini
// player, and are told while it is up, so the desktop's asks for the
// window reach the window that is showing.
Item {
    id: host

    required property Window window
    // The window's controls: their asks, and the word that puts the window
    // back in its place.
    property var controls: null
    // Whether the development hook may open the mini player at start.
    property bool hooked: true
    property var mini: null

    Component { id: maker; MiniWindow { backdrop: backdropMaker } }

    // Behind the card's surfaces (02-mini §1.2): the app's own background
    // when the card inherits it, or the cover's atmosphere, still, under a
    // veil of the window's surface, when the card's own blur is on.
    Component {
        id: backdropMaker
        Item {
            readonly property url atmosphere: Player.art.toString().startsWith("file:") ? "image://atmosphere/" + encodeURIComponent(Player.art.toString()) : ""
            Loader {
                anchors.fill: parent
                active: Mini.open && Theme.ambient && !Mini.backgroundBlur
                sourceComponent: Backdrop.AppBackground { paletteConsumer: "mini"; plane: Qt.rect(0, 0, 0, 0) }
            }
            Loader {
                anchors.fill: parent
                active: Mini.open && Mini.backgroundBlur && Player.path !== ""
                sourceComponent: Fields.AtmosphereField {
                    source: parent.atmosphere
                    fallbackSource: Player.art
                    animated: false
                    dim: Theme.dark ? Card.blurDim : 0
                    baseOpacity: Card.blurBase
                    warpOpacity: Card.blurWarp
                }
            }
            Rectangle {
                anchors.fill: parent
                visible: Mini.backgroundBlur
                color: Theme.color.surfaceWindow
                opacity: Card.blurVeil
            }
        }
    }

    function ensure() {
        if (host.mini === null)
            host.mini = maker.createObject(null)
        return host.mini
    }
    function present() {
        const shown = host.ensure()
        shown.show()
        if (shown.visibility === Window.Minimized)
            shown.showNormal()
        shown.raise()
        shown.requestActivate()
    }

    Binding {
        target: host.controls
        property: "miniOpen"
        value: Mini.open
        when: host.controls !== null
    }
    Connections {
        target: host.controls
        ignoreUnknownSignals: true
        function onMiniAsked() { Mini.toggle() }
        function onPresentMini() { host.present() }
        function onQuitMini() { Mini.closeApp() }
    }
    // The development hooks' frame recorder counts this window's frames
    // too, so a measurement of the mini player measures the mini player.
    Connections {
        target: host.mini
        enabled: Dev.frames
        ignoreUnknownSignals: true
        function onFrameSwapped() { Dev.frame() }
    }
    Connections {
        target: Mini
        function onOpenChanged() {
            if (Mini.open) {
                host.present()
                host.window.hide()
            } else if (host.mini !== null) {
                host.mini.hide()
                host.window.show()
                host.window.raise()
                host.window.requestActivate()
                if (host.controls)
                    host.controls.restorePlace()
            }
        }
        function onCloseAsked() { host.window.close() }
    }

    Component.onCompleted: {
        if (host.hooked && Mini.devSurface >= 0) {
            Mini.surface = Mini.devSurface
            Mini.enter()
        }
    }
}
