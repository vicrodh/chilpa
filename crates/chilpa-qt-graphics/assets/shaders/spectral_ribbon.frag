// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
#version 440

// Spectral Ribbon (immersive scene, mode 4) — FRAGMENT stage. Line-faithful
// GLSL 440 port of `fs_main` in crates/qbz-ui/ui/shaders/spectral_ribbon.wgsl
// (the paint-as-you-play sonogram: frequency on Y, time on X, the Spek
// purple→orange heatmap). The CPU side keeps the persistent R8 spectrogram
// (512 freq bands × 2048 time columns, shader_underlay.rs:44-45); the Qt
// port's Rust half publishes one 512-byte row per viz tick and the C++
// RibbonItem uploads it at the playback-time column with the same gap-fill
// (shader_underlay.rs:897-927).
//
// Mechanical differences from the WGSL, enumerated:
//   * textureSample(spectrogram, samp, uv) -> texture(spectrogram, uv)
//     (combined image sampler, binding 3 like the reference).
//   * log(x)/log(10.0) stays as-is (GLSL has no log10 on all targets; the
//     two-log spelling IS the reference's).
// The plot-rect Y convention (PLOT_Y0/Y1 already flipped vs the Slint
// overlay, spectral_ribbon.wgsl:77-81) is UNCHANGED — the QML overlay
// (SpectralOverlay.qml) keeps the overlay's top-down fractions, and the
// item's mirrorVertically makes the GL composite match wgpu's display.

layout(std140, binding = 0) uniform buf {
    float time;
    float phase;
    float beat;
    float level;
    vec2 resolution;
    float levelSmooth;
    float transientAmp;
    vec4 energyLo;
    vec4 energyHi;
    vec4 bandsLo;
    vec4 bandsHi;
    vec4 primary;
    vec4 secondary;
    vec4 accent;
} u;

layout(binding = 3) uniform sampler2D spectrogram;

layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 fragColor;

// Spek-like 7-stop purple→magenta→orange ramp (Tauri spekColor), linear RGB.
vec3 spek_color(float x) {
    float t = clamp(x, 0.0, 1.0);
    // stops: 0.00 black, 0.36 deep-blue, 0.60 indigo, 0.78 purple,
    //        0.92 magenta, 0.98 red-orange, 1.00 orange.
    if (t < 0.36) {
        return mix(vec3(0.0, 0.0, 0.0), vec3(0.0, 0.0, 0.243), t / 0.36);
    } else if (t < 0.60) {
        return mix(vec3(0.0, 0.0, 0.243), vec3(0.055, 0.0, 0.392), (t - 0.36) / 0.24);
    } else if (t < 0.78) {
        return mix(vec3(0.055, 0.0, 0.392), vec3(0.361, 0.0, 0.439), (t - 0.60) / 0.18);
    } else if (t < 0.92) {
        return mix(vec3(0.361, 0.0, 0.439), vec3(0.745, 0.0, 0.282), (t - 0.78) / 0.14);
    } else if (t < 0.98) {
        return mix(vec3(0.745, 0.0, 0.282), vec3(0.863, 0.188, 0.0), (t - 0.92) / 0.06);
    } else {
        return mix(vec3(0.863, 0.188, 0.0), vec3(0.933, 0.471, 0.125), (t - 0.98) / 0.02);
    }
}

// Plot rectangle in 0..1 screen space (leaves a margin for axes; the bottom
// band clears the song card / time axis). MUST match SpectralOverlay.qml.
const float PLOT_X0 = 0.055;
const float PLOT_X1 = 0.970;
const float PLOT_Y0 = 0.220;
const float PLOT_Y1 = 0.930;

void main() {
    vec3 bg = vec3(0.012, 0.027, 0.047);  // #03070c

    // Inside the plot rectangle?
    if (uv.x < PLOT_X0 || uv.x > PLOT_X1 || uv.y < PLOT_Y0 || uv.y > PLOT_Y1) {
        fragColor = vec4(bg, 1.0);
        return;
    }

    // Plot-local coords. tf = time fraction (left→right), ff = freq
    // fraction (band 0/bass at the BOTTOM, 511/Nyquist at the TOP).
    float tf = (uv.x - PLOT_X0) / (PLOT_X1 - PLOT_X0);
    float ff = (uv.y - PLOT_Y0) / (PLOT_Y1 - PLOT_Y0);

    // Sample the spectrogram: u = frequency band (0..1 over 512), v = time
    // column. Un-played columns are zero -> background.
    float amp = texture(spectrogram, vec2(ff, tf)).r;

    // dB + gamma (Tauri): db in [-120,0] -> normalized -> ^2.15.
    float db = 20.0 * log(max(1e-6, amp)) / log(10.0);
    float db_norm = clamp((db + 120.0) / 120.0, 0.0, 1.0);
    float toned = pow(db_norm, 2.15);
    vec3 col = spek_color(toned);
    float a = (10.0 + toned * 156.0) / 255.0;

    // Composite the heatmap over the dark background.
    vec3 outc = mix(bg, col, a);

    // Real-time ceiling line: a horizontal line at the highest active
    // frequency (u.energyHi.y = smoothed peak band fraction), spanning the
    // full plot width, in the axis green.
    float on_line = 1.0 - smoothstep(0.0, 0.006, abs(ff - u.energyHi.y));
    outc = mix(outc, vec3(0.373, 0.722, 0.478), on_line * 0.65);

    fragColor = vec4(outc, 1.0);
}
