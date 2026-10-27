#version 440
// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.

    // PAIRED with spectrum_wave.frag (SpectrumBand). THE PAIRING RULE (see
    // scene_pack.vert): both stages must declare the SAME uniform block,
    // member for member, or the OpenGL program link rejects the two layouts.
    // Keep this block byte-identical to spectrum_wave.frag's.

    layout(location = 0) in vec4 qt_Vertex;
    layout(location = 1) in vec2 qt_MultiTexCoord0;
    layout(location = 0) out vec2 qt_TexCoord0;

    layout(std140, binding = 0) uniform buf {
        mat4 qt_Matrix;
        vec4 topColor;
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
        float b14;
        float b15;
        float b16;
        float b17;
        float b18;
        float b19;
        float b20;
        float b21;
        float b22;
        float b23;
        float b24;
        float b25;
        float b26;
        float b27;
        float b28;
        float b29;
        float b30;
        float b31;
        float b32;
        float b33;
        float b34;
        float b35;
        float b36;
        float b37;
        float b38;
        float b39;
        float b40;
        float b41;
        float b42;
        float b43;
        float b44;
        float b45;
        float b46;
        float b47;
    };

    out gl_PerVertex { vec4 gl_Position; };

    void main() {
        qt_TexCoord0 = qt_MultiTexCoord0;
        gl_Position = qt_Matrix * qt_Vertex;
    }
