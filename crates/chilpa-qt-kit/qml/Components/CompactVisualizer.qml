// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
import QtQuick
import QtQuick.Window
import app.chilpa.kit
import "../Theme"

// I share the producer and pulse with other surfaces. Hidden instances keep
// no renderer or pulse subscription; only the selected stream crosses QML.
Item {
    id: band
    required property var feed
    property bool active: false
    property int mode: 0
    property color topColor: Theme.color.accent
    property color bottomColor: Theme.color.accent
    property Component scopeRenderer
    readonly property bool gpuAvailable: GraphicsInfo.api !== GraphicsInfo.Software
        && GraphicsInfo.api !== GraphicsInfo.Null
    readonly property bool windowShowing: !Window.window
        || (Window.window.visibility !== Window.Hidden && Window.window.visibility !== Window.Minimized)
    readonly property bool capturing: active && visible && windowShowing
    readonly property int effectiveMode: (mode === 3 || mode === 4) && (!gpuAvailable || !scopeRenderer) ? 0 : mode
    signal cycleRequested(int nextMode)
    implicitHeight: Metrics.visualizerBandTall

    function sync() { if (feed) feed.setCompact(capturing, effectiveMode) }
    function nextMode() {
        const next = (mode + 1) % 7
        return next === 3 && (!gpuAvailable || !scopeRenderer) ? 5 : next
    }
    onCapturingChanged: sync()
    onEffectiveModeChanged: sync()
    Component.onCompleted: sync()
    Component.onDestruction: if (feed) feed.setCompact(false, effectiveMode)

    Loader {
        id: columns
        anchors.fill: parent
        active: band.capturing && band.effectiveMode !== 3 && band.effectiveMode !== 4
        sourceComponent: Item {
            id: plot
            property var targetValues: []
            property var values: []
            property var peaks: []
            property var holds: []
            property bool settling: false
            readonly property var buffers: ({ a: [], b: [], peaksA: [], peaksB: [], flip: false })
            readonly property int count: band.effectiveMode === 1 ? 48 : band.effectiveMode === 2 ? 5 : 28
            readonly property bool shaderOk: band.gpuAvailable && effect.status !== ShaderEffect.Error
            readonly property real slot: width / count
            readonly property var activeBins: [0, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]

            function receive(source) {
                if (!band.capturing) return
                if (band.effectiveMode !== 1 && Array.isArray(source)) {
                    targetValues = source
                    settling = true
                    return
                }
                const size = band.effectiveMode === 1 ? 48 : band.effectiveMode === 2 ? 5 : 16
                const next = new Array(size)
                for (let i = 0; i < size; ++i)
                    next[i] = Math.max(0, Math.min(1, band.effectiveMode === 1 ? Math.abs(source[i * 5] || 0) : source[i] || 0))
                targetValues = next
                settling = true
            }
            function receiveCurrent() {
                receive(band.effectiveMode === 1 ? band.feed.waveform : band.effectiveMode === 2 ? band.feed.energy : band.feed.bars)
            }
            function bin(index) { return count === 28 ? activeBins[index < 14 ? index : 27 - index] : index }
            function at(index) { return values[bin(index)] || 0 }
            function peak(index) { return peaks[bin(index)] || 0 }
            function tick() {
                const next = buffers.flip ? buffers.a : buffers.b
                next.length = targetValues.length
                const withPeaks = band.effectiveMode === 5
                const tops = withPeaks ? (buffers.flip ? buffers.peaksA : buffers.peaksB) : null
                if (withPeaks) tops.length = targetValues.length
                let moved = false
                for (let i = 0; i < targetValues.length; ++i) {
                    const old = values[i] || 0
                    const delta = targetValues[i] - old
                    const value = Math.abs(delta) > 0.0005 ? old + delta * 0.45 : targetValues[i]
                    next[i] = value
                    moved = moved || Math.abs(delta) > 0.0005
                    if (!withPeaks) continue
                    let peak = peaks[i] || 0
                    let hold = Math.max(0, (holds[i] || 0) - Pulse.period)
                    if (value > peak) { peak = value; hold = 160 }
                    else if (hold === 0) peak = Math.max(value, peak - Pulse.period / 650)
                    tops[i] = peak
                    holds[i] = hold
                    if (band.effectiveMode === 5 && peak > value + 0.0005) moved = true
                }
                settling = moved
                if (!moved) return
                buffers.flip = !buffers.flip
                values = next
                if (withPeaks) peaks = tops
            }
            Component.onCompleted: receiveCurrent()
            Connections {
                target: band
                function onEffectiveModeChanged() {
                    plot.values = []; plot.peaks = []; plot.holds = []
                    plot.receiveCurrent()
                }
            }
            Connections {
                target: band.feed
                function onBarsChanged() { if (band.effectiveMode === 0 || band.effectiveMode >= 5) plot.receive(band.feed.bars) }
                function onWaveformChanged() { if (band.effectiveMode === 1) plot.receive(band.feed.waveform) }
                function onEnergyChanged() { if (band.effectiveMode === 2) plot.receive(band.feed.energy) }
            }
            PulseClock {
                running: band.capturing && plot.settling
                onTriggered: plot.tick()
            }
            CompactSpectrumShader {
                id: effect
                anchors.fill: parent
                visible: plot.shaderOk
                mode: band.effectiveMode
                values: plot.values
                peakValues: plot.peaks
                topColor: band.topColor
                bottomColor: band.bottomColor
            }
            Gradient {
                id: tint
                GradientStop { position: 0; color: band.topColor }
                GradientStop { position: 1; color: band.bottomColor }
            }
            Repeater {
                model: plot.shaderOk ? 0 : plot.count
                delegate: Item {
                    id: column
                    required property int index
                    x: index * plot.slot
                    width: plot.slot
                    height: plot.height
                    Rectangle {
                        x: band.effectiveMode === 2 ? 3 : 1
                        width: Math.max(0, column.width - x * 2) // derived: paired gaps
                        height: Math.max(Metrics.hairline * 2, plot.at(column.index) * column.height) // derived: two-pixel minimum
                        y: band.effectiveMode === 1 ? (column.height - height) / 2 : column.height - height
                        visible: band.effectiveMode < 5
                        radius: 1 // intrinsic: compact column corner
                        gradient: tint
                    }
                    Rectangle {
                        visible: band.effectiveMode === 5 && plot.peak(column.index) > 0.001
                        x: 1
                        y: Math.max(0, column.height * (1 - plot.peak(column.index)) - height)
                        width: Math.max(0, column.width - 2) // derived: paired pixel gaps
                        height: 1 // intrinsic: a peak is a single pixel mark
                        color: band.topColor
                    }
                    Repeater {
                        model: band.effectiveMode >= 5 ? Math.max(1, Math.floor(column.height / plot.slot)) : 0
                        delegate: Rectangle {
                            required property int index
                            x: 1
                            y: column.height - (index + 1) * plot.slot
                            width: Math.max(0, plot.slot - 2) // derived: paired pixel gaps
                            height: width
                            visible: (index + 0.5) * plot.slot <= plot.at(column.index) * column.height
                            color: band.topColor
                        }
                    }
                }
            }
        }
    }
    Loader {
        id: scope
        objectName: "compactVisualizerScope"
        anchors.fill: parent
        active: band.capturing && (band.effectiveMode === 3 || band.effectiveMode === 4)
        sourceComponent: band.scopeRenderer
    }
    Binding { target: scope.item; property: "mode"; value: band.effectiveMode - 3; when: scope.item !== null }
    Binding { target: scope.item; property: "points"; value: band.effectiveMode === 3 ? band.feed.goniometer : band.feed.oscilloscope; when: scope.item !== null }
    Binding { target: scope.item; property: "traceColor"; value: band.topColor; when: scope.item !== null }
    MouseArea {
        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        onClicked: band.cycleRequested(band.nextMode())
    }
}
