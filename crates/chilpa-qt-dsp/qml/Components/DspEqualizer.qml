import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: editor
    property var draft: ({})
    property var curve: []
    signal edited(var value)
    readonly property var shapes: ["peak", "low_shelf", "high_shelf", "high_pass", "low_pass", "notch"]
    spacing: Metrics.gap8

    function changeBand(index, key, value) {
        const next = JSON.parse(JSON.stringify(draft))
        next.filters[index][key] = value
        edited(next)
    }
    Canvas {
        id: plot
        width: parent.width
        height: Metrics.dspCurveHeight
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
        Connections { target: editor; function onCurveChanged() { plot.requestPaint() } }
        onPaint: {
            const ctx = getContext("2d")
            ctx.reset()
            ctx.fillStyle = Theme.color.fillField
            ctx.fillRect(0, 0, width, height)
            ctx.strokeStyle = Theme.color.strokeHairline
            ctx.lineWidth = 1
            for (const db of [-12, -6, 0, 6, 12]) {
                const y = height * (18 - db) / 36
                ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(width, y); ctx.stroke()
            }
            if (editor.curve.length < 2) return
            ctx.strokeStyle = Theme.color.accent
            ctx.lineWidth = Metrics.gap2
            ctx.beginPath()
            editor.curve.forEach((db, index) => {
                const x = index * width / (editor.curve.length - 1)
                const y = height * (18 - Math.max(-18, Math.min(18, db))) / 36
                index === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y)
            })
            ctx.stroke()
        }
    }
    Text {
        width: parent.width
        text: I18n.tr("EQ response · 20 Hz–20 kHz · ±18 dB · before preamp", I18n.rev)
        font.pointSize: Type.small
        color: Theme.color.textTertiary
    }
    Row {
        spacing: Metrics.gap8
        Repeater {
            model: [I18n.tr("Filter", I18n.rev), "Hz", "dB", "Q"]
            Text {
                required property int index
                required property string modelData
                width: index === 0 ? Metrics.dspBandShapeWidth : Metrics.dspBandValueWidth
                text: modelData
                font.pointSize: Type.small
                color: Theme.color.textTertiary
            }
        }
    }
    Repeater {
        model: editor.draft.filters || []
        Row {
            id: band
            required property int index
            required property var modelData
            spacing: Metrics.gap8
            SelectField {
                width: Metrics.dspBandShapeWidth
                options: [I18n.tr("Peak", I18n.rev), I18n.tr("Low shelf", I18n.rev), I18n.tr("High shelf", I18n.rev), I18n.tr("High pass", I18n.rev), I18n.tr("Low pass", I18n.rev), I18n.tr("Notch", I18n.rev)]
                values: editor.shapes
                value: band.modelData.shape
                onPicked: value => editor.changeBand(band.index, "shape", value)
            }
            DspNumber { value: band.modelData.frequency; minimum: 10; maximum: 24000; label: "Hz"; onCommitted: value => editor.changeBand(band.index, "frequency", value) }
            DspNumber { value: band.modelData.gain_db; minimum: -24; maximum: 18; label: "dB"; enabled: ["peak", "low_shelf", "high_shelf"].includes(band.modelData.shape); onCommitted: value => editor.changeBand(band.index, "gain_db", value) }
            DspNumber { value: band.modelData.q; minimum: 0.1; maximum: 12; label: "Q"; onCommitted: value => editor.changeBand(band.index, "q", value) }
            IconButton {
                glyph: Icons.x
                tip: I18n.tr("Remove filter", I18n.rev)
                onClicked: {
                    const next = JSON.parse(JSON.stringify(editor.draft))
                    next.filters.splice(band.index, 1)
                    editor.edited(next)
                }
            }
        }
    }
    PillButton {
        text: I18n.tr("Add filter", I18n.rev)
        enabled: (editor.draft.filters || []).length < 10
        onClicked: {
            const next = JSON.parse(JSON.stringify(editor.draft))
            if (!next.filters) next.filters = []
            next.filters.push({ shape: "peak", frequency: 1000, gain_db: 0, q: 0.707 })
            editor.edited(next)
        }
    }
}
