#version 440
// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.

    // PAIRED with spectrum_bars.frag (SpectrumBand). THE PAIRING RULE (see
    // scene_pack.vert): both stages must declare the SAME uniform block,
    // member for member, or the OpenGL program link rejects the two layouts
    // (Vulkan never links, so it tolerated the mismatch). Keep this block
    // byte-identical to spectrum_bars.frag's.

    layout(location = 0) in vec4 qt_Vertex;
    layout(location = 1) in vec2 qt_MultiTexCoord0;
    layout(location = 0) out vec2 qt_TexCoord0;

    layout(std140, binding = 0) uniform buf {
        mat4 qt_Matrix;
        vec4 topColor;
        vec4 bottomColor;
        float qt_Opacity;
        float bandW;
        float bandH;
        float b0;
        float b1;
        float b2;
        float b3;
        float b4;
        float b5;
        float b6;
        float b7;
        float b8;
        float b9;
        float b10;
        float b11;
        float b12;
        float b13;
    };

    out gl_PerVertex { vec4 gl_Position; };

    void main() {
        qt_TexCoord0 = qt_MultiTexCoord0;
        gl_Position = qt_Matrix * qt_Vertex;
    }
