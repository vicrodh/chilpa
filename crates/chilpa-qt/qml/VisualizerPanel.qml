// What is actually going out the device.
//
// It is not the decoding and it is not a pretty animation: these are the
// samples that were delivered, copied after packing. If this moves, audio is
// going out; if it stays flat, none is, and that is already a diagnosis.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

Rectangle {
    id: root
    radius: 14
    color: "#ffffff"
    border.color: "#1a411d1f"
    implicitHeight: 110

    // It redraws when new data arrives, not on a timer of its own: two clocks
    // out of step look worse than one.
    property var scope: Chilpa.scope
    property var spectrum: Chilpa.spectrum
    property var pairs: Chilpa.pairs
    onScopeChanged: wave.requestPaint()
    onSpectrumChanged: bars.requestPaint()
    onPairsChanged: gonio.requestPaint()

    /// What is drawn. The bridge decides, and remembers it between sessions.
    readonly property string which: Chilpa.visualizer
    readonly property bool asSpectrum: which === "spectrum"
    readonly property bool asGoniometer: which === "goniometer"

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 12
        spacing: 8

        Canvas {
            id: bars
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.asSpectrum

            onPaint: {
                const ctx = getContext("2d")
                ctx.reset()
                const bands = root.spectrum
                if (!bands || bands.length === 0) return
                const barW = width / bands.length
                for (let i = 0; i < bands.length; i++) {
                    const v = Math.min(1, parseFloat(bands[i]) || 0)
                    const barH = Math.max(1, v * height)
                    // The lows in red, the highs in wine: the gradient says
                    // which part of the spectrum is being looked at.
                    const t = i / bands.length
                    ctx.fillStyle = Qt.rgba(0.84 - 0.6 * t, 0.15 + 0.05 * t, 0.17 + 0.05 * t, 1)
                    ctx.fillRect(i * barW, height - barH, Math.max(1, barW - 2), barH)
                }
            }
        }

        // The goniometer: each sample is a dot, left against right, turned 45
        // degrees. A vertical line is mono; a wide blob is stereo; a
        // horizontal line is out of phase, which cancels in mono.
        Canvas {
            id: gonio
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.asGoniometer

            onPaint: {
                const ctx = getContext("2d")
                ctx.reset()
                const cx = width / 2
                const cy = height / 2
                const r = Math.min(cx, cy) - 2

                ctx.strokeStyle = "#1a411d1f"
                ctx.lineWidth = 1
                ctx.beginPath()
                ctx.moveTo(cx, cy - r); ctx.lineTo(cx, cy + r)
                ctx.moveTo(cx - r, cy); ctx.lineTo(cx + r, cy)
                ctx.stroke()

                const points = root.pairs
                if (!points || points.length === 0) return
                ctx.fillStyle = "#66d7272b"
                for (let n = 0; n < points.length; n++) {
                    const p = points[n].split("\t")
                    const i = parseFloat(p[0]) || 0
                    const d = parseFloat(p[1]) || 0
                    // Turned 45 degrees: mono ends up vertical, which is how a
                    // goniometer is read.
                    const x = cx + (d - i) * r * 0.7071
                    const y = cy - (d + i) * r * 0.7071
                    ctx.fillRect(x, y, 2, 2)
                }
            }
        }

        Canvas {
            id: wave
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: !root.asSpectrum && !root.asGoniometer

            onPaint: {
                const ctx = getContext("2d")
                ctx.reset()
                const w = width
                const h = height
                const mid = h / 2

                ctx.strokeStyle = "#1a411d1f"
                ctx.lineWidth = 1
                ctx.beginPath()
                ctx.moveTo(0, mid)
                ctx.lineTo(w, mid)
                ctx.stroke()

                const points = root.scope
                if (!points || points.length === 0) return

                const barW = w / points.length
                ctx.fillStyle = "#d7272b"
                for (let i = 0; i < points.length; i++) {
                    // Each point is its slice's peak: it is drawn
                    // symmetrically, like a wave.
                    const v = Math.min(1, parseFloat(points[i]) || 0)
                    const barH = Math.max(1, v * mid)
                    ctx.fillRect(i * barW, mid - barH, Math.max(1, barW - 1), barH * 2)
                }
            }
        }

        // The levels per channel, with their number: a meter with no number
        // is no use for comparing anything.
        RowLayout {
            Layout.fillWidth: true
            spacing: 10

            Repeater {
                model: Chilpa.levels
                delegate: RowLayout {
                    spacing: 6
                    readonly property var cols: modelData.split("\t")
                    readonly property real peak: parseFloat(cols[0]) || 0
                    readonly property real rms: parseFloat(cols[1]) || 0

                    Label {
                        text: index === 0 ? "I" : (index === 1 ? "D" : (index + 1))
                        font.pixelSize: 10
                        color: "#7a411d1f"
                    }
                    Rectangle {
                        Layout.preferredWidth: 90
                        Layout.preferredHeight: 6
                        radius: 3
                        color: "#f0eae4"
                        Rectangle {
                            width: parent.width * Math.min(1, rms)
                            height: parent.height
                            radius: parent.radius
                            color: "#2e7d32"
                        }
                        Rectangle {
                            x: Math.min(parent.width - 2, parent.width * Math.min(1, peak))
                            width: 2
                            height: parent.height
                            color: peak > 0.999 ? "#d7272b" : "#411d1f"
                        }
                    }
                    Label {
                        text: peak > 0 ? (20 * Math.log(peak) / Math.LN10).toFixed(1) + " dB" : "—"
                        font.pixelSize: 10
                        color: peak > 0.999 ? "#d7272b" : "#7a411d1f"
                    }
                }
            }
            Item { Layout.fillWidth: true }

            // The correlation between channels: one is mono, zero is wide, and
            // below zero there is something out of phase.
            Label {
                visible: Chilpa.correlation <= 1.0
                text: Chilpa.translate("phase", Chilpa.trRev) + " " + Chilpa.correlation.toFixed(2)
                font.pixelSize: 10
                color: Chilpa.correlation < 0 ? "#d7272b" : "#7a411d1f"
            }

            // Changing the drawing without menus: a click on the name.
            Label {
                text: root.which
                font.pixelSize: 10
                color: "#7a411d1f"
                MouseArea {
                    anchors.fill: parent
                    anchors.margins: -6
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        const order = ["wave", "spectrum", "goniometer"]
                        const i = Math.max(0, order.indexOf(root.which))
                        Chilpa.chooseVisualizer(order[(i + 1) % order.length])
                    }
                }
            }
        }
    }
}
