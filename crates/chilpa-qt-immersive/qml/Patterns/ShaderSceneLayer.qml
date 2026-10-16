// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
import QtQuick
import QtQuick.Window
import app.chilpa
import app.chilpa.kit
import app.chilpa.graphics
import app.chilpa.ambient
import app.chilpa.immersive
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Components"
import "../Theme"

Item {
    id: root

    property int scene: Immersive.scene
    property bool opened: Immersive.open

    property var feed: Visualizer
    property bool spectralHeaderVisible: true
    property bool running: Player.playing && !Player.paused && !Motion.reduced
    property bool shaderFailed: false
    readonly property bool available: Immersive.accelerated
        && !root.shaderFailed

    function onShaderError(name, logText) {
        console.warn("ShaderSceneLayer: " + name + " shader failed:", logText)
        shaderFailed = true
        Immersive.setAvailable(false)
    }

    readonly property bool windowShowing: root.Window.window
        ? (root.Window.window.visibility !== Window.Minimized
           && root.Window.window.visibility !== Window.Hidden)
        : true
    readonly property bool active: root.visible && opened
        && root.scene > 0 && root.available && root.windowShowing

    readonly property bool moving: active && running
    property bool holding: false
    function syncPulse() {
        if (holding === moving)
            return
        holding = moving
        holding ? Pulse.hold() : Pulse.release()
    }
    onMovingChanged: {
        syncPulse()
        if (active && scene === 4 && !running) {
            pendingRibbon = null
            if (ribbonLoader && ribbonLoader.item)
                ribbonLoader.item.pulseTick(pausedRibbon(), energyHi)
        }
    }
    onRunningChanged: if (running) frozenRibbon = null
    Component.onCompleted: {
        syncPulse()
        try { pendingPack = JSON.parse(feed.packJson) } catch (e) {}
        pendingBars = feed.bars
        pendingHeights = feed.linebedHeights
        pendingRibbon = feed.ribbonFrame
    }
    Component.onDestruction: if (holding) Pulse.release()

    readonly property real pixelRatio: root.Screen.devicePixelRatio
    readonly property real resW: Math.min(Stage.sceneMaxWide, Math.max(1, Math.round(width * pixelRatio)))
    readonly property real resH: Math.min(Stage.sceneMaxTall, Math.max(1, Math.round(height * pixelRatio)))

    readonly property int tickMs: Pulse.period
    property real timeS: Math.random() * 40.0

    property real phase: 0
    property real beat: 0
    property real level: 0
    property real levelSmooth: 0
    property real transientAmp: 0
    property real beatAbove: 0
    property vector4d energyLo: Qt.vector4d(0, 0, 0, 0)
    property vector4d energyHi: Qt.vector4d(0, 0, 0, 0)
    property vector4d bandsLo: Qt.vector4d(0, 0, 0, 0)
    property vector4d bandsHi: Qt.vector4d(0, 0, 0, 0)

    property var pendingPack: null
    property var pendingHeights: null
    property var pendingRibbon: null
    // Keep one canonical paused image even if the drain delivers a late row.
    property var frozenRibbon: null
    function pausedRibbon() {
        if (frozenRibbon === null)
            frozenRibbon = feed.ribbonSnapshot()
        return frozenRibbon
    }
    property var pendingBars: null

    function rhiPack() {
        return {
            "time": root.timeS,
            "beat": root.beat,
            "level": root.level,
            "levelSmooth": root.levelSmooth,
            "energyLo": root.energyLo,
            "energyHi": root.energyHi,
            "transientAmp": root.transientAmp,
            "bandsLo": root.bandsLo,
            "bandsHi": root.bandsHi,
            "beatAbove": root.beatAbove,
            "primary": Ambient.primary,
            "secondary": Ambient.secondary,
            "accent": Ambient.accent
        }
    }

    Connections {
        target: root.feed
        function onPackJsonChanged() {
            try {
                root.pendingPack = JSON.parse(root.feed.packJson)
            } catch (e) {
            }
        }
        function onLinebedHeightsChanged() {
            root.pendingHeights = root.feed.linebedHeights
        }
        function onRibbonFrameChanged() {
            root.pendingRibbon = root.feed.ribbonFrame
        }
        
    }

    Connections {
        target: root.feed
        function onBarsChanged() {
            root.pendingBars = root.feed.bars
        }
    }

    Connections {
        target: Pulse
        function onMsChanged() {
            if (!root.moving)
                return
            root.timeS += root.tickMs / 1000.0
            if (lineBedLoader.item) {
                lineBedLoader.item.pulseTick(root.pendingHeights)
                root.pendingHeights = null
            }
            if (tunnelFlowLoader.item) {
                tunnelFlowLoader.item.pulseTick(root.timeS, root.pendingBars)
                root.pendingBars = null
            }
            if (plasmaLoader.item)
                plasmaLoader.item.pulseTick(root.rhiPack())
            if (ribbonLoader.item) {
                ribbonLoader.item.pulseTick(root.pendingRibbon, root.energyHi)
                root.pendingRibbon = null
            }
            var p = root.pendingPack
            if (p === null)
                return
            root.pendingPack = null
            root.phase = p.phase
            root.beat = p.beat
            root.level = p.level
            root.levelSmooth = p.levelSmooth
            root.transientAmp = p.transient
            root.beatAbove = p.beatAbove || 0
            root.energyLo = Qt.vector4d(p.energyLo[0], p.energyLo[1], p.energyLo[2], p.energyLo[3])
            root.energyHi = Qt.vector4d(p.energyHi[0], p.energyHi[1], p.energyHi[2], p.energyHi[3])
            root.bandsLo = Qt.vector4d(p.bandsLo[0], p.bandsLo[1], p.bandsLo[2], p.bandsLo[3])
            root.bandsHi = Qt.vector4d(p.bandsHi[0], p.bandsHi[1], p.bandsHi[2], p.bandsHi[3])
        }
    }


    Component {
        id: tunnelScene
        PackScene {
            uniforms: root
            onFailed: (details) => root.onShaderError("tunnel", details)
            fragmentShader: "qrc:/qt/qml/app/chilpa/graphics/assets/shaders/tunnel.frag.qsb"
        }
    }

    Component {
        id: auroraScene
        PackScene {
            uniforms: root
            onFailed: (details) => root.onShaderError("aurora", details)
            fragmentShader: "qrc:/qt/qml/app/chilpa/graphics/assets/shaders/aurora.frag.qsb"
        }
    }

    Component {
        id: ambientScene
        PackScene {
            uniforms: root
            property real dim: 0.0
            onFailed: (details) => root.onShaderError("ambient", details)
            fragmentShader: "qrc:/qt/qml/app/chilpa/ambient/assets/shaders/ambient.frag.qsb"
            vertexShader: "qrc:/qt/qml/app/chilpa/ambient/assets/shaders/ambient.vert.qsb"
        }
    }

    Loader {
        anchors.fill: parent
        active: root.active
        sourceComponent: root.scene === 2 ? tunnelScene
            : root.scene === 3 ? auroraScene
            : root.scene === 7 ? ambientScene
            : null
    }

    readonly property real renderScale: Math.min(1, Stage.sceneMaxWide / Math.max(1, width * pixelRatio), Stage.sceneMaxTall / Math.max(1, height * pixelRatio))
    readonly property int renderWidth: Math.max(1, Math.round(width * pixelRatio * renderScale))
    readonly property int renderHeight: Math.max(1, Math.round(height * pixelRatio * renderScale))

    Loader {
        id: lineBedLoader
        anchors.fill: parent
        active: root.active && root.scene === 5
        sourceComponent: LineBedScene {
            fixedColorBufferWidth: root.renderWidth
            fixedColorBufferHeight: root.renderHeight
        }
        onLoaded: item.pulseTick(root.pendingHeights !== null ? root.pendingHeights : root.feed.linebedHeights)
    }

    Loader {
        id: plasmaLoader
        anchors.fill: parent
        active: root.active && root.scene === 1
        sourceComponent: PlasmaScene {
            fixedColorBufferWidth: root.renderWidth
            fixedColorBufferHeight: root.renderHeight
        }
        onLoaded: item.pulseTick(root.rhiPack())
    }

    Loader {
        id: ribbonLoader
        anchors.fill: parent
        active: root.active && root.scene === 4
        sourceComponent: Item {
            function pulseTick(f, peak) {
                ribbonItem.pulseTick(f, peak)
            }
            RibbonScene {
                fixedColorBufferWidth: root.renderWidth
                fixedColorBufferHeight: root.renderHeight
                id: ribbonItem
                anchors.fill: parent
            }
            SpectralOverlay {
                anchors.fill: parent
                feed: root.feed
                headerVisible: root.spectralHeaderVisible
            }
        }
        onLoaded: {
            item.pulseTick(root.running ? root.feed.ribbonSnapshot() : root.pausedRibbon(), root.energyHi)
            root.pendingRibbon = null
        }
    }

    Loader {
        id: tunnelFlowLoader
        anchors.fill: parent
        active: root.active && root.scene === 8
        sourceComponent: TunnelFlowScene {
            fixedColorBufferWidth: root.renderWidth
            fixedColorBufferHeight: root.renderHeight
        }
        onLoaded: item.pulseTick(root.timeS, root.pendingBars !== null ? root.pendingBars : root.feed.bars)
    }
}
