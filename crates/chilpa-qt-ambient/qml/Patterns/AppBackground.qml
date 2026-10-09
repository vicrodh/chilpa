import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.ambient
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Components"

// The dynamic background of the window (design/01 1.6), as QBZ's Qt shell
// mounts it: the bottom-most layer, under every chrome surface. One of four
// looks, by the mode kept in Settings: the album-colored field, the blurred
// cover, the desktop's wallpaper, or the wallpaper blurred; the first two
// wait for a track, the wallpaper is always there. Each look carries its
// own legibility veil, dark or light by the theme, and the content plane
// frosts over all of them. With the mode off, or no track for the modes
// that need one, nothing here is mounted and the window is the one it
// always was.
Item {
    id: root

    // The content's rectangle, in the window's coordinates: the frosted
    // plane under the screens.
    property rect plane: Qt.rect(0, 0, 0, 0)

    readonly property bool hasTrack: Player.path !== ""
    readonly property int mode: Ambient.mode
    // THE one predicate: the theme publishes it for every surface that goes
    // translucent, so none of them derives its own.
    readonly property bool on: mode > 0 && (mode >= 3 || hasTrack)
    readonly property bool windowShowing: root.Window.window
        ? (root.Window.window.visibility !== Window.Hidden && root.Window.window.visibility !== Window.Minimized)
        : true
    readonly property bool painting: on && visible && windowShowing
    readonly property bool dark: Theme.dark
    // The veil: black at the dim on dark themes; on light ones the theme's
    // own near-white, a notch stronger, because darkening a light theme
    // lands its text on mid grey.
    readonly property real veilStrength: dark ? Metrics.ambientDim : Metrics.ambientVeilLight
    readonly property real innerDim: dark ? Metrics.ambientDim : 0
    readonly property color veilColor: dark ? Theme.color.ambientVeil : Theme.color.surfaceWindow
    readonly property bool playing: Player.playing && !Player.paused
    // The cover on disk through the atmosphere provider; a cover that is not
    // a file has no atmosphere.
    readonly property url atmosphere: Player.art.toString().startsWith("file:") ? "image://atmosphere/" + encodeURIComponent(Player.art.toString()) : ""
    readonly property real blur: Prefs.previewKey === "appearance.wallpaperBlur" ? Number(Prefs.previewValue) / 100 : Ambient.wallpaperBlur

    onOnChanged: Ambient.on = on
    Component.onCompleted: {
        Ambient.on = on
        Ambient.follow(Player.art)
        if (mode >= 3)
            Ambient.refreshWallpaper()
    }
    onModeChanged: if (mode >= 3) Ambient.refreshWallpaper()

    Connections {
        target: Player
        function onArtChanged() { Ambient.follow(Player.art) }
    }
    Connections {
        target: Prefs
        function onRevChanged() { Ambient.reload() }
    }
    // The wallpaper can change while Chilpa runs: it is resolved again when
    // the window comes back to the front.
    Connections {
        target: root.Window.window
        ignoreUnknownSignals: true
        function onActiveChanged() {
            if (root.Window.window.active && root.mode >= 3)
                Ambient.refreshWallpaper()
        }
    }

    // Mode 1: the field, and its veil.
    AmbientField {
        anchors.fill: parent
        visible: root.painting && root.mode === 1
        running: visible
        primary: Ambient.primary
        secondary: Ambient.secondary
        accent: Ambient.accent
        dim: root.innerDim
    }
    // Mode 2: the blurred cover, drifting while the transport plays.
    AtmosphereField {
        anchors.fill: parent
        visible: root.painting && root.mode === 2
        source: visible ? root.atmosphere : ""
        fallbackSource: visible ? Player.art : ""
        animated: visible && root.playing
        dim: root.innerDim
    }
    // Mode 3: the wallpaper under the window.
    WallpaperField {
        anchors.fill: parent
        visible: root.painting && root.mode === 3
        source: visible ? Ambient.wallpaperUrl : ""
        window: root.Window.window
        windowsJson: Ambient.wallpaperWindowsJson
        blur: root.blur
        dim: root.innerDim
        onTrackAsked: Ambient.trackWindowPosition()
    }
    // Mode 4: the wallpaper through the atmosphere pass.
    AtmosphereField {
        anchors.fill: parent
        visible: root.painting && root.mode === 4
        source: visible ? Ambient.wallpaperAtmosphereUrl : ""
        animated: visible && root.playing
        dim: root.innerDim
    }
    // The veil, over whichever is painting. Dark themes bake it into mode
    // 1's shader and the atmosphere's; they still get it over the wallpaper.
    Rectangle {
        anchors.fill: parent
        visible: root.painting && (!root.dark || root.mode === 1)
        color: root.veilColor
        opacity: root.veilStrength
    }

    // The frosted content plane where the screens draw: no edge, so the
    // field runs into it (the owner, 2026-10-08).
    Rectangle {
        visible: root.painting
        x: root.plane.x
        y: root.plane.y
        width: root.plane.width
        height: root.plane.height
        topLeftRadius: Metrics.radiusPanel
        topRightRadius: Metrics.radiusPanel
        color: Theme.color.ambientContent
    }
}
