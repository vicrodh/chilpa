#version 440

// Paired with atmosphere.frag: both stages declare the SAME uniform block,
// member for member, or the OpenGL program link rejects the two layouts.
// Keep this block byte-identical to atmosphere.frag's.

layout(location = 0) in vec4 qt_Vertex;
layout(location = 1) in vec2 qt_MultiTexCoord0;
layout(location = 0) out vec2 qt_TexCoord0;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float resX;
    float resY;
    float dim;
    vec4 l1;
    vec4 l2;
    vec4 l3;
    vec4 l4;
};

out gl_PerVertex { vec4 gl_Position; };

void main() {
    qt_TexCoord0 = qt_MultiTexCoord0;
    gl_Position = qt_Matrix * qt_Vertex;
}
