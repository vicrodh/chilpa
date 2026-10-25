import QtQuick
import app.chilpa.kit
import "../Theme"
import "../Components"

// I paint the destination acknowledgement before I construct its page.
Item {
    id: transition
    property bool resizing: false
    property real heldWidth: -1
    onResizingChanged: if (resizing) heldWidth = width; else heldWidth = -1
    property string route: ""
    property string retainedScreen: ""
    property string componentRoute: ""
    property Component sourceComponent
    property string committedRoute: ""
    property string acknowledgedRoute: ""
    property string phase: "acknowledging"
    readonly property var item: phase === "content" ? (frontA ? first : second).item : null
    property bool frontA: true
    property int generation: 0
    property string pendingRoute: ""
    property Component pendingComponent
    property double requestedAt: 0
    property bool awaitingFrame: false
    property bool awaitingContentFrame: false
    signal acknowledgedFrame(string destination)
    signal preparingRoute()
    LoggingCategory { id: timing; name: "chilpa.nav.timing"; defaultLogLevel: LoggingCategory.Warning }

    function routeKeyOf(loader) { return ((loader?.pageRoute || "").split("\u001f")[3]) || "" }
    function routeIdOf(loader) { return Number((loader?.pageRoute || "").split("\u001f")[2]) || 0 }
    function scrollKeyOf(loader) { const parts = loader.pageRoute.split("\u001f"); return parts[1] === retainedScreen ? parts.slice(0, 3).join("\u001f") : loader.pageRoute }

    function request(force) {
        if (!force && phase === "content" && route === committedRoute
                && (frontA ? first : second).sourceComponent === sourceComponent)
            return
        if (phase === "acknowledging" && acknowledgedRoute === route && awaitingFrame)
            return
        const current = frontA ? first : second
        const from = committedRoute.split("\u001f"), to = route.split("\u001f")
        if (!force && phase === "content" && current.item && retainedScreen && from[0] === to[0]
                && from[1] === retainedScreen && to[1] === retainedScreen && current.sourceComponent === sourceComponent) {
            generation += 1
            settleReveal()
            Qt.callLater(retarget, generation)
            return
        }
        if (phase === "content") current.scroll.remember()
        current.scroll.enabled = false
        const focus = transition.Window.window?.activeFocusItem
        for (let ancestor = focus; ancestor; ancestor = ancestor.parent) {
            if (ancestor === current) { focus.focus = false; break }
        }
        generation += 1
        settleSkeleton()
        awaitingContentFrame = false
        acknowledgedRoute = route
        pendingRoute = route
        requestedAt = Date.now()
        phase = "acknowledging"
        awaitingFrame = true
        frameFence.arm(generation * 2)
    }
    onRouteChanged: request()
    onSourceComponentChanged: {
        if (phase === "content" || phase === "loading") request()
    }
    Component.onCompleted: request()

    // I keep a navigation shell while its own content changes. Deferring
    // coalesces the individual route properties published in the same turn.
    function retarget(epoch) {
        if (epoch !== generation || phase !== "content") return
        const current = frontA ? first : second
        componentRoute = route
        if (epoch !== generation) return
        current.pageRoute = route
        if (epoch !== generation) return
        current.epoch = epoch
        committedRoute = route
        acknowledgedRoute = route
        pendingRoute = ""
        preparingRoute()
        if (epoch !== generation) return
        awaitingContentFrame = true
        frameFence.arm(epoch * 2 + 1)
    }

    // I retain both trees until the acknowledgement has reached the screen.
    function start() { request() }
    function begin(epoch) {
        if (epoch !== generation || phase !== "acknowledging") return
        phase = "resolving"
        preparingRoute()
        if (epoch !== generation) return
        componentRoute = acknowledgedRoute
        if (epoch !== generation) return
        const current = frontA ? first : second
        const next = frontA ? second : first
        if (current.item?.leaving !== undefined) current.item.leaving = true
        if (epoch !== generation) return
        next.scroll.enabled = false
        next.sourceComponent = null
        next.pageRoute = componentRoute
        next.epoch = epoch
        pendingComponent = sourceComponent
        if (!pendingComponent) { phase = "error"; return }
        phase = "loading"
        next.sourceComponent = pendingComponent
    }

    function present(next, epoch) {
        if (epoch !== generation || next.epoch !== generation || phase !== "loading"
                || next.sourceComponent !== pendingComponent || !next.item)
            return
        const current = frontA ? first : second
        next.scroll.enabled = true
        settleReveal()
        next.opacity = Motion.reduced ? 1 : 0
        frontA = next === first
        committedRoute = pendingRoute
        if (epoch !== generation) return
        // Before the phase changes, so the skeleton is never hidden at the mount.
        if (!Motion.reduced) retire.restart()
        awaitingContentFrame = true
        phase = "content"
        if (epoch !== generation) return
        next.scroll.restore()
        if (epoch !== generation) return
        pendingRoute = ""
        current.scroll.enabled = false
        current.sourceComponent = null
        if (!Motion.reduced) {
            reveal.restart()
            revealGuard.restart()
        }
        if (epoch !== generation) return
        frameFence.arm(epoch * 2 + 1)
    }

    function settleReveal() {
        revealGuard.stop()
        reveal.stop()
        first.opacity = 1
        second.opacity = 1
        settleSkeleton()
    }

    function settleSkeleton() {
        retire.stop()
        skeleton.opacity = 1
    }

    OpacityAnimator {
        id: reveal
        target: transition.frontA ? first : second
        from: 0
        to: 1
        duration: Motion.page
        easing.type: Easing.OutCubic
    }
    // The skeleton dissolves into the page from the frame the page is
    // mounted, as QBZ retires a placeholder at handover. It runs on the
    // render thread like the reveal: the mount keeps the window's thread
    // busy, and a fade driven from there would hold the bars still behind
    // the arriving page and drop them at once.
    OpacityAnimator {
        id: retire
        target: skeleton
        from: 1
        to: 0
        duration: Motion.handover
    }
    // The reveal only advances while frames are drawn. A page must never
    // stay invisible because they stopped.
    Timer {
        id: revealGuard
        interval: Motion.revealGuard
        onTriggered: transition.settleReveal()
    }
    Connections {
        target: Motion
        function onReducedChanged() { if (Motion.reduced) transition.settleReveal() }
    }

    Loader {
        id: first
        property string pageRoute: ""
        property int epoch: -1
        readonly property alias scroll: firstScroll
        anchors.fill: parent
        asynchronous: true
        visible: transition.phase === "content" && transition.frontA
        enabled: visible
        onLoaded: Qt.callLater(transition.present, first, epoch)
        onStatusChanged: if (status === Loader.Error && epoch === transition.generation) transition.phase = "error"
        ScrollMemory {
            id: firstScroll
            viewport: first.item && first.item.contentY !== undefined ? first.item : first.item?.scrollView || null
            key: transition.scrollKeyOf(first)
        }
    }
    Loader {
        id: second
        property string pageRoute: ""
        property int epoch: -1
        readonly property alias scroll: secondScroll
        anchors.fill: parent
        asynchronous: true
        visible: transition.phase === "content" && !transition.frontA
        enabled: visible
        onLoaded: Qt.callLater(transition.present, second, epoch)
        onStatusChanged: if (status === Loader.Error && epoch === transition.generation) transition.phase = "error"
        ScrollMemory {
            id: secondScroll
            viewport: second.item && second.item.contentY !== undefined ? second.item : second.item?.scrollView || null
            key: transition.scrollKeyOf(second)
        }
    }
    FrameFence {
        id: frameFence
        onPresented: token => {
            const epoch = Math.floor(token / 2)
            if (epoch !== transition.generation) return
            if (token % 2) {
                if (transition.phase === "content") transition.awaitingContentFrame = false
                return
            }
            if (!transition.awaitingFrame || transition.phase !== "acknowledging") return
            transition.awaitingFrame = false
            transition.acknowledgedFrame(transition.acknowledgedRoute)
            console.info(timing, "navigation: " + transition.acknowledgedRoute.split("\u001f")[1]
                + " acknowledged in " + (Date.now() - transition.requestedAt) + "ms")
            Qt.callLater(transition.begin, epoch)
        }
    }
    Text {
        anchors.centerIn: parent
        visible: transition.phase === "error"
        text: I18n.tr("Try again", I18n.rev)
        color: Theme.color.textPrimary
        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: transition.request() }
    }
    // The skeleton holds the destination's place from the acknowledgement
    // until its page is mounted. Nothing opaque goes under it: the page fades
    // in over the plane's own background, as QBZ's does. An opaque shell
    // there covered the dynamic background for the whole navigation and left
    // in one step when the reveal ended.
    Column {
        id: skeleton
        objectName: "acknowledgementSkeleton"
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: Metrics.contentX
        spacing: Metrics.gap16
        visible: transition.phase !== "error" && (transition.phase !== "content" || retire.running)
        z: -1
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
