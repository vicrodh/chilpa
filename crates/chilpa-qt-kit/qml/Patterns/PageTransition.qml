import QtQuick
import "../Theme"

// I keep the current page visible while the next one is built. A page may
// still load its own data after construction, but navigation never exposes
// an empty Loader or repeatedly remounts the page being left.
Item {
    id: transition
    property string route: ""
    property Component sourceComponent
    property string committedRoute: ""
    readonly property var item: (frontA ? first : second).item
    property bool frontA: true
    property string pendingRoute: ""
    property Component pendingComponent

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
            revealGuard.stop()
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
        next.sourceComponent = null
        next.sourceComponent = sourceComponent
        revealGuard.restart()
    }

    function present(next, timedOut) {
        const current = frontA ? first : second
        if (next === current || next.sourceComponent !== pendingComponent || pendingRoute !== route)
            return
        if (!next.item)
            return
        if (!timedOut && next.item.visualReady === false)
            return
        revealGuard.stop()
        frontA = next === first
        committedRoute = pendingRoute
        pendingRoute = ""
        current.sourceComponent = null
    }

    Loader {
        id: first
        anchors.fill: parent
        asynchronous: true
        visible: transition.frontA
        onLoaded: transition.present(first)
    }
    Connections {
        target: first.item
        ignoreUnknownSignals: true
        function onVisualReadyChanged() { transition.present(first) }
    }
    Loader {
        id: second
        anchors.fill: parent
        asynchronous: true
        visible: !transition.frontA
        onLoaded: transition.present(second)
    }
    Connections {
        target: second.item
        ignoreUnknownSignals: true
        function onVisualReadyChanged() { transition.present(second) }
    }
    Timer {
        id: revealGuard
        interval: 2500
        onTriggered: transition.present(transition.frontA ? second : first, true)
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
