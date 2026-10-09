import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// When the listening happens over a day: twenty-four wedges around a
// clock face, midnight at the top, each as long as its hour's plays
// against the busiest one, which is drawn whole. The hours 0, 6, 12 and 18
// are written around it.
Item {
    id: clock

    // Twenty-four counts, from midnight.
    property var hours: []
    property color tint: Theme.color.accent
    readonly property int most: hours.reduce((m, h) => Math.max(m, h), 0)
    readonly property int peak: most > 0 ? hours.indexOf(most) : -1
    readonly property real reach: Math.min(width, height) / 2 - Charts.clockLabel - Metrics.gap4 // derived: room for the hours around it

    onHoursChanged: face.requestPaint()
    onTintChanged: face.requestPaint()

    Canvas {
        id: face
        anchors.fill: parent
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
        onPaint: {
            const ctx = getContext("2d")
            ctx.reset()
            const cx = width / 2, cy = height / 2, inner = Charts.clockHole, outer = Math.max(inner + 1, clock.reach)
            ctx.strokeStyle = Theme.color.strokeHairline
            ctx.lineWidth = 1
            for (const part of [0.5, 1]) {
                ctx.beginPath()
                ctx.arc(cx, cy, inner + (outer - inner) * part, 0, 2 * Math.PI)
                ctx.stroke()
            }
            const slice = 2 * Math.PI / 24
            ctx.fillStyle = clock.tint
            for (let h = 0; h < 24; ++h) {
                const share = clock.most > 0 ? (clock.hours[h] || 0) / clock.most : 0
                if (share <= 0)
                    continue
                // By the square root, so a wedge's area follows its plays.
                const r = inner + (outer - inner) * Math.sqrt(share)
                const from = -Math.PI / 2 + h * slice + slice * 0.08
                const to = from + slice * 0.84
                ctx.globalAlpha = h === clock.peak ? 1 : 0.45 + 0.4 * share
                ctx.beginPath()
                ctx.arc(cx, cy, r, from, to)
                ctx.arc(cx, cy, inner, to, from, true)
                ctx.closePath()
                ctx.fill()
            }
            ctx.globalAlpha = 1
        }
    }
    Repeater {
        model: [0, 6, 12, 18]

        Text {
            required property int modelData
            readonly property real angle: -Math.PI / 2 + modelData * Math.PI / 12
            readonly property real distance: clock.reach + Charts.clockLabel / 2 + Metrics.gap2 // derived: just outside the face
            x: clock.width / 2 + Math.cos(angle) * distance - width / 2 // derived: centered on its hour
            y: clock.height / 2 + Math.sin(angle) * distance - height / 2 // derived: centered on its hour
            text: String(modelData).padStart(2, "0")
            font.pointSize: Type.micro
            font.features: { "tnum": 1 }
            color: Theme.color.textTertiary
        }
    }
}
