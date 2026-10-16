import QtQuick
import QtQuick.Effects
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// The desktop's wallpaper behind the window (mode 3), as QBZ's Qt shell
// draws it: the part of the picture that lies UNDER this window, wherever
// the window is, so the window reads as translucent to the desktop. Lightly
// blurred beneath the half-alpha chrome, so nothing in the picture competes
// with the text while it stays recognizable. Nothing animates: it costs a
// frame only when the window moves or resizes, and a screen-sized texture at
// most.
//
// Where the window is: X11, macOS and Windows tell Qt. Wayland hides it from
// clients; on KDE Plasma the compositor reports every window's place through
// org_kde_plasma_window_management, published as `windowsJson`, and the
// rectangle whose size is this window's is the one the picture sits under.
// Where neither can say, the crop is centred on the screen.
Item {
    id: field

    // file URL of the wallpaper.
    property url source
    // The window this field is drawn in.
    property var window: null
    // This process's windows as the compositor reports them: `[{x,y,w,h}]`.
    property string windowsJson: "[]"
    // MultiEffect blur, 0 crisp to 1 strongest.
    property real blur: 0
    // Darkening on top: the dark theme's legibility veil.
    property real dim: 0
    // Writable so a test takes the Wayland branch under the offscreen platform.
    property bool isWayland: Qt.platform.pluginName === "wayland"

    // Whoever mounts the field starts the compositor's report when the field
    // is painting on a Wayland session.
    signal trackAsked()

    clip: true

    function track() {
        if (field.visible && field.isWayland && field.source.toString() !== "")
            field.trackAsked()
    }
    onVisibleChanged: track()
    onSourceChanged: track()
    onIsWaylandChanged: track()
    Component.onCompleted: track()

    // Read only while this field is the background: the report keeps coming
    // after the mode is switched away, and an invisible item whose geometry
    // changes still dirties the window.
    readonly property var reported: {
        if (!field.visible || !field.isWayland)
            return []
        try {
            const parsed = JSON.parse(field.windowsJson || "[]")
            return Array.isArray(parsed) ? parsed : []
        } catch (e) {
            return []
        }
    }
    readonly property var compositorRect: pickRect(reported, window ? window.width : width, window ? window.height : height)

    // The rectangle whose size is closest to this window's, when it is close
    // enough to be it at all: another size is another of our windows, and
    // the centred crop beats that window's place.
    function pickRect(rectangles, w, h) {
        let best = null
        let bestScore = 0
        for (let i = 0; i < rectangles.length; ++i) {
            const r = rectangles[i]
            if (!r || typeof r.w !== "number" || typeof r.h !== "number" || typeof r.x !== "number" || typeof r.y !== "number")
                continue
            const score = Math.abs(r.w - w) + Math.abs(r.h - h)
            if (best === null || score < bestScore) {
                best = r
                bestScore = score
            }
        }
        return best !== null && bestScore <= Metrics.wallpaperWindowTolerance ? best : null
    }

    readonly property bool positionKnown: visible && window !== null && (!isWayland || compositorRect !== null)
    readonly property real screenW: Math.max(1, Screen.width)
    readonly property real screenH: Math.max(1, Screen.height)
    // The window's origin inside its screen; centred when unknown.
    readonly property real offX: !positionKnown ? Math.max(0, (screenW - width) / 2) // derived: centered on the screen
        : (isWayland ? compositorRect.x : window.x) - Screen.virtualX
    readonly property real offY: !positionKnown ? Math.max(0, (screenH - height) / 2) // derived: centered on the screen
        : (isWayland ? compositorRect.y : window.y) - Screen.virtualY

    Image {
        id: picture
        source: field.source
        x: -field.offX
        y: -field.offY
        width: field.screenW
        height: field.screenH
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        cache: false
        sourceSize.width: Math.round(field.screenW * Screen.devicePixelRatio)
        sourceSize.height: Math.round(field.screenH * Screen.devicePixelRatio)
        smooth: true
    }
    MultiEffect {
        visible: field.blur > 0 && picture.status === Image.Ready
        source: picture
        x: picture.x
        y: picture.y
        width: picture.width
        height: picture.height
        autoPaddingEnabled: false
        blurEnabled: true
        blur: field.blur
        blurMax: Metrics.wallpaperBlurMax
    }
    Rectangle {
        anchors.fill: parent
        color: Theme.color.ambientVeil
        opacity: field.dim
    }
}
