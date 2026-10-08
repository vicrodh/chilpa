import QtQuick
import "../Theme"

// The field drawn without shaders: the arm the software scene graph takes,
// as QBZ's Qt shell has it. Plain radial gradients cannot reproduce the
// metaball tail, so this is an approximation by construction, but the same
// one the reference makes at its extremes: a base of the album mix at the
// 0.42 floor, with the lobes added on top in the same orbits. What it drops
// is the warp (the lobes are circles, not amoebas) and the grain.
Canvas {
    id: canvas

    property color primary
    property color secondary
    property color accent
    // The field's clock, in seconds; the host advances it on the pulse.
    property real t: 0

    renderTarget: Canvas.Image
    renderStrategy: Canvas.Immediate

    onPrimaryChanged: requestPaint()
    onSecondaryChanged: requestPaint()
    onAccentChanged: requestPaint()
    onWidthChanged: requestPaint()
    onHeightChanged: requestPaint()
    onVisibleChanged: if (visible) requestPaint()

    // A color as the canvas writes it; colors are never built with Qt.rgba
    // in QML, and these are not tokens but the cover's own.
    function asText(c, a) {
        return "rgba(" + Math.round(c.r * 255) + "," + Math.round(c.g * 255) + "," + Math.round(c.b * 255) + "," + a + ")"
    }

    // One lobe: a radial gradient from `a` at the centre to nothing at the
    // radius, the shader's wide falloff.
    function blob(ctx, cx, cy, r, c, a) {
        const g = ctx.createRadialGradient(cx, cy, 0, cx, cy, r)
        g.addColorStop(0.0, asText(c, a))
        g.addColorStop(0.55, asText(c, a * 0.45))
        g.addColorStop(1.0, asText(c, 0))
        ctx.fillStyle = g
        ctx.fillRect(cx - r, cy - r, 2 * r, 2 * r)
    }

    onPaint: {
        const ctx = getContext("2d")
        ctx.reset()
        const w = width
        const h = height
        if (w <= 0 || h <= 0)
            return
        const p = canvas.primary
        const s = canvas.secondary
        const a = canvas.accent
        const c4 = { r: (p.r + a.r) / 2, g: (p.g + a.g) / 2, b: (p.b + a.b) / 2 }

        // The floor: where the shader has no dominant lobe it still paints
        // the weighted album color at 0.42, then the 0.92 master.
        const f = Field.floor * Field.master
        ctx.fillStyle = asText({ r: (p.r + s.r + a.r + c4.r) / 4 * f, g: (p.g + s.g + a.g + c4.g) / 4 * f, b: (p.b + s.b + a.b + c4.b) / 4 * f }, 1)
        ctx.fillRect(0, 0, w, h)

        // The shader's orbits, as window fractions.
        const t = canvas.t * 0.75
        const centers = [
            [0.32 + 0.30 * Math.sin(t * 0.40), 0.42 + 0.30 * Math.cos(t * 0.33)],
            [0.66 + 0.30 * Math.sin(t * 0.35 + 2.1), 0.56 + 0.32 * Math.cos(t * 0.29 + 1.3)],
            [0.50 + 0.34 * Math.cos(t * 0.31 + 4.0), 0.36 + 0.30 * Math.sin(t * 0.45 + 3.2)],
            [0.46 + 0.32 * Math.sin(t * 0.27 + 5.3), 0.64 + 0.28 * Math.cos(t * 0.49 + 0.7)]
        ]
        const colors = [p, s, a, c4]
        const rr = Field.lobe * h
        ctx.globalCompositeOperation = "lighter"
        for (let i = 0; i < 4; ++i)
            blob(ctx, centers[i][0] * w, centers[i][1] * h, rr * Field.lobes[i].factor, colors[i], Field.lobes[i].alpha)
        ctx.globalCompositeOperation = "source-over"

        // The vertical shade, sampled from the shader's quadratic curve.
        const v = ctx.createLinearGradient(0, 0, 0, h)
        for (let i = 0; i < Field.shade.length; ++i)
            v.addColorStop(i / (Field.shade.length - 1), "rgba(0,0,0," + Field.shade[i] + ")")
        ctx.fillStyle = v
        ctx.fillRect(0, 0, w, h)
    }
}
