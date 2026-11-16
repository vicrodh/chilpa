import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: chart
    property var values: []
    property var filters: []
    property bool interactive: false
    property real maximumHeight: Metrics.dspCurveHeight
    property int selected: -1
    signal picked(int index)
    signal previewed(int index, real frequency, real gain)
    signal committed(int index, real frequency, real gain)
    signal qCommitted(int index, real q)
    readonly property real range: Math.max(18, Math.ceil(Math.max(...values.map(value => Math.abs(value)), 0) / 6) * 6)
    spacing: Metrics.gap6
    function xFor(hz) { return Math.max(0, Math.min(1, Math.log(hz / 20) / Math.log(1000))) * plot.width }
    function yFor(db) { return plot.height * (range - Math.max(-range, Math.min(range, db))) / (range * 2) }
    function at(x, y) {
        let result = -1, distance = Metrics.dspCurveHitRadius * Metrics.dspCurveHitRadius
        filters.forEach((filter, index) => {
            if (filter.enabled === false) return
            const squared = Math.pow(xFor(filter.frequency) - x, 2) + Math.pow(yFor(filter.gain_db) - y, 2)
            if (squared <= distance) { result = index; distance = squared }
        })
        return result
    }
    function coordinates(x, y) {
        return { frequency: Math.max(20, Math.min(20000, 20 * Math.pow(1000, x / plot.width))),
                 gain: Math.max(-24, Math.min(18, range - y * range * 2 / plot.height)) }
    }
    Canvas {
        id: plot
        objectName: "dspResponseCurve"
        width: parent.width
        height: Math.min(chart.maximumHeight, width * Metrics.dspCurveAspectRatio)
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
        Connections {
            target: chart
            function onValuesChanged() { plot.requestPaint() }
            function onFiltersChanged() { plot.requestPaint() }
            function onSelectedChanged() { plot.requestPaint() }
        }
        Connections { target: Theme; function onRevChanged() { plot.requestPaint() } }
        onPaint: {
            const ctx = getContext("2d")
            ctx.reset()
            ctx.fillStyle = Theme.color.surfacePanel
            ctx.fillRect(0, 0, width, height)
            ctx.strokeStyle = Theme.color.strokeHairline
            ctx.lineWidth = 1
            for (let index = 0; index <= 6; index++) {
                const y = height * index / 6
                ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(width, y); ctx.stroke()
            }
            for (const hz of [100, 1000, 10000]) {
                const x = chart.xFor(hz)
                ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, height); ctx.stroke()
            }
            ctx.strokeStyle = Theme.color.accent
            ctx.lineWidth = Metrics.gap2
            ctx.beginPath()
            chart.values.forEach((db, index) => {
                const x = index * width / Math.max(1, chart.values.length - 1)
                index === 0 ? ctx.moveTo(x, chart.yFor(db)) : ctx.lineTo(x, chart.yFor(db))
            })
            ctx.stroke()
            if (!chart.interactive) return
            chart.filters.forEach((filter, index) => {
                if (filter.enabled === false) return
                ctx.beginPath(); ctx.arc(chart.xFor(filter.frequency), chart.yFor(filter.gain_db), index === chart.selected ? Metrics.gap6 : Metrics.gap4, 0, Math.PI * 2)
                ctx.fillStyle = filter.locked ? Theme.color.textTertiary : Theme.color.textPrimary
                ctx.fill()
            })
        }
        MouseArea {
            anchors.fill: parent
            property int dragIndex: -1
            enabled: chart.interactive
            hoverEnabled: true
            cursorShape: chart.selected >= 0 ? Qt.SizeAllCursor : Qt.ArrowCursor
            preventStealing: pressed && chart.selected >= 0
            onPressed: mouse => {
                dragIndex = chart.at(mouse.x, mouse.y)
                chart.picked(dragIndex)
                if (dragIndex < 0 || chart.filters[dragIndex].locked) { dragIndex = -1; mouse.accepted = false }
            }
            onPositionChanged: mouse => {
                if (!pressed || dragIndex < 0) return
                const next = chart.coordinates(mouse.x, mouse.y)
                chart.previewed(dragIndex, next.frequency, next.gain)
            }
            onReleased: mouse => {
                if (dragIndex < 0) return
                const next = chart.coordinates(mouse.x, mouse.y)
                chart.committed(dragIndex, next.frequency, next.gain)
            }
            onWheel: wheel => {
                const index = chart.at(wheel.x, wheel.y)
                if (index < 0 || chart.filters[index].locked) { wheel.accepted = false; return }
                chart.picked(index)
                chart.qCommitted(index, Math.max(0.1, Math.min(12, chart.filters[index].q * Math.pow(1.05, wheel.angleDelta.y / 120))))
            }
        }
    }
    Item {
        width: parent.width
        height: Metrics.gap16
        Repeater {
            model: [100, 1000, 10000]
            Text {
                required property real modelData
                x: chart.xFor(modelData) - width / 2
                text: modelData < 1000 ? String(modelData) : modelData / 1000 + "k"
                color: Theme.color.textTertiary
                font.pointSize: Type.small
            }
        }
    }
    Text {
        width: parent.width
        text: I18n.tr("EQ and tone · upper channel response · 20 Hz–20 kHz · before preamp", I18n.rev) + " · ±" + chart.range + " dB"
        wrapMode: Text.WordWrap
        color: Theme.color.textTertiary
        font.pointSize: Type.small
    }
    Text {
        visible: chart.interactive
        width: parent.width
        text: I18n.tr("Points show individual filter gains. Drag to adjust frequency and gain; scroll over a point to adjust Q. Locked filters stay fixed.", I18n.rev)
        wrapMode: Text.WordWrap
        color: Theme.color.textTertiary
        font.pointSize: Type.small
    }
}
