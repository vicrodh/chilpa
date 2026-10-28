import QtQuick
import QtTest
import app.chilpa.graphics
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

TestCase {
    id: test
    name: "NativeSpectrum"
    when: windowShown
    visible: true
    width: 1280
    height: 1008
    property int plotWidth: 448
    property int plotHeight: 224
    Rectangle {
        id: reference; width: test.plotWidth; height: test.plotHeight; color: "black"
        CompactSpectrumShader { id: quad; anchors.fill: parent; topColor: "white"; bottomColor: "white" }
    }
    Rectangle {
        id: candidate; y: test.plotHeight; width: test.plotWidth; height: test.plotHeight; color: "black"
        SpectrumItem { id: columns; anchors.fill: parent; topColor: "white"; bottomColor: "white" }
    }
    function test_native_geometry_preserves_every_lit_pixel_data() {
        return [{tag: "compact", width: 448, height: 224}, {tag: "immersive", width: 1280, height: 504},
            {tag: "ambient-gradient", width: 448, height: 224, gradient: true},
            {tag: "translucent-gradient", width: 448, height: 224, gradient: true, translucent: true},
            {tag: "ancestor-opacity", width: 448, height: 224, gradient: true, opacity: 0.5},
            {tag: "immersive-rectangles", width: 1280, height: 504, rows: 2, thick: 3}]
    }
    function test_native_geometry_preserves_every_lit_pixel(data) {
        if (GraphicsInfo.api === GraphicsInfo.Software || GraphicsInfo.api === GraphicsInfo.Null)
            skip("The native geometry requires the native RHI renderer")
        plotWidth = data.width; plotHeight = data.height
        quad.rowDivisions = columns.rowDivisions = data.rows || 1
        quad.peakThickness = columns.peakThickness = data.thick || 1
        reference.opacity = candidate.opacity = data.opacity || 1
        quad.topColor = columns.topColor = data.translucent ? "#8042d9c8" : data.gradient ? "#42d9c8" : "white"
        quad.bottomColor = columns.bottomColor = data.translucent ? "#c0416bd8" : data.gradient ? "#416bd8" : "white"
        for (const mode of [5, 6]) {
            quad.mode = mode; columns.mode = mode
            for (const level of (data.gradient ? [0.33] : [0, 0.001, 0.08, 0.33, 0.5, 1])) {
                const values = Array(16).fill(level).map((v, i) => level === 0.33 ? (i * 7 % 16) / 15 : v)
                const peaks = values.map((v, i) => v === 0 ? 0 : Math.min(1, v + (i % 4) * 0.1))
                quad.values = values; columns.values = values
                quad.peakValues = peaks; columns.peakValues = peaks
                waitForRendering(candidate)
                tryCompare(quad, "status", ShaderEffect.Compiled)
                const expected = grabImage(reference)
                const actual = grabImage(candidate)
                let changed = 0, maximum = 0
                for (let y = 0; y < plotHeight; ++y) {
                    for (let x = 0; x < plotWidth; ++x) {
                        let delta = Math.abs(expected.red(x, y) - actual.red(x, y))
                        if (data.gradient) delta = Math.max(delta, Math.abs(expected.green(x, y) - actual.green(x, y)),
                            Math.abs(expected.blue(x, y) - actual.blue(x, y)))
                        if (delta > 1) changed++
                        maximum = Math.max(maximum, delta)
                    }
                }
                compare(changed, 0, "mode " + mode + ", level " + level + ", largest rounding difference " + maximum)
            }
        }
    }
}
