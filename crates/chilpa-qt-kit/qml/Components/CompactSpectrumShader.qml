// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
import QtQuick

ShaderEffect {
    id: effect
    property var values: []
    property var peakValues: []
    objectName: "compactVisualizerShader"
    function matrix(array, offset) {
        return Qt.matrix4x4(array[offset] || 0, array[offset + 1] || 0, array[offset + 2] || 0, array[offset + 3] || 0,
            array[offset + 4] || 0, array[offset + 5] || 0, array[offset + 6] || 0, array[offset + 7] || 0,
            array[offset + 8] || 0, array[offset + 9] || 0, array[offset + 10] || 0, array[offset + 11] || 0,
            array[offset + 12] || 0, array[offset + 13] || 0, array[offset + 14] || 0, array[offset + 15] || 0)
    }
    function pushUniforms() {
        if (mode >= 5) return
        const size = mode === 0 ? 14 : mode === 1 ? 48 : 5
        for (let i = 0; i < size; ++i)
            effect["b" + i] = values[mode === 0 ? (i === 0 ? 0 : i + 1) : i] || 0
    }
    onValuesChanged: pushUniforms()
    property vector2d dims: Qt.vector2d(width, height)
    property real bandW: width
    property real bandH: height
    property color topColor
    property color bottomColor
    property real mode: 0
    property matrix4x4 amplitudes0: mode >= 5 ? matrix(values, 0) : matrix([], 0)
    property matrix4x4 peaks: matrix(peakValues, 0)
    readonly property string shaderStem: mode === 0 ? "compact-bars"
        : mode === 1 ? "compact-wave" : mode === 2 ? "compact-energy" : "compact-visualizer"
    vertexShader: "qrc:/qt/qml/app/chilpa/kit/assets/shaders/" + shaderStem + ".vert.qsb"
    fragmentShader: "qrc:/qt/qml/app/chilpa/kit/assets/shaders/" + shaderStem + ".frag.qsb"
    Component.onCompleted: pushUniforms()
    property real b0: 0
    property real b1: 0
    property real b2: 0
    property real b3: 0
    property real b4: 0
    property real b5: 0
    property real b6: 0
    property real b7: 0
    property real b8: 0
    property real b9: 0
    property real b10: 0
    property real b11: 0
    property real b12: 0
    property real b13: 0
    property real b14: 0
    property real b15: 0
    property real b16: 0
    property real b17: 0
    property real b18: 0
    property real b19: 0
    property real b20: 0
    property real b21: 0
    property real b22: 0
    property real b23: 0
    property real b24: 0
    property real b25: 0
    property real b26: 0
    property real b27: 0
    property real b28: 0
    property real b29: 0
    property real b30: 0
    property real b31: 0
    property real b32: 0
    property real b33: 0
    property real b34: 0
    property real b35: 0
    property real b36: 0
    property real b37: 0
    property real b38: 0
    property real b39: 0
    property real b40: 0
    property real b41: 0
    property real b42: 0
    property real b43: 0
    property real b44: 0
    property real b45: 0
    property real b46: 0
    property real b47: 0
}
