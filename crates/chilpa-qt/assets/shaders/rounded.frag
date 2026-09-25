#version 440
// A cover with rounded corners, in one pass.
//
// The distance to a rounded rectangle is computed per pixel, so the corners
// are antialiased without rendering the image into a layer first: no
// offscreen texture per cover, no extra pass, which is what a layer and a
// mask effect would cost for every cover on screen.

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    vec2 dims;
    float radius;
};

layout(binding = 1) uniform sampler2D source;

void main() {
    vec2 p = qt_TexCoord0 * dims;
    vec2 q = abs(p - dims * 0.5) - (dims * 0.5 - vec2(radius));
    float d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - radius;
    // One device pixel of antialiasing, whatever the scale.
    float aa = max(fwidth(d), 0.0001);
    float inside = clamp(0.5 - d / aa, 0.0, 1.0);
    fragColor = texture(source, qt_TexCoord0) * inside * qt_Opacity;
}
