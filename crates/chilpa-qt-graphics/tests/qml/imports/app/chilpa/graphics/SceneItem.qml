import QtQuick
// UI tests use a passive item; the release smoke exercises the native renderer.
Item {
    property bool mirrorVertically: false
    property int fixedColorBufferWidth: 0
    property int fixedColorBufferHeight: 0
    property real time: 0
    property real phase: 0
    property real beat: 0
    property real level: 0
    property real levelSmooth: 0
    property real transientAmp: 0
    property real bass: 0
    property real mid: 0
    property real high: 0
    property real kick: 0
    property color primary: "white"
    property color secondary: "white"
    property color accent: "white"
    property color palette0: "white"
    property color palette1: "white"
    property color palette2: "white"
    property color palette3: "white"
    property vector4d energyLo
    property vector4d energyHi
    property vector4d bandsLo
    property vector4d bandsHi
    property var heights
    property var frame
    property var points
    property int mode: 0
    property color traceColor: "white"
    property real lineWidth: 1
    property int trailDepth: 1
    property real fillOpacity: 0
    function update() {}
}
