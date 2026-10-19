import QtQuick
import app.chilpa.themes
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// QBZ's inline HSV colour picker (`QbzColorPicker.qml`): a saturation and
// value square, a hue strip, a preview and an editable HEX field. One is
// mounted at a time, under the open swatch, never as a popup.
//
// It owns its hue, saturation and value, seeded from `value` when it is
// mounted and when the value changes from outside: a colour worked out of
// RGB loses its hue at s == 0 or v == 0. `dragging` keeps the round trip of
// its own colour from moving the crosshair mid-drag, and the 8-bit HEX
// comparison keeps it from decomposing its own colour again. No timer and
// no animation: it repaints when it is moved and costs nothing at rest.
//
// Every colour it paints is worked out in Rust (`ThemeEditor.hsv`); hue is
// 0 to 1. `picked` and the HEX field speak `#rrggbb`.
Rectangle {
    id: root

    property color value
    // A colour, live while dragging, as `#rrggbb`.
    signal picked(string hex)
    // A drag ended: the owner writes the colour now.
    signal released()
    // A HEX typed in and committed.
    signal hexCommitted(string hex)

    property real hue: 0.6028
    property real sat: 0.74
    property real val: 0.96
    property bool dragging: false

    readonly property color current: ThemeEditor.hsv(root.hue, root.sat, root.val, 1)
    readonly property string currentHex: _hex(root.current)
    readonly property color white: ThemeEditor.hsv(0, 0, 1, 1)
    readonly property color edge: ThemeEditor.hsv(0, 0, 0, 0.5) // derived: QBZ's half-black outline

    function _hex(c) {
        function b(x) {
            const v = Math.max(0, Math.min(255, Math.round(x * 255))) // derived: 8-bit channels
            return (v < 16 ? "0" : "") + v.toString(16) // derived: two hex digits
        }
        return "#" + b(c.r) + b(c.g) + b(c.b)
    }

    // Seeds the owned HSV from `value`; QBZ's `decompose`, hue 0 to 1.
    function decompose() {
        const r = root.value.r, g = root.value.g, b = root.value.b
        const mx = Math.max(r, Math.max(g, b))
        const mn = Math.min(r, Math.min(g, b))
        const d = mx - mn
        root.val = mx
        root.sat = mx <= 0 ? 0 : d / mx
        if (d <= 0)
            root.hue = 0
        else if (mx === r)
            root.hue = ((((g - b) / d) % 6) + 6) % 6 / 6 // derived: the six hue sectors
        else if (mx === g)
            root.hue = (((b - r) / d) + 2) / 6 // derived: the six hue sectors
        else
            root.hue = (((r - g) / d) + 4) / 6 // derived: the six hue sectors
    }

    Component.onCompleted: root.decompose()
    onValueChanged: {
        if (!root.dragging && root._hex(root.value) !== root.currentHex)
            root.decompose()
    }

    color: Theme.color.surfacePanel
    radius: Metrics.radiusCard
    border.width: 1
    border.color: Theme.color.strokeHairline
    implicitHeight: col.implicitHeight + Metrics.gap12 * 2 // derived: the margin above and below
    height: implicitHeight

    Column {
        id: col
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: Metrics.gap12
        spacing: Metrics.gap10

        // Saturation (left to right) and value (top to bottom).
        Rectangle {
            id: sv
            objectName: "pickerShade"
            width: parent.width
            height: Editor.shadeField
            radius: Metrics.radiusCard
            clip: true
            color: ThemeEditor.hsv(root.hue, 1, 1, 1)
            Rectangle {
                anchors.fill: parent
                radius: sv.radius
                gradient: Gradient {
                    orientation: Gradient.Horizontal
                    GradientStop { position: 0.0; color: root.white }
                    GradientStop { position: 1.0; color: ThemeEditor.hsv(0, 0, 1, 0) }
                }
            }
            Rectangle {
                anchors.fill: parent
                radius: sv.radius
                gradient: Gradient {
                    GradientStop { position: 0.0; color: ThemeEditor.hsv(0, 0, 0, 0) }
                    GradientStop { position: 1.0; color: ThemeEditor.hsv(0, 0, 0, 1) }
                }
            }
            Rectangle {
                width: Editor.crosshair
                height: Editor.crosshair
                radius: width / 2 // derived: a circle
                x: Math.max(0, Math.min(sv.width - width, root.sat * sv.width))
                y: Math.max(0, Math.min(sv.height - height, (1 - root.val) * sv.height))
                color: "transparent"
                border.width: Metrics.focusRing
                border.color: root.white
                Rectangle {
                    x: Metrics.focusRing
                    y: Metrics.focusRing
                    width: parent.width - Metrics.focusRing * 2 // derived: inside the outer ring
                    height: parent.height - Metrics.focusRing * 2 // derived: inside the outer ring
                    radius: width / 2 // derived: a circle
                    color: "transparent"
                    border.width: 1
                    border.color: root.edge
                }
            }
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.CrossCursor
                onPressed: (m) => { root.dragging = true; root._pickSv(m.x, m.y) }
                onPositionChanged: (m) => { if (root.dragging) root._pickSv(m.x, m.y) }
                onReleased: { root.dragging = false; root.released() }
                onCanceled: { root.dragging = false; root.released() }
            }
        }

        Rectangle {
            id: hueStrip
            objectName: "pickerHue"
            width: parent.width
            height: Editor.hueBand
            radius: height / 2 // derived: round ends
            clip: true
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0.0000; color: ThemeEditor.hsv(0 / 6, 1, 1, 1) }
                GradientStop { position: 0.1666; color: ThemeEditor.hsv(1 / 6, 1, 1, 1) }
                GradientStop { position: 0.3333; color: ThemeEditor.hsv(2 / 6, 1, 1, 1) }
                GradientStop { position: 0.5000; color: ThemeEditor.hsv(3 / 6, 1, 1, 1) }
                GradientStop { position: 0.6666; color: ThemeEditor.hsv(4 / 6, 1, 1, 1) }
                GradientStop { position: 0.8333; color: ThemeEditor.hsv(5 / 6, 1, 1, 1) }
                GradientStop { position: 1.0000; color: ThemeEditor.hsv(6 / 6, 1, 1, 1) }
            }
            Rectangle {
                width: Editor.hueMarker
                height: parent.height + Metrics.gap4 // derived: past the strip on both edges
                y: -Metrics.gap2
                x: Math.max(0, Math.min(hueStrip.width - width, root.hue * hueStrip.width))
                radius: width / 2 // derived: round ends
                color: root.white
                border.width: 1
                border.color: root.edge
            }
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onPressed: (m) => { root.dragging = true; root._pickHue(m.x) }
                onPositionChanged: (m) => { if (root.dragging) root._pickHue(m.x) }
                onReleased: { root.dragging = false; root.released() }
                onCanceled: { root.dragging = false; root.released() }
            }
        }

        Row {
            width: parent.width
            spacing: Metrics.gap10
            Rectangle {
                width: Editor.previewLong
                height: Editor.previewTall
                radius: Metrics.radiusCard
                border.width: 1
                border.color: Theme.color.strokeHairline
                color: root.current
            }
            TextField {
                id: hex
                objectName: "pickerHex"
                width: Math.max(Editor.hexLeast, parent.width - Editor.previewLong - Metrics.gap10)
                anchors.verticalCenter: parent.verticalCenter
                text: root.currentHex
                placeholder: "#rrggbb"
                onAccepted: root.hexCommitted(hex.text)
                // A shorter string is an edit in progress.
                onTextEdited: if (hex.text.length === 7) root.hexCommitted(hex.text) // derived: "#" and six digits
            }
        }
    }

    function _pickSv(mx, my) {
        root.sat = Math.max(0, Math.min(1, mx / sv.width))
        root.val = Math.max(0, Math.min(1, 1 - my / sv.height))
        root.picked(root._hex(ThemeEditor.hsv(root.hue, root.sat, root.val, 1)))
    }
    function _pickHue(mx) {
        root.hue = Math.max(0, Math.min(1, mx / hueStrip.width))
        root.picked(root._hex(ThemeEditor.hsv(root.hue, root.sat, root.val, 1)))
    }
}
