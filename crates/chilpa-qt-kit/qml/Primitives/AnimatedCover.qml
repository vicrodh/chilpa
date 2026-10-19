import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

Item {
    id: cover
    property var colors: []
    property string label: ""
    property real phase: 0

    Timer {
        interval: 50
        repeat: true
        running: cover.visible && cover.width > 0 && cover.colors.length >= 3
        onTriggered: {
            cover.phase += 0.015
            surface.requestPaint()
        }
    }
    Canvas {
        id: surface
        anchors.fill: parent
        onPaint: {
            if (cover.colors.length < 3) return
            const ctx = getContext("2d")
            const w = width, h = height, r = Math.min(Metrics.radiusCard, w / 8)
            ctx.clearRect(0, 0, w, h)
            ctx.save()
            ctx.beginPath()
            ctx.moveTo(r, 0); ctx.lineTo(w - r, 0); ctx.quadraticCurveTo(w, 0, w, r)
            ctx.lineTo(w, h - r); ctx.quadraticCurveTo(w, h, w - r, h)
            ctx.lineTo(r, h); ctx.quadraticCurveTo(0, h, 0, h - r)
            ctx.lineTo(0, r); ctx.quadraticCurveTo(0, 0, r, 0)
            ctx.closePath(); ctx.clip()
            ctx.fillStyle = cover.colors[0]; ctx.fillRect(0, 0, w, h)
            for (let i = 1; i < 3; ++i) {
                const x = w * (0.45 + 0.38 * Math.sin(cover.phase + i * 2.3))
                const y = h * (0.45 + 0.34 * Math.cos(cover.phase * 0.8 + i * 2.1))
                const radius = Math.max(w, h) * 0.9
                const glow = ctx.createRadialGradient(x, y, 0, x, y, radius)
                glow.addColorStop(0, cover.colors[i])
                glow.addColorStop(1, "transparent")
                ctx.fillStyle = glow; ctx.fillRect(0, 0, w, h)
            }
            ctx.restore()
        }
    }
    Text {
        anchors.centerIn: parent
        width: parent.width - Metrics.gap24
        text: cover.visible ? cover.label : ""
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.WordWrap
        font.pointSize: Type.titleSmall
        font.weight: Type.bold
        color: Theme.color.backdropText
        style: Text.Outline
        styleColor: Theme.color.overlayScrim
    }
}
