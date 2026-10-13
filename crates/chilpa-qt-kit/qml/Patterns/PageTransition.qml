import QtQuick
import "../Theme"

// I show the destination on the next presented frame. Building a full page
// synchronously in the click handler would delay any visual acknowledgement.
Item {
    id: transition
    property string route: ""
    property Component sourceComponent
    property string committedRoute: ""
    readonly property var item: page.item
    property bool armed: false

    onRouteChanged: {
        if (route === committedRoute)
            return
        armed = true
        reveal.stop()
        page.opacity = 0
        commitGuard.restart()
    }
    onSourceComponentChanged: {
        if (!armed && committedRoute === route)
            committedPage = sourceComponent
    }
    property Component committedPage
    function commit() {
        if (!armed)
            return
        commitGuard.stop()
        const previous = committedPage
        committedRoute = route
        committedPage = sourceComponent
        armed = false
        if (previous === committedPage && page.status === Loader.Ready) {
            revealGuard.stop()
            if (Motion.reduced) page.opacity = 1
            else reveal.restart()
        } else if (page.status !== Loader.Ready) {
            revealGuard.restart()
        }
    }
    Connections {
        target: transition.Window.window
        ignoreUnknownSignals: true
        function onFrameSwapped() { transition.commit() }
    }
    Timer { id: commitGuard; interval: 250; onTriggered: transition.commit() }
    Timer { id: revealGuard; interval: 1500; onTriggered: page.opacity = 1 }
    OpacityAnimator {
        id: reveal
        target: page
        from: 0
        to: 1
        duration: Motion.page
        easing.type: Easing.OutCubic
    }
    Loader {
        id: page
        anchors.fill: parent
        asynchronous: true
        sourceComponent: transition.committedPage
        onLoaded: {
            if (transition.armed) return
            revealGuard.stop()
            if (Motion.reduced) page.opacity = 1
            else reveal.restart()
        }
    }
    Column {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: Metrics.contentX
        spacing: Metrics.gap16
        visible: transition.armed || (page.status === Loader.Loading)
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
