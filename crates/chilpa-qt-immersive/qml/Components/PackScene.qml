import QtQuick
import app.chilpa.ambient

// The shared uniform block for the tunnel and aurora fragment stages.
ShaderEffect {
    id: effect
    required property var uniforms
    signal failed(string details)
    blending: false
    property color primary: Ambient.primary
    property color secondary: Ambient.secondary
    property color accent: Ambient.accent
    property vector4d energyLo: uniforms.energyLo
    property vector4d energyHi: uniforms.energyHi
    property vector4d bandsLo: uniforms.bandsLo
    property vector4d bandsHi: uniforms.bandsHi
    property real time: uniforms.timeS
    property real phase: uniforms.phase
    property real beat: uniforms.beat
    property real level: uniforms.level
    property real resX: uniforms.resW
    property real resY: uniforms.resH
    property real levelSmooth: uniforms.levelSmooth
    property real transientAmp: uniforms.transientAmp
    vertexShader: "qrc:/qt/qml/app/chilpa/graphics/assets/shaders/scene_pack.vert.qsb"
    onStatusChanged: if (status === ShaderEffect.Error) failed(log)
}
