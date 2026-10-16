// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
#version 440

// Tunnel (immersive scene, mode 2) — a line-faithful GLSL 440 port of
// crates/qbz-ui/ui/shaders/tunnel.wgsl (DOOM TUNNEL, original clean-room
// shader). Every constant and every formula below is the reference's number;
// the differences are mechanical and are enumerated here:
//
//   * uniform block instead of the bind group (the ShaderEffect contract):
//     qt_Matrix/qt_Opacity are Qt-supplied, everything else is a property on
//     the effect item, resolved BY NAME out of the .qsb reflection data (see
//     ambient.frag). `resolution: vec2` arrives as resX/resY floats, the
//     energy/bands vec4s as energyLo/energyHi/bandsLo/bandsHi — the QML
//     property names are spelled identically on both sides.
//   * uv = qt_TexCoord0 replaces in.uv DIRECTLY (no y-flip) — the same
//     convention the ambient.frag port uses and the owner validated against
//     the reference frame. Vertically this mirrors the wgpu frame; the scene
//     is y-symmetric except the wind/sway phases, so the picture is the
//     reference's mirrored, consistent with the Ambient port.
//   * `let` -> local float decls; `array<f32,8>(...)` -> `float[8](...)`;
//     `atan2(y,x)` -> `atan(y,x)`; `select(f,t,cond)` -> ternary;
//     `u32(ringId & 7)` -> int index, `b[ringId & 7]` — two's-complement
//     `& 7` on a negative ringId wraps identically to WGSL i32&u32 in both
//     languages (-1 & 7 = 7), and the 4096 phase wrap (host side) keeps the
//     ring math seamless exactly like the reference.
//   * Output is premultiplied (`col * qt_Opacity`), like every other effect
//     in this tree; the layer mounts it with blending: false, so opacity is
//     1.0 and the picture equals the reference's `vec4(col, 1.0)`.
//
// QSB-SKIP-GLES100 — build.rs drops the `100 es` variant for this file: GLSL
// ES 1.00 has no integer bitwise ops (band_at's `& 7`), and there is no
// Canvas fallback arm for scenes — on a tier that cannot run the effect the
// picker gate (QbzShell.shaderScenesAvailable) never offers the scene, and a
// load failure makes ShaderSceneLayer hand the background back to the
// atmosphere.
//
// The algorithm (tunnel.wgsl:78-184): NOT a raymarcher — a single fragment
// pass. Chebyshev/box distance r = max(|p.x|, |p.y|) gives SQUARE rings; the
// curvature lives in the PATH (the corridor snakes), never in the
// cross-section; the host forward-motion clock (phase, wrapped at 4096) is
// amplified by FLIGHT_SPEED; colors come from the album-art palette.

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
    // Named transientAmp, not `transient`: the uniform is matched BY NAME to
    // a QML property and `transient` is a RESERVED QML keyword (qmlcachegen
    // rejects it). Same field, same JSON pack key (`transient`) — only the
    // QML-side identifier changes.
    float transientAmp;
};

// One of the 8 log FFT bands, selected by ring index (per-ring spectral
// sweep). WGSL masks with `& 7u`; the int two's-complement equivalent wraps
// negatives the same way.
float band_at(int i) {
    float b[8] = float[8](
        bandsLo.x, bandsLo.y, bandsLo.z, bandsLo.w,
        bandsHi.x, bandsHi.y, bandsHi.z, bandsHi.w
    );
    return b[i & 7];
}

// --- Tunables (the reference's numbers, tunnel.wgsl:65-75) -----------------
// Forward-flight multiplier on the host phase clock. The clock wraps at 4096
// (an integer multiple of 8), so any INTEGER multiplier keeps floor()/fract()/
// `& 7` ring math seamless across the (≈hourly) wrap.
const float FLIGHT_SPEED = 6.0;
// How hard the centerline snakes. The bend is applied to the PATH, so the
// rectangles only translate — they never warp.
const float BEND_AMT = 0.14;
// Number of radial speed-lines fanning out of the vanishing point (even, so
// they stay continuous across the atan seam at ±π).
const float SPOKES = 16.0;

void main() {
    float t = time;
    vec2 uv = qt_TexCoord0;

    // FFT aggregates from the 8 log bands (match the Tauri canvas crossovers:
    // bars 0-3 / 4-9 / 10-15 land on even band-pair boundaries).
    float bass = clamp((bandsLo.x + bandsLo.y) * 0.5, 0.0, 1.0);
    float mid = clamp((bandsLo.z + bandsLo.w + bandsHi.x) / 3.0, 0.0, 1.0);
    float high = clamp((bandsHi.y + bandsHi.z + bandsHi.w) / 3.0, 0.0, 1.0);
    float beatC = clamp(beat, 0.0, 1.0);

    // FAST flight: the host forward-motion clock (base 0.012 + level + beat
    // per frame, wrapped at 4096) amplified.
    float flight = phase * FLIGHT_SPEED;

    // Corridor cross-section is WIDER than tall (a hallway, not a square
    // shaft): wider-than-tall scale derived from the viewport aspect.
    float viewAspect = resX / max(resY, 1.0);
    float scaleX = clamp(1.04 + viewAspect * 0.25, 1.22, 1.58);
    float scaleY = clamp(1.38 - scaleX * 0.45, 0.62, 0.82);

    // Vanishing point: a gentle Lissajous sway (from `time` only, so its own
    // phase never jumps). Kept small — the real motion is the depth-wind.
    float cphase = t * 0.18;
    vec2 center = vec2(0.5, 0.5);
    center.x += sin(cphase) * 0.03 + sin(cphase * 2.08 + 1.2) * 0.012;
    center.y += cos(cphase * 0.82 + 0.7) * 0.02;

    // First pass: unbent cross-section + depth, so we know how DEEP this
    // pixel sits before bending the path.
    vec2 p0 = (uv - center) / vec2(scaleX, scaleY);
    float r0 = max(abs(p0.x), abs(p0.y)) + 1e-4;
    float depth0 = 1.0 / r0;
    float z0 = log2(depth0) - flight;

    // WINDING CENTERLINE — the curvature is in the PATH, not the rectangle.
    // The wave scrolls toward you with `flight`; the near mouth barely bends,
    // deep sections swing the most. Louder music = a livelier road.
    float windE = 0.75 + levelSmooth * 0.6 + beatC * 0.3;
    float windX = sin(z0 * 0.65 + t * 0.6) + 0.5 * sin(z0 * 1.27 - t * 0.9);
    float windY = 0.6 * cos(z0 * 0.85 + t * 0.5) + 0.35 * sin(z0 * 1.6 + t * 0.7);
    float bendDepth = smoothstep(0.0, 2.6, depth0); // ~0 at the mouth -> 1 deep in
    vec2 bend = vec2(windX, windY) * (bendDepth * BEND_AMT * windE);

    // Second pass: BENT cross-section coords; BOX distance (Chebyshev) ->
    // SQUARE rings that ride the winding path.
    vec2 p = p0 - bend;
    float r = max(abs(p.x), abs(p.y)) + 1e-4;
    float depth = 1.0 / r;
    float z = log2(depth) - flight;   // minus -> rings grow outward (forward)
    int ringId = int(floor(z));
    float ringFrac = fract(z);

    // Rectangle outline: bright on the frame edge, dark in the gap. Thickness
    // breathes with bass + beat.
    float lineW = 0.06 + bass * 0.16 + beatC * 0.10;
    float edge = smoothstep(0.0, lineW, ringFrac) * smoothstep(0.0, lineW, 1.0 - ringFrac);
    float frame = 1.0 - edge;

    // Per-ring spectral pulse — the band sweeping bass->treble into depth.
    float ringPulse = band_at(ringId);

    // Four corridor corner lines (|p.x| == |p.y|) converging to the portal —
    // the strongest "hallway" cue.
    float corner = 1.0 - smoothstep(0.0, 0.05, abs(abs(p.x) - abs(p.y)));

    // RADIAL SPEED-LINES fanning OUT of the vanishing point: thin spokes at
    // SPOKES angles, invisible at the portal, fading in just outside it and
    // streaming toward the mouth. They rotate very slowly and shimmer hard
    // with treble + the beat, so they read as light rushing past.
    float ang = atan(p.y, p.x);
    float spokeWave = 0.5 + 0.5 * cos(ang * SPOKES + t * 0.25);
    float spoke = pow(spokeWave, 12.0)
        * smoothstep(0.02, 0.14, r)          // emanate from the vanishing point
        * (1.0 - smoothstep(0.85, 1.3, r));  // ease off at the near mouth

    // Depth shading: black square portal at the center, lit corridor outward.
    float depthShade = smoothstep(0.04, 0.55, r);
    float nearWeight = smoothstep(0.35, 1.1, r);

    // Palette: rings recede primary (near mouth) -> secondary -> accent (far).
    float palT = 1.0 - smoothstep(0.05, 0.6, r);
    vec3 ringCol = mix(primary.rgb, secondary.rgb, smoothstep(0.0, 0.5, palT));
    ringCol = mix(ringCol, accent.rgb, smoothstep(0.5, 1.0, palT));

    // Walls: lit gray gradient distinguishing left/right from top/bottom.
    float wallLit = (abs(p.x) > abs(p.y)) ? 0.16 : 0.10;
    vec3 col = vec3(wallLit) * depthShade * (0.6 + mid * 0.8);

    // Ring frames (palette + spectral pulse + bass) and the corner lines.
    float ringBright = frame * depthShade * (0.45 + ringPulse * 1.3 + bass * 0.5);
    col += ringCol * ringBright;
    col += accent.rgb * corner * depthShade * (0.35 + high * 0.7) * frame;

    // Radial speed-lines (toward `primary` so they read as bright streaks).
    col += primary.rgb * spoke * (0.5 + high * 1.4 + beatC * 0.8);

    // Beat punch + a treble rim sparkle on the nearest ring (toward accent).
    col *= 1.0 + beatC * 0.6;
    col += accent.rgb * high * nearWeight * frame * 0.25;

    col = clamp(col, vec3(0.0), vec3(1.0));
    // Premultiplied, like every other effect in this tree (blending: false at
    // the mount, so qt_Opacity is 1 and this equals the reference frame).
    fragColor = vec4(col * qt_Opacity, qt_Opacity);
}
