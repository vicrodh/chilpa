// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
#version 440

// Line Bed (immersive scene, mode 5) — FRAGMENT stage. Line-faithful port
// of `fs_main` in crates/qbz-ui/ui/shaders/line_bed.wgsl:
//
//   * the bottom cutoff (`in.pos.y > u.resolution.y * 0.86 → discard`)
//     reads the `yFrac` varying the VS computed from its own screen-space
//     position — identical value to the reference's framebuffer-space
//     builtin, portable across the gl_FragCoord y-orientation split (see
//     the vertex shader's header);
//   * the gradient: valleys take `primary`, peaks tip toward `accent`
//     (`g = clamp(intensity * 1.25, 0, 1)`), peaks also brighten via alpha
//     (0.5 + g*0.4) under the pipeline's SrcAlpha blend.
//
// QSB-SKIP-GLES100 — paired with the vertex stage (texelFetch); the ES 2.0
// variant cannot carry the scene and the tier gate keeps it off that
// hardware anyway.

layout(std140, binding = 0) uniform buf {
    vec2 resolution;
    vec4 primary;
    vec4 accent;
} u;

layout(location = 0) in float intensity;
layout(location = 1) in float yFrac;

layout(location = 0) out vec4 fragColor;

void main() {
    // Bottom cutoff — don't render below ~86% of the view, so the bed ends
    // above the player bar instead of spilling to the window bottom.
    if (yFrac > 0.86)
        discard;
    // Height gradient: the bed/valleys take `primary`, the mountain peaks
    // tip toward `accent`, gradually — peaks also brighten.
    float g = clamp(intensity * 1.25, 0.0, 1.0);
    vec3 col = mix(u.primary.rgb, u.accent.rgb, g);
    float a = 0.5 + g * 0.4;
    fragColor = vec4(col, a);
}
