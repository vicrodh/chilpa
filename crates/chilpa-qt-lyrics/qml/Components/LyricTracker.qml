import QtQuick
import QtQuick.Controls.Basic
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// I keep the active row at its anchor after text, translation and layout
// settle. Only a real gesture asks to leave follow; layout never does.
Item {
    id: tracker
    property ListView view: null
    property ScrollBar scrollBar: null
    property int current: -1
    property bool following: false
    property bool live: true
    property real anchor: 0.5
    property int duration: Motion.lyricScroll
    property int previous: -1
    property bool animatePending: false
    property bool centering: false
    readonly property bool moving: glide.running
    signal browseRequested()
    visible: false
    width: 0
    height: 0

    function request(animated) {
        if (!live || !following) return
        animatePending = animatePending || animated === true
        Qt.callLater(center)
    }
    function reset() { previous = -1; request(false) }
    function center() {
        if (!view || !live || !following || current < 0 || current >= view.count || centering) return
        centering = true
        const near = previous >= 0 && Math.abs(current - previous) <= 2
        previous = current
        const animate = animatePending && near && !Motion.reduced
        animatePending = false
        view.forceLayout()
        let row = view.itemAtIndex(current)
        if (!row) {
            view.positionViewAtIndex(current, ListView.Center)
            view.forceLayout()
            row = view.itemAtIndex(current)
        }
        if (!row) { centering = false; return }
        const to = row.y + row.height / 2 - view.height * anchor
        if (glide.running && !animate) {
            glide.to = to
        } else if (Math.abs(view.contentY - to) > 0.5) {
            glide.stop()
            if (animate) { glide.to = to; glide.start() }
            else view.contentY = to
        }
        centering = false
    }
    function browse() {
        if (!following) return
        glide.stop()
        animatePending = false
        browseRequested()
    }
    onCurrentChanged: request(true)
    onFollowingChanged: {
        if (following) { if (view) view.cancelFlick(); request(true) }
        else { glide.stop(); animatePending = false }
    }
    onLiveChanged: { if (live) request(false); else glide.stop() }
    onAnchorChanged: request(false)
    NumberAnimation {
        id: glide
        target: tracker.view
        property: "contentY"
        duration: tracker.duration
        easing.type: Easing.InOutCubic
        onFinished: tracker.request(false)
    }
    Connections {
        target: tracker.view
        function onWidthChanged() { tracker.request(false) }
        function onHeightChanged() { tracker.request(false) }
        function onContentHeightChanged() { tracker.request(false) }
        function onCountChanged() { tracker.request(false) }
        function onVisibleChanged() { tracker.request(false) }
        function onDraggingChanged() { if (tracker.view.dragging) tracker.browse() }
        function onFlickingChanged() { if (tracker.view.flicking) tracker.browse() }
        function onMovementEnded() { tracker.request(false) }
        function onContentYChanged() {
            if (!tracker.centering && !tracker.moving && !tracker.view.moving) tracker.request(false)
        }
    }
    Connections { target: Motion; function onReducedChanged() { if (Motion.reduced) { glide.stop(); tracker.request(false) } } }
    Connections { target: tracker.scrollBar; function onPressedChanged() { if (tracker.scrollBar.pressed) tracker.browse() } }
    WheelHandler {
        parent: tracker.view
        target: null
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
        onWheel: event => { tracker.browse(); event.accepted = false }
    }
}
