// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
#version 440

// Line Bed (immersive scene, mode 5) — VERTEX stage. A line-faithful GLSL
// 440 port of `vs_main` in crates/qbz-ui/ui/shaders/line_bed.wgsl (itself a
// clean-room port of the Tauri LinebedPanel projection math): 200
// depth-stacked polylines, one historical 256-point spectrum each (X =
// frequency, Y = magnitude, Z = age), projected with a LEVELED pitch-only
// camera, each band span subdivided with a Catmull-Rom-STYLE uniform cubic
// B-spline across the 4 neighboring bands so the polylines read as smooth
// curves. The 256x200 heights arrive as an R32F texture (binding 4),
// DEPTH-ORDERED row 0 = newest, fed by the Rust LineBedState
// (src/linebed_qt.rs) — `textureLoad` becomes `texelFetch`.
//
// The ONLY mechanical differences from the WGSL, enumerated:
//   * @builtin(vertex_index) / @builtin(instance_index) become two FLOAT
//     vertex attributes (`vid`, `lineIdx`) driven from the 1531-vert
//     template buffer and a 200-entry per-instance buffer — the portable
//     spelling under QRhi (GLES 3.0 has no gl_VertexID; the C++ side
//     uploads 0..1530 / 0..199 verbatim).
//   * `u.resolution: vec2` sits at offset 0 of a TRIMMED uniform block
//     (this scene reads only resolution in the VS and primary/accent in the
//     FS — line_bed.wgsl has no time/level/energy input); same values,
//     same meaning as the 144-byte block's fields.
//   * the FS cutoff coordinate is exported as the `yFrac` varying
//     (screen_y / resolution.y) instead of being re-read from
//     @builtin(position) in the FS — gl_FragCoord.y is BOTTOM-up on GL,
//     top-down on wgpu/Vulkan, so the portable route is to interpolate the
//     value the VS already computed. w == 1 throughout, so the interpolated
//     yFrac equals the reference's per-pixel pos.y/resolution.y exactly.
//
// QSB-SKIP-GLES100 — texelFetch (vertex texture fetch) needs GLSL ES 3.00;
// the ES 2.0 variant cannot carry this shader and the tier gate keeps the
// scene off that hardware anyway.
// QSB-SKIP-BATCH — a QQuickRhiItem shader, NOT a Qt Quick ShaderEffect
// stage: the `-b` batching rewrite is only valid for scene-graph batched
// items and would corrupt the custom attribute layout.

layout(location = 0) in float vid;      // 0..1530: subdivided point index
layout(location = 1) in float lineIdx;  // 0..199: depth row (per instance)

layout(std140, binding = 0) uniform buf {
    vec2 resolution;
    vec4 primary;
    vec4 accent;
} u;

layout(binding = 4) uniform sampler2D heights_tex;

layout(location = 0) out float intensity;
layout(location = 1) out float yFrac;

out gl_PerVertex { vec4 gl_Position; };

// --- Camera constants (verbatim from line_bed.wgsl:42-59, which took them
// verbatim from LinebedPanel.svelte) ---------------------------------------
const float LINE_LENGTH = 9.0;
const float WORLD_AMPLITUDE = 2.4;
const float PLANE_HALF_WIDTH = 1147.5;   // (255*9)/2
const float PLANE_HALF_DEPTH = 1990.0;   // (199*20)/2
const float CAM_X = 26.1;
const float CAM_Y = 1738.6;
const float CAM_Z = 868.8;
const float CAMERA_NEAR = 80.0;
const float FOV_DEG = 45.0;              // vertical
const int NUM_BANDS = 256;
// MUST match the host's LINEBED_SUBDIV (shader_underlay.rs:54 /
// linebed_qt.rs) and the 1531-vert template the C++ uploads.
const float SUBDIV = 6.0;
// Vertical screen position of the projection origin (lower = the bed sits
// HIGHER). 0.30 so the bed lifts off the bottom, clears the player bar,
// and uses the dead space up top.
const float VCENTER = 0.30;

float height_at(int line, int band) {
    int b = clamp(band, 0, NUM_BANDS - 1);
    return texelFetch(heights_tex, ivec2(b, line), 0).r;
}

// Uniform cubic B-spline through the 4 neighboring bands — C2-continuous
// (no kinks at the band points) and never overshoots; it APPROXIMATES
// (gently smooths) the samples, so the whole line reads as one continuous
// curve instead of stitched segments.
float curve_height(int line, float band_f) {
    int b0 = int(floor(band_f));
    float t = band_f - float(b0);
    float p0 = height_at(line, b0 - 1);
    float p1 = height_at(line, b0);
    float p2 = height_at(line, b0 + 1);
    float p3 = height_at(line, b0 + 2);
    float t2 = t * t;
    float t3 = t2 * t;
    float w0 = (1.0 - 3.0 * t + 3.0 * t2 - t3) / 6.0;
    float w1 = (4.0 - 6.0 * t2 + 3.0 * t3) / 6.0;
    float w2 = (1.0 + 3.0 * t + 3.0 * t2 - 3.0 * t3) / 6.0;
    float w3 = t3 / 6.0;
    return p0 * w0 + p1 * w1 + p2 * w2 + p3 * w3;
}

void main() {
    int line = int(lineIdx);
    // Continuous (subdivided) band position along the line, 0..255.
    float band_f = vid / SUBDIV;
    float h = curve_height(line, band_f);

    // World position. Only Y is audio-driven; X/Z are the fixed lattice.
    float world_x = band_f * LINE_LENGTH - PLANE_HALF_WIDTH;
    float world_y = h * WORLD_AMPLITUDE;
    float depth_factor = lineIdx / 199.0;
    float world_z = -PLANE_HALF_DEPTH + depth_factor * (PLANE_HALF_DEPTH * 2.0);

    // LEVELED view transform: pitch ONLY (Rx). Yaw and roll are ZEROED so
    // every line sits flat.
    float cosX = cos(0.6543);
    float sinX = sin(0.6543);

    float tX = world_x - CAM_X;
    float tY = world_y - CAM_Y;
    float tZ = world_z - CAM_Z;

    // Rz, Ry = identity (leveled); apply Rx (pitch about the X axis).
    float rX = tX;
    float rY = tY * cosX - tZ * sinX;
    float rZ = tY * sinX + tZ * cosX;

    // Near-clip (v1 — no polyline split, like the reference).
    float depth = max(-rZ, CAMERA_NEAR);

    // SAME focal for X and Y (no aspect correction — faithful to Tauri).
    float focal = u.resolution.y * 0.5 / tan(FOV_DEG * 3.14159265 / 360.0);
    float screen_x = u.resolution.x * 0.5 + rX * focal / depth;
    float screen_y = u.resolution.y * VCENTER - rY * focal / depth;

    // Screen px -> clip/NDC (flip Y — QRhi's NDC is y-up like wgpu's).
    float ndc_x = screen_x / u.resolution.x * 2.0 - 1.0;
    float ndc_y = 1.0 - screen_y / u.resolution.y * 2.0;

    gl_Position = vec4(ndc_x, ndc_y, 0.0, 1.0);
    intensity = clamp(h / 84.0, 0.0, 1.0);
    yFrac = screen_y / u.resolution.y;
}
