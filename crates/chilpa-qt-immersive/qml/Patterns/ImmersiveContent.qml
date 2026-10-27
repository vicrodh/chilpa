import QtQuick
import QtQuick.Window
import app.chilpa
import app.chilpa.kit
import app.chilpa.ambient
import app.chilpa.immersive
import app.chilpa.graphics
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/ambient/qml/Components" as Fields
import "../Components"
import "../Theme"

// The immersive view over the window (03-immersive §1.1): a dark stage
// with the cover's atmosphere drifting under one panel in focus or the
// split, the song card at the corner over the panels that have no block
// of their own, the floating player and the header that fade six seconds
// after the pointer rests, and the search results on top. Open, it takes
// the keys: Escape dismisses the search first and leaves second; the
// arrows seek through the window's controls. Leaving gives the window
// back its focus and, if the view put it there, its windowed state.
Item {
    id: view
    // The window under the view, for its full screen and its focus.
    property var window: null
    property var controls: null
    visible: Immersive.open
    clip: true
    focus: Immersive.open
    property bool shaders: true
    readonly property bool windowShowing: window === null || (window.visibility !== Window.Minimized && window.visibility !== Window.Hidden)
    readonly property bool presented: visible && windowShowing
    readonly property bool focusMode: Immersive.viewMode === 0
    readonly property int mode: Immersive.mode
    readonly property int splitPanel: Immersive.splitPanel
    readonly property bool sceneOff: Immersive.scene === 0
    readonly property bool playing: Player.playing && !Player.paused
    // The cover's atmosphere, as the album header and the mini draw it.
    readonly property url atmosphere: Player.art.toString().startsWith("file:") ? "image://atmosphere/" + encodeURIComponent(Player.art.toString()) : ""

    Component.onCompleted: { view.wake(); grab.restart() }
    Timer { id: grab; interval: Stage.focusDelay; onTriggered: view.forceActiveFocus() }

    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        hoverEnabled: true
        cursorShape: view.window !== null && view.window.visibility === Window.FullScreen && !view.chromeVisible ? Qt.BlankCursor : Qt.ArrowCursor
        onPressed: (mouse) => { mouse.accepted = true; view.wake() }
        onReleased: (mouse) => mouse.accepted = true
        onClicked: (mouse) => mouse.accepted = true
        onDoubleClicked: (mouse) => mouse.accepted = true
        onWheel: (wheel) => wheel.accepted = true
    }

    HoverHandler {
        id: hover
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
        onPointChanged: {
            const p = hover.point.position
            if (Math.abs(p.x - view.lastHover.x) > Stage.hoverGuard || Math.abs(p.y - view.lastHover.y) > Stage.hoverGuard) {
                view.lastHover = p
                view.wake()
            }
        }
    }
    Timer {
        id: hideTimer
        interval: Stage.hideAfter
        running: view.visible
        onTriggered: {
            if (!search.visible && !view.pointerInChrome)
                view.chromeVisible = false
            else
                restart()
        }
    }

    LyricsFeed {
        id: lyricsFeed
        live: Immersive.open && view.sceneOff && ((view.focusMode && view.mode === 4) || (!view.focusMode && (view.splitPanel === 0 || view.splitPanel === 5)))
    }

    // Nothing under the view gets a click, a wheel or a hover.
    // The chrome hides six seconds after the pointer last moved, unless it
    // rests on the header, the bar or the card, or the search shows.
    property bool chromeVisible: true
    property point lastHover: Qt.point(-1, -1)
    function overChrome(item) {
        if (!item.visible)
            return false
        const p = item.mapFromItem(view, hover.point.position.x, hover.point.position.y)
        return p.x >= 0 && p.y >= 0 && p.x <= item.width && p.y <= item.height
    }
    readonly property bool pointerInChrome: hover.hovered && (overChrome(header) || overChrome(playerBar) || overChrome(songCard) || overChrome(spectrumCycle))
    function wake() {
        chromeVisible = true
        hideTimer.restart()
    }
    // Everything under the chrome, as one item the bar can blur.
    Item {
        id: stage
        anchors.fill: parent
        Rectangle { anchors.fill: parent; color: Theme.color.immersiveStage }
        ShaderSceneLayer { anchors.fill: parent; spectralHeaderVisible: !header.typing }

        // The cover's atmosphere, drifting while it plays; off under the
        // scopes, the rings and any scene.
        Fields.AtmosphereField {
            anchors.fill: parent
            visible: view.presented && view.sceneOff && !(view.focusMode && (view.mode === 7 || view.mode === 8 || view.mode === 9))
            source: visible ? view.atmosphere : ""
            fallbackSource: visible ? Player.art : ""
            animated: visible && view.playing && Immersive.open
            dim: Stage.atmosphereDim
        }

        Loader { anchors.fill: parent; active: view.presented && view.sceneOff && view.focusMode && view.mode === 0; visible: active; sourceComponent: AlbumReactivePanel { shaders: view.shaders } }
        Loader { anchors.fill: parent; active: view.presented && view.sceneOff && view.focusMode && view.mode === 1; visible: active; sourceComponent: StaticPanel { shaders: view.shaders } }
        Loader { anchors.fill: parent; active: view.presented && view.sceneOff && view.focusMode && view.mode === 2; visible: active; sourceComponent: CoverFlowPanel { shaders: view.shaders } }
        Loader { id: spectrum; anchors.fill: parent; active: view.presented && view.sceneOff && view.focusMode && view.mode === 3; visible: active; sourceComponent: SpectrumPanel { shaders: view.shaders } }
        Loader { anchors.fill: parent; active: view.presented && view.sceneOff && view.focusMode && view.mode === 4; visible: active; sourceComponent: LyricsFocusPanel { feed: lyricsFeed } }
        Loader { anchors.fill: parent; active: view.presented && view.sceneOff && view.focusMode && view.mode === 5; visible: active; sourceComponent: QueueTabsPanel { centered: false } }
        Loader { anchors.fill: parent; active: view.presented && view.sceneOff && view.focusMode && view.mode === 6; visible: active; sourceComponent: WaveBedPanel {} }
        Loader { anchors.fill: parent; active: view.presented && view.sceneOff && view.focusMode && view.mode === 9; visible: active; sourceComponent: ReactiveRingsPanel {} }
        Loader {
            anchors.fill: parent
            active: view.presented && view.sceneOff && view.focusMode && (view.mode === 7 || view.mode === 8) && Immersive.accelerated
            visible: active
            sourceComponent: ScopeStage { scopeMode: view.mode === 7 ? 0 : 1; atmosphere: view.atmosphere }
        }
        Loader {
            anchors.fill: parent
            active: view.presented && view.sceneOff && !view.focusMode && (view.splitPanel === 0 || view.splitPanel === 3)
            visible: active
            sourceComponent: SplitLayout { shaders: view.shaders; feed: lyricsFeed }
        }
        Loader {
            anchors.fill: parent
            active: view.presented && view.sceneOff && !view.focusMode && (view.splitPanel === 4 || view.splitPanel === 5)
            visible: active
            sourceComponent: ReactiveSplitPanel { lyricsMode: view.splitPanel === 5; shaders: view.shaders; feed: lyricsFeed }
        }

        Loader {
            id: songCard
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            anchors.rightMargin: Stage.cardEdge
            anchors.bottomMargin: Stage.cardEdge
            sourceComponent: SongCard { shaders: view.shaders }
            visible: active
            active: view.presented && (!view.sceneOff || (view.focusMode && (view.mode === 4 || view.mode === 6 || view.mode === 7 || view.mode === 8 || view.mode === 9))) && Player.path !== ""
        }

    }
    ImmersivePlayerBar {
        id: playerBar
        objectName: "immersiveBar"
        anchors.horizontalCenter: parent.horizontalCenter
        y: view.height - Stage.barTall - Immersive.barBottom // derived: the bar's height and its margin above the bottom
        viewportWide: view.width
        shaders: view.shaders
        beneath: stage
        opacity: view.chromeVisible ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: Motion.reduced ? 0 : Stage.chromeFade } }
        enabled: view.chromeVisible
        onWakeAsked: view.wake()
        onExitAsked: Immersive.open = false
    }
    LeaveButton {
        id: leave
        objectName: "immersiveLeave"
        x: Stage.edge
        y: playerBar.y + (Stage.barTall - height) / 2 // derived: level with the bar's middle
        opacity: playerBar.opacity
        enabled: view.chromeVisible
        onClicked: Immersive.open = false
    }
    LeaveButton {
        id: spectrumCycle
        objectName: "immersiveSpectrumCycle"
        x: leave.x + leave.width + Metrics.gap16
        y: leave.y
        visible: spectrum.active
        opacity: playerBar.opacity
        enabled: view.chromeVisible
        glyph: Lucide.chartColumn
        iconFamily: Lucide.font
        tip: I18n.trArgs("Change spectrum style ({})", [spectrum.item ? spectrum.item.variantLabel : ""], I18n.rev)
        onClicked: { if (spectrum.item) spectrum.item.cycleVariant(); view.wake() }
    }
    ImmersiveHeader {
        id: header
        objectName: "immersiveHeader"
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.leftMargin: Stage.edge
        anchors.rightMargin: Stage.edge
        anchors.topMargin: Stage.headerTop
        height: Stage.headerTall
        hostWindow: view.window
        resultsOpen: search.visible
        opacity: view.chromeVisible ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: Motion.reduced ? 0 : Stage.chromeFade } }
        enabled: view.chromeVisible
        onStepAsked: (by) => search.step(by)
        onAcceptAsked: search.chooseCurrent()
        onEscapeAsked: view.dismissOrLeave()
        onExitAsked: Immersive.open = false
    }
    SearchResults {
        id: search
        objectName: "immersiveSearchResults"
        anchors.fill: parent
        query: header.query
        shaders: view.shaders
        onCleared: header.query = ""
    }
    Connections {
        target: Player
        function onPathChanged() { if (Immersive.open) Immersive.refreshFavorite() }
        function onPositionChanged() { if (Immersive.open && Immersive.scene === 4) Visualizer.setCursor(Player.path, Player.position, Player.duration) }
        function onDurationChanged() { if (Immersive.open && Immersive.scene === 4) Visualizer.setCursor(Player.path, Player.position, Player.duration) }
    }
    Connections {
        target: Ratings
        function onRevChanged() { if (Immersive.open) Immersive.refreshFavorite() }
    }
    onVisibleChanged: if (visible) Immersive.refreshFavorite()

    function dismissOrLeave() {
        if (search.visible)
            search.dismiss()
        else
            Immersive.open = false
    }
    Keys.onEscapePressed: (event) => { dismissOrLeave(); event.accepted = true }
    Keys.onPressed: (event) => {
        if (event.key === Qt.Key_G && !header.typing) {
            if (Immersive.accelerated) {
                Immersive.cycleScene()
                view.wake()
            }
            event.accepted = true
            return
        }
        if (header.typing)
            return
        if (view.controls) {
            view.controls.pressed(event)
            if (event.accepted)
                view.wake()
        }
    }
}
