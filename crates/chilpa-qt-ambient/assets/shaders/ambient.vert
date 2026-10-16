#version 440

// Paired with ambient.frag: both stages declare the SAME uniform block,
// member for member, or the OpenGL program link rejects the two layouts
// (Vulkan never links, so it tolerates the mismatch). Keep this block
// byte-identical to ambient.frag's.

layout(location = 0) in vec4 qt_Vertex;
layout(location = 1) in vec2 qt_MultiTexCoord0;
layout(location = 0) out vec2 qt_TexCoord0;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    vec4 primary;
    vec4 secondary;
    vec4 accent;
    float qt_Opacity;
    float time;
    float levelSmooth;
    float resX;
    float resY;
    float dim;
};

out gl_PerVertex { vec4 gl_Position; };

void main() {
    qt_TexCoord0 = qt_MultiTexCoord0;
    gl_Position = qt_Matrix * qt_Vertex;
}
