// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
#version 440

// Aurora (immersive scene, mode 3) — a line-faithful GLSL 440 port of
// crates/qbz-ui/ui/shaders/aurora.wgsl (AURORA WARP, original clean-room
// shader; the stage-0 spike scene of the 2026-08-15 immersive-completion
// contract, chosen because it exercises the full uniform pack with zero
// extra GPU resources). Every constant and every formula is the reference's;
// the differences are mechanical (same list as tunnel.frag): uniform block
// instead of a bind group, resolution as resX/resY, energy/bands vec4s by
// their QML property names, `let` -> local floats, uv = qt_TexCoord0 with NO
// y-flip (the ambient.frag convention), premultiplied output.
//
// Three sine-domain-warped curtains take their colors from the album-art
// palette (primary / secondary / accent); bass swells the low curtain, mids
// drive sway speed/amplitude, presence the top curtain, air = shimmer
// frequency (5.0 + air*8.0) cross-fading toward accent, beat kicks the
// brightness and lifts the curtains, level_smooth sets the baseline bloom.
// Fixed work per pixel, no loops — GPU-cheap.
//
// Pure float math (no integer ops), so this shader keeps the FULL qsb
// variant set including 100 es (unlike ambient.frag / tunnel.frag).

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

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
    // Named transientAmp, not `transient` — a RESERVED QML keyword (see
    // tunnel.frag). The JSON pack key stays `transient`.
    float transientAmp;
};

// A small smooth flow field: cheap sine lobes that act like a fake curl, so
// the curtains ripple horizontally instead of sliding rigidly.
float flow(vec2 p, float t) {
    float a = sin(p.x * 2.3 + t * 0.7) * 0.5;
    float b = sin(p.x * 4.7 - t * 0.45 + p.y * 1.3) * 0.25;
    float c = sin((p.x + p.y) * 1.7 + t * 0.9) * 0.18;
    return a + b + c;
}

void main() {
    float t = time;
    float bass = clamp(energyLo.y, 0.0, 1.0);
    float mids = clamp(energyLo.z, 0.0, 1.0);
    float presence = clamp(energyLo.w, 0.0, 1.0);
    float air = clamp(energyHi.x, 0.0, 1.0);
    float beatC = clamp(beat, 0.0, 1.0);
    float lvl = clamp(levelSmooth, 0.0, 1.0);

    float aspect = resX / max(resY, 1.0);
    // x spread by aspect so the curtains aren't squashed on wide windows.
    vec2 p = vec2((qt_TexCoord0.x - 0.5) * aspect, qt_TexCoord0.y);

    // Mids drive the sway amplitude & speed.
    float swaySpeed = 0.6 + mids * 0.9;
    float warp1 = flow(p * 3.0, t * swaySpeed);
    float warp2 = flow(p * 2.0 + vec2(7.3, 0.0), t * (0.8 * swaySpeed) + 2.0);
    float warp3 = flow(p * 2.6 + vec2(-4.1, 1.0), t * (0.65 * swaySpeed) + 4.0);

    // Curtain widths: bass swells the low curtain; beat lifts; presence the
    // top.
    float bw1 = 0.10 + bass * 0.16 + beatC * 0.03;
    float bw2 = 0.09 + mids * 0.10;
    float bw3 = 0.07 + presence * 0.10;

    // Anchors drift on a slow vertical LFO + a beat lift; sway from warp +
    // mids.
    float lift = beatC * 0.03;
    float sway = 0.08 + mids * 0.12;
    float center1 = 0.40 + 0.02 * sin(t * 0.23) + warp1 * sway - lift;
    float center2 = 0.60 + 0.02 * sin(t * 0.19 + 1.7) + warp2 * sway - lift;
    float center3 = 0.50 + 0.025 * sin(t * 0.16 + 3.1) + warp3 * (sway * 0.9) - lift;

    float band1 = bw1 / (abs(p.y - center1) + bw1);
    float band2 = bw2 / (abs(p.y - center2) + bw2);
    float band3 = bw3 / (abs(p.y - center3) + bw3);

    // Treble shimmer cross-fades each curtain base color toward accent.
    float shFreq = 5.0 + air * 8.0;
    float sh1 = 0.5 + 0.5 * sin(p.x * shFreq + t * 1.3);
    float sh2 = 0.5 + 0.5 * sin(p.x * (shFreq * 0.8) - t * 1.1 + 1.0);
    float sh3 = 0.5 + 0.5 * sin(p.x * (shFreq * 1.2) + t * 1.6 + 2.0);
    float shGain = 0.25 + air * 0.5;

    vec3 col1 = mix(primary.rgb, accent.rgb, sh1 * shGain);
    vec3 col2 = mix(secondary.rgb, accent.rgb, sh2 * shGain);
    vec3 col3 = mix(accent.rgb, primary.rgb, sh3 * shGain);

    vec3 col = col1 * band1 * (0.55 + sh1 * 0.45);
    col += col2 * band2 * (0.55 + sh2 * 0.45);
    col += col3 * band3 * (0.5 + sh3 * 0.4) * (0.4 + presence * 0.8);

    // Baseline brightness from the smoothed level + beat; dim primary floor
    // glow.
    col *= 0.5 + lvl * 0.9;
    col *= 1.0 + beatC * 0.5;
    col += primary.rgb * 0.04;

    // Beat bloom: vertical shimmer streaks where the curtains already are.
    float streak = abs(fract(p.y * 6.0 - t * 2.0) - 0.5);
    float bloom = (0.5 / (streak * 8.0 + 0.5)) * beatC;
    col += accent.rgb * bloom * (band1 + band2 + band3) * 0.4;

    col = clamp(col, vec3(0.0), vec3(1.0));
    // Premultiplied, like every other effect in this tree (blending: false at
    // the mount, so qt_Opacity is 1 and this equals the reference frame).
    fragColor = vec4(col * qt_Opacity, qt_Opacity);
}
