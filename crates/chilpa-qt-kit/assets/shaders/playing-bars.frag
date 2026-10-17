#version 440
// I animate four rounded bars in one fixed quad, including their edge coverage.
layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    vec2 dims;
    vec4 tint;
    vec2 geometry;
    float radius;
    float phase;
};

void main() {
    vec2 p = qt_TexCoord0 * (dims + vec2(2.0 * radius)) - vec2(radius);
    vec4 amplitudes = vec4(0.6, 1.0, 0.4, 0.8);
    vec4 phases = vec4(0.0, 0.25, 0.125, 0.375);
    float d = 1000000.0;
    // I take the union before derivatives, keeping coverage continuous in gaps.
    for (int column = 0; column < 4; ++column) {
        float u = fract(phase + phases[column]);
        float h = dims.y * amplitudes[column] * (0.3 + 0.7 * (0.5 - 0.5 * cos(u * 6.283185)));
        vec2 halfSize = vec2(geometry.x, h) * 0.5;
        float r = min(radius, min(halfSize.x, halfSize.y));
        vec2 center = vec2(float(column) * geometry.y + halfSize.x, dims.y - halfSize.y);
        vec2 q = abs(p - center) - (halfSize - vec2(r));
        d = min(d, length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r);
    }
    float coverage = clamp(0.5 - d / max(fwidth(d), 0.0001), 0.0, 1.0);
    fragColor = tint * coverage * qt_Opacity;
}
