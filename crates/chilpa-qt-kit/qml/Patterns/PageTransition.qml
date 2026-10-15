import QtQuick
import "../Theme"
import "../Components"

// I replace the current page as soon as the destination shell is built.
// Each page owns placeholders for its data; a source cannot delay navigation.
Item {
    id: transition
    property string route: ""
    property Component sourceComponent
    property string committedRoute: ""
    readonly property var item: (frontA ? first : second).item
    property bool frontA: true
    property string pendingRoute: ""
    property Component pendingComponent
    property double requestedAt: 0
    property bool awaitingFrame: false
    LoggingCategory { id: timing; name: "chilpa.nav.timing"; defaultLogLevel: LoggingCategory.Warning }

    function request() { Qt.callLater(start) }
    onRouteChanged: request()
    onSourceComponentChanged: request()
    Component.onCompleted: request()

    function start() {
        if (!sourceComponent)
            return
        const current = frontA ? first : second
        const next = frontA ? second : first
        if (current.item?.leaving !== undefined) current.item.leaving = route !== committedRoute
        if (route === committedRoute && current.sourceComponent === sourceComponent) {
            pendingRoute = ""
            pendingComponent = null
            next.scroll.enabled = false
            next.sourceComponent = null
            return
        }
        if (pendingRoute === route && pendingComponent === sourceComponent
                && next.sourceComponent === sourceComponent)
            return
        pendingRoute = route
        pendingComponent = sourceComponent
        requestedAt = Date.now()
        next.scroll.enabled = false
        next.sourceComponent = null
        next.pageRoute = route
        next.sourceComponent = sourceComponent
    }

    function present(next) {
        const current = frontA ? first : second
        if (next === current || next.sourceComponent !== pendingComponent || pendingRoute !== route)
            return
        if (!next.item)
            return
        next.scroll.enabled = true
        next.scroll.restore()
        // I reveal the mounted page on the render thread without holding up
        // navigation, input or the page's data. A new route replaces this fade.
        settleReveal()
        next.opacity = Motion.reduced ? 1 : 0
        frontA = next === first
        committedRoute = pendingRoute
        pendingRoute = ""
        awaitingFrame = true
        current.scroll.remember()
        current.scroll.enabled = false
        current.sourceComponent = null
        if (!Motion.reduced)
            reveal.restart()
    }

    function settleReveal() {
        reveal.stop()
        first.opacity = 1
        second.opacity = 1
    }

    OpacityAnimator {
        id: reveal
        target: transition.frontA ? first : second
        from: 0
        to: 1
        duration: Motion.page
        easing.type: Easing.OutCubic
    }
    Connections {
        target: Motion
        function onReducedChanged() { if (Motion.reduced) transition.settleReveal() }
    }

    Loader {
        id: first
        property string pageRoute: ""
        readonly property alias scroll: firstScroll
        anchors.fill: parent
        asynchronous: true
        visible: transition.frontA
        onLoaded: Qt.callLater(transition.present, first)
        ScrollMemory {
            id: firstScroll
            viewport: first.item && first.item.contentY !== undefined ? first.item : first.item?.scrollView || null
            key: first.pageRoute
        }
    }
    Loader {
        id: second
        property string pageRoute: ""
        readonly property alias scroll: secondScroll
        anchors.fill: parent
        asynchronous: true
        visible: !transition.frontA
        onLoaded: Qt.callLater(transition.present, second)
        ScrollMemory {
            id: secondScroll
            viewport: second.item && second.item.contentY !== undefined ? second.item : second.item?.scrollView || null
            key: second.pageRoute
        }
    }
    Connections {
        target: transition.Window.window
        function onFrameSwapped() {
            if (!transition.awaitingFrame) return
            transition.awaitingFrame = false
            console.info(timing, "navigation: " + transition.committedRoute.split("\u001f")[1]
                + " presented in " + (Date.now() - transition.requestedAt) + "ms")
        }
    }
    Column {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: Metrics.contentX
        spacing: Metrics.gap16
        visible: !transition.item
        Rectangle { width: parent.width * 0.36; height: Metrics.gap16; radius: Metrics.gap8; color: Theme.color.stateHover }
        Repeater {
            model: 5
            Rectangle {
                required property int index
                width: parent.width
                height: Metrics.settingsRow
                radius: Metrics.radiusControl
                color: Theme.color.stateHover
                opacity: index % 2 ? 0.55 : 0.8
            }
        }
    }
}
