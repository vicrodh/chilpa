#version 440
// I draw every column in a fixed quad; only bounded uniforms change.
layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;
layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    vec2 dims;
    vec4 topColor;
    vec4 bottomColor;
    float mode;
    mat4 amplitudes0;
    mat4 peaks;
};

float element(mat4 values, int index) { return values[index % 4][index / 4]; }
void main() {
    float slot = dims.x / 28.0;
    vec2 pixel = qt_TexCoord0 * dims;
    int column = min(27, int(pixel.x / max(slot, 0.0001)));
    int bin = column < 14 ? column : 27 - column;
    if (bin > 0) bin += 1;
    float value = clamp(element(amplitudes0, bin), 0.0, 1.0);
    float x = pixel.x - float(column) * slot;
    float row = floor((dims.y - pixel.y) / max(slot, 0.0001));
    float cellY = mod(dims.y - pixel.y, max(slot, 0.0001));
    float coverage = step(1.0, x) * step(x, slot - 1.0) * step(1.0, cellY) * step(cellY, slot - 1.0)
        * step((row + 0.5) * slot, value * dims.y);
    float peakCoverage = 0.0;
    if (mode < 5.5) {
        float peak = element(peaks, bin);
        float peakY = max(0.0, dims.y * (1.0 - peak) - 1.0);
        peakCoverage = step(0.001, peak) * step(1.0, x) * step(x, slot - 1.0)
            * step(peakY, pixel.y) * step(pixel.y, peakY + 1.0);
        coverage = max(coverage, peakCoverage);
    }
    float gradient = clamp((pixel.y - dims.y * (1.0 - value)) / max(value * dims.y, 1.0), 0.0, 1.0);
    vec4 color = mix(mix(topColor, bottomColor, gradient), topColor, peakCoverage);
    fragColor = color * coverage * qt_Opacity;
}
