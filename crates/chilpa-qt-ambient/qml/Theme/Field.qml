pragma Singleton
import QtQuick

// The numbers of the dynamic background, as QBZ's Qt shell draws it. The
// window's theme does not name them: they belong to these fields alone.
QtObject {
    // I seed the field so two launches open on different poses.
    readonly property real seedSpan: 40
    // What the shader paints where no lobe dominates, times the master
    // brightness: the base the Canvas arm starts from. Never black.
    readonly property real floor: 0.42
    readonly property real master: 0.92
    // The lobes' radius as a share of the field's HEIGHT (the shader works in
    // a space where y spans exactly 1), and each lobe's factor and alpha.
    readonly property real lobe: 0.34
    readonly property var lobes: [
        { factor: 1.0, alpha: 0.50 },
        { factor: 0.95, alpha: 0.46 },
        { factor: 0.88, alpha: 0.44 },
        { factor: 0.82, alpha: 0.42 }
    ]
    // The vertical shade is a quadratic multiply, nearly nothing across the
    // middle and 16% at the edges: five stops sample that curve.
    readonly property var shade: [0.16, 0.04, 0.0, 0.04, 0.16]

    // The atmosphere: milliseconds per pulse for its drift, the four layers
    // of the 128 px bitmap (how far past the viewport each grows, where it
    // starts, how far it swings and how opaque it is), and the plain cover
    // shown while the bitmap is not ready.
    readonly property int atmosphereTick: 33
    readonly property var layers: [
        { grow: Qt.size(520, 420), origin: Qt.point(-260, -210), swing: Qt.point(132, 58), opacity: 0.95 },
        { grow: Qt.size(620, 500), origin: Qt.point(-310, -250), swing: Qt.point(156, 74), opacity: 0.48 },
        { grow: Qt.size(700, 540), origin: Qt.point(-350, -270), swing: Qt.point(118, 92), opacity: 0.24 },
        { grow: Qt.size(560, 620), origin: Qt.point(-280, -310), swing: Qt.point(102, 128), opacity: 0.18 }
    ]
    readonly property int staticGrow: 220
    readonly property real staticOpacity: 0.35
}
