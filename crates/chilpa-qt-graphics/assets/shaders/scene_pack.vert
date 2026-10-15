// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
#version 440

// PAIRED with tunnel.frag AND aurora.frag (their uniform layouts are
// identical, so one vertex stage serves both).
//
// THE PAIRING RULE (2026-08-15 black-Tunnel report): a ShaderEffect's two
// stages are LINKED into one program under the OpenGL RHI backend, and GLSL
// refuses one block name with two layouts ("uniform `_22' declared as type
// `buf' and type `buf'"). Vulkan never links stages, so the shared
// spectrum.vert (block = qt_Matrix + qt_Opacity) paired with these wider
// fragment blocks rendered fine there and died only on GL. The fix is Qt's
// own documented pattern: BOTH stages declare the SAME block, member for
// member, in the same order — the vertex stage simply never reads the
// fragment-only fields. Keep this block byte-identical to tunnel.frag's.

layout(location = 0) in vec4 qt_Vertex;
layout(location = 1) in vec2 qt_MultiTexCoord0;
layout(location = 0) out vec2 qt_TexCoord0;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    vec4 primary;
    vec4 secondary;
    vec4 accent;
    vec4 energyLo;
    vec4 energyHi;
    vec4 bandsLo;
    vec4 bandsHi;
    float qt_Opacity;
    float time;
    float phase;
    float beat;
    float level;
    float resX;
    float resY;
    float levelSmooth;
    float transientAmp;
};

out gl_PerVertex { vec4 gl_Position; };

void main() {
    qt_TexCoord0 = qt_MultiTexCoord0;
    gl_Position = qt_Matrix * qt_Vertex;
}
