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
            readonly property var values: animationState.values
            readonly property var peaks: animationState.peaks
            readonly property int count: band.effectiveMode === 1 ? 48 : band.effectiveMode === 2 ? 5 : 28
            readonly property bool shaderOk: band.gpuAvailable && (!effect.item || effect.item.status !== ShaderEffect.Error)
            readonly property real slot: width / count
            readonly property var activeBins: [0, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]
            function bin(index) { return count === 28 ? activeBins[index < 14 ? index : 27 - index] : index }
            function at(index) { return values[bin(index)] || 0 }
            function peak(index) { return peaks[bin(index)] || 0 }
            CompactSpectrumState {
                id: animationState
                active: band.capturing
                mode: band.effectiveMode
                source: !active ? [] : mode === 1 ? band.feed.waveform : mode === 2 ? band.feed.energy : band.feed.bars
            }
            Loader {
                id: effect
                anchors.fill: parent
                active: band.gpuAvailable
                sourceComponent: CompactSpectrumShader {
                    mode: band.effectiveMode
                    values: plot.values
                    peakValues: plot.peaks
                    topColor: band.topColor
                    bottomColor: band.bottomColor
                }
            }
            Loader {
                anchors.fill: parent
                active: !plot.shaderOk
                sourceComponent: CompactSpectrumFallback {
                    modelState: plot; mode: band.effectiveMode
                    topColor: band.topColor; bottomColor: band.bottomColor
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
