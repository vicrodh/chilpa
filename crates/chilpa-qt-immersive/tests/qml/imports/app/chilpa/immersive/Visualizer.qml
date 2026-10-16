pragma Singleton
import QtQuick

// The visualizer's streams as the tests see them: flat arrays a test
// writes, where the bridge publishes lists.
QtObject {
    property var bars: new Array(16).fill(0)
    property var energy: new Array(5).fill(0)
    property var waveform: new Array(512).fill(0)
    property var goniometer: new Array(512).fill(0)
    property var oscilloscope: new Array(512).fill(0)
    property real stereoCorrelation: 0
    property string packJson: ""
    property var linebedHeights: null
    property var ribbonFrame: null
    property int sampleRateHz: 44100
    property int bitDepth: 16
    property bool presented: false
    function setPresented(value) { presented = value }
    function setCursor(track, position, duration) {}
    function ribbonSnapshot() { return ribbonFrame }
}
