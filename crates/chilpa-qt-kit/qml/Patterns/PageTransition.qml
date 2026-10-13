import QtQuick
import "../Theme"

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
        if (route === committedRoute && current.sourceComponent === sourceComponent) {
            pendingRoute = ""
            pendingComponent = null
            next.sourceComponent = null
            return
        }
        if (pendingRoute === route && pendingComponent === sourceComponent
                && next.sourceComponent === sourceComponent)
            return
        pendingRoute = route
        pendingComponent = sourceComponent
        requestedAt = Date.now()
        next.sourceComponent = null
        next.sourceComponent = sourceComponent
    }

    function present(next) {
        const current = frontA ? first : second
        if (next === current || next.sourceComponent !== pendingComponent || pendingRoute !== route)
            return
        if (!next.item)
            return
        frontA = next === first
        committedRoute = pendingRoute
        pendingRoute = ""
        awaitingFrame = true
        current.sourceComponent = null
    }

    Loader {
        id: first
        anchors.fill: parent
        asynchronous: true
        visible: transition.frontA
        onLoaded: transition.present(first)
    }
    Loader {
        id: second
        anchors.fill: parent
        asynchronous: true
        visible: !transition.frontA
        onLoaded: transition.present(second)
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
