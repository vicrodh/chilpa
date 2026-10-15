// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
#version 440

// Tunnel Flow (immersive shader scene, mode 8 — Qt-only) — FRAGMENT stage.
// Block B1 of the 2026-08-15 immersive-completion contract (spec
// 02-tauri-tunnel-port.md). A REWRITE of the legacy Tauri Canvas2D panel
// (qbz-worktrees/legacy-tauri .../panels/TunnelFlowPanel.svelte) as a
// feedback fragment shader: 18 warped square ring shells traveling outward,
// feedback trails (history sampled with the inverse zoom/rotation/drift
// transform at alpha ~0.74, then the near-black fade fill), the background
// wash, the portal (black hole + throat ring + halo), and the corner
// vignette. The feedback history lives in two RGBA8 ping-pong textures owned
// by the C++ TunnelFlowItem (cxx/tunnelflow_item.* — the PlasmaItem shape).
//
// Mechanical differences from the Canvas2D source, enumerated:
//   * Vector polygon fills/strokes become PER-PIXEL ANALYTIC evaluation: each
//     ring is a square SHELL measured with the Chebyshev distance
//     (max(|x|/hx, |y|/hy)) to the warped square. The per-corner jitter of
//     makeWarpedSquare is kept as four scalars per square and interpolated
//     piecewise-linearly along the perimeter toward the pixel (the side the
//     pixel direction hits, between that side's two corners); the boundary
//     moves by 0.64*jitter — the outward-normal component of the source's
//     (sx*0.6+sy*0.22, sy*0.6-sx*0.22) diagonal displacement.
//   * Canvas2D blend modes become explicit color math: `screen` is
//     mix(dst, 1-(1-dst)(1-src), srcAlpha) per layer, `multiply` is
//     dst *= 1-alpha (black source), and `soft-light` (the diagonal haze) is
//     the W3C per-pixel soft-light formula composited with the source alpha
//     (spec 02 §3 sanctions "the standard soft-light formula applied
//     per-pixel" — this is that choice).
//   * The strut sort (rings by perspective) needs no sort: perspective is
//     monotonic in travel, so the sorted order is a MODULAR ROTATION of the
//     ring indices (sorted rank j = ringIndex (i0+j) % 18, i0 = the ring
//     whose travel just wrapped). The gate/span/lane/start formulas are
//     ported verbatim — the "simplified deterministic pick" spec 02 §3
//     allows, with zero visual approximation beyond the sort itself.
//   * drawSpeedMarks is dead code in the source (STREAK_COUNT = 0) and is
//     NOT ported (spec 02 §3).
//   * The source rendered at 0.58-0.7x CSS size and was upscaled; Qt renders
//     at physical size (x DPR, capped 2560x1440 — the plasma_item pattern).
//     Stroke widths are multiplied by WIDTH_COMP (1.56 ~ mean 1/scale) so
//     the lines keep their on-screen weight; every other constant is
//     verbatim.
//   * Canvas y is DOWN, texture uv y is UP (uv = pos*0.5+0.5, the plasma
//     convention; the GL composite flip is mirrorVertically on the item).
//     All motion terms are symmetric sinusoids, so only fixed asymmetries
//     would differ — there are none that read as orientation.
//   * The 36x36-artwork palette arrives as four vec4 uniforms (Rust port of
//     extractLinePaletteFromArtwork, src/tunnelflow_qt.rs); the audio
//     smoothing/kick/phase accumulators run in TunnelFlowScene.qml on the
//     pulse edge (the pulse law — nothing ticks off-pulse).
//
// No uint arithmetic -> the full qsb variant bake (no QSB-SKIP-GLES100).

layout(std140, binding = 0) uniform buf {
    float time;        // seconds (the shader converts to the source's ms)
    float phase;       // the TAURI phase accumulator (TunnelFlowScene.qml)
    float bass;        // mean of smoothed Viz16 bands 0..3
    float mid;         // bands 4..9
    float high;        // bands 10..15
    float kick;        // kickPulse (decays 0.9/pulse)
    vec2 resolution;   // history target px (capped 2560x1440)
    vec4 palette0;
    vec4 palette1;
    vec4 palette2;
    vec4 palette3;
} u;

layout(binding = 1) uniform sampler2D prev_tex;

layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 fragColor;

// Stroke-width compensation for the source's 0.58-0.7 render scale (see the
// header): 1/0.7 .. 1/0.58 = 1.43..1.72, midpoint ~1.56.
const float WIDTH_COMP = 1.56;
// (e^3.2 - 1) denominator of the spacing curve.
const float SPACING_DENOM = 23.5325301971; // exp(3.2) - 1
const int RING_COUNT = 18;

// Frame-scope values the ring/portal helpers share (set once in main).
float gT;       // time, MILLISECONDS (the source's timeMs domain)
float gPhase;
float gBass, gMid, gHigh, gKick;
float gMinDim;
vec2 gTC;       // wandering tunnel center, px
vec2 gTScale;   // tunnelScaleX/Y (the elliptical cross-section)
vec2 gPx;       // the pixel being shaded, px

vec4 pal(int i) {
    int k = i - (i / 4) * 4; // i % 4 without the % int pitfalls
    if (k == 0) return u.palette0;
    if (k == 1) return u.palette1;
    if (k == 2) return u.palette2;
    return u.palette3;
}

float clamp01(float x) { return clamp(x, 0.0, 1.0); }

// Canvas2D `screen` with source alpha (dst is opaque).
vec3 screenOver(vec3 dst, vec3 src, float a) {
    return mix(dst, vec3(1.0) - (vec3(1.0) - dst) * (vec3(1.0) - src), clamp(a, 0.0, 1.0));
}

// W3C soft-light, one channel.
float softLightCh(float s, float d) {
    if (s <= 0.5)
        return d - (1.0 - 2.0 * s) * d * (1.0 - d);
    float g = d <= 0.25 ? ((16.0 * d - 12.0) * d + 4.0) * d : sqrt(d);
    return d + (2.0 * s - 1.0) * (g - d);
}
vec3 softLight(vec3 src, vec3 dst) {
    return vec3(softLightCh(src.r, dst.r), softLightCh(src.g, dst.g), softLightCh(src.b, dst.b));
}

// makeWarpedSquare's per-corner jitter scalar (verbatim waves).
float cornerJitter(float halfSize, float warp, float seed, int k) {
    float fk = float(k);
    float waveA = sin(gT * 0.0015 + seed * 0.9 + fk * 1.7);
    float waveB = cos(gT * 0.0012 - seed * 0.7 + fk * 1.3);
    return halfSize * warp * (waveA * 0.6 + waveB * 0.4);
}

// Warped-square corner position (verbatim displacement).
vec2 cornerPos(vec2 c, float halfSize, float jitter, int k) {
    vec2 s = vec2((k == 0 || k == 3) ? -1.0 : 1.0, (k < 2) ? -1.0 : 1.0);
    return c + vec2(s.x * halfSize * gTScale.x + (s.x * 0.6 + s.y * 0.22) * jitter * gTScale.x,
                    s.y * halfSize * gTScale.y + (s.y * 0.6 - s.x * 0.22) * jitter * gTScale.y);
}

// Piecewise-linear interpolation of the four corner jitters along the
// perimeter, toward the pixel direction nq (square-normalized coords).
float perimJitter(vec2 nq, float j0, float j1, float j2, float j3) {
    if (abs(nq.x) >= abs(nq.y)) {
        float s = clamp(nq.y / max(abs(nq.x), 1e-5), -1.0, 1.0);
        return nq.x > 0.0 ? mix(j1, j2, s * 0.5 + 0.5) : mix(j0, j3, s * 0.5 + 0.5);
    }
    float s = clamp(nq.x / max(abs(nq.y), 1e-5), -1.0, 1.0);
    return nq.y > 0.0 ? mix(j3, j2, s * 0.5 + 0.5) : mix(j0, j1, s * 0.5 + 0.5);
}

float segDist(vec2 p, vec2 a, vec2 b) {
    vec2 pa = p - a;
    vec2 ba = b - a;
    float h = clamp(dot(pa, ba) / max(dot(ba, ba), 1e-6), 0.0, 1.0);
    return length(pa - ba * h);
}

// Antialiased stroke coverage: width w px centered on the path.
float strokeCov(float d, float w) {
    float aa = max(fwidth(d), 0.6);
    return 1.0 - smoothstep(w * 0.5 - aa, w * 0.5 + aa, d);
}

// Per-ring derived geometry (drawTunnelLayers, verbatim constants).
struct Ring {
    vec2 c;          // bent center, px
    float outerHalf;
    float innerHalf;
    float warpO;     // outer warp
    float warpI;     // inner warp (x0.72)
    float fade;
    float travel;
    float spacing;
};

Ring ringAt(int i, float travelRate) {
    float fi = float(i);
    float travel = fract(fi / float(RING_COUNT) + travelRate);
    float persp = pow(travel, 1.42);
    float spacing = (exp(persp * 3.2) - 1.0) / SPACING_DENOM;
    float appear = clamp01((persp - 0.18) / 0.26);
    float vanish = 1.0 - clamp01((persp - 0.9) / 0.1);
    Ring r;
    r.fade = pow(max(appear * vanish, 0.0), 0.82);
    r.outerHalf = gMinDim * (0.084 + spacing * 0.92);
    float thickness = gMinDim * (0.005 + (1.0 - spacing) * (0.028 + gBass * 0.016) + gKick * 0.0036);
    r.innerHalf = max(gMinDim * 0.028, r.outerHalf - thickness);
    r.warpO = 0.0025 + (1.0 - spacing) * 0.013 + gHigh * 0.004;
    r.warpI = r.warpO * 0.72;
    float bendStrength = gMinDim * (0.018 + gHigh * 0.01 + gKick * 0.008) * (1.0 - spacing);
    r.c = gTC + vec2(sin(gT * 0.0011 + fi * 0.31 + gPhase * 0.12) * bendStrength,
                     cos(gT * 0.00086 + fi * 0.24 + gPhase * 0.1) * bendStrength * 0.66);
    r.travel = travel;
    r.spacing = spacing;
    return r;
}

void main() {
    vec2 res = u.resolution;
    float W = res.x;
    float minDim = min(res.x, res.y);
    float aspect = res.x / max(res.y, 1.0);

    gT = u.time * 1000.0;
    gPhase = u.phase;
    gBass = u.bass;
    gMid = u.mid;
    gHigh = u.high;
    gKick = u.kick;
    gMinDim = minDim;
    gPx = uv * res; // texture space, y-up (see the header)

    float tunnelScaleX = clamp(1.04 + aspect * 0.25, 1.22, 1.58);
    float tunnelScaleY = clamp(1.38 - tunnelScaleX * 0.45, 0.62, 0.82);
    gTScale = vec2(tunnelScaleX, tunnelScaleY);

    // --- Tunnel center wander (render(), verbatim) -------------------------
    vec2 canvasCenter = res * 0.5;
    float curvePhase = gT * 0.0006 + gPhase * 0.14;
    gTC = vec2(
        canvasCenter.x + sin(curvePhase) * (W * (0.14 + gHigh * 0.05 + gKick * 0.04))
            + sin(curvePhase * 2.08 + 1.2) * W * 0.042,
        canvasCenter.y + cos(curvePhase * 0.82 + 0.7) * (minDim * (0.036 + gBass * 0.018 + gKick * 0.014))
            + cos(curvePhase * 1.7 + 0.4) * minDim * 0.012);
    float lateralCurve = clamp((gTC.x - canvasCenter.x) / max(1.0, W * 0.25), -1.0, 1.0);

    // --- 1. Feedback (drawFeedback) -----------------------------------------
    // The source draws the previous frame with
    //   M = T(tc) * R(a) * S(zoom) * T(-tc + drift)
    // so destination pixel p shows the history at M^-1(p) =
    //   tc - drift + R(-a) * (p - tc) / zoom.
    float fbAngle = sin(gT * 0.0011) * 0.0022 + (gHigh - gBass) * 0.003;
    float zoom = 1.004 + gBass * 0.014 + gHigh * 0.01 + gKick * 0.011;
    float driftRadius = minDim * (0.006 + gBass * 0.016 + gHigh * 0.01 + gKick * 0.012);
    vec2 drift = vec2(cos(gT * 0.001 + gPhase * 0.18), sin(gT * 0.0012 + gPhase * 0.16)) * driftRadius;
    vec2 d = gPx - gTC;
    float ca = cos(fbAngle);
    float sa = sin(fbAngle);
    vec2 rd = vec2(d.x * ca + d.y * sa, -d.x * sa + d.y * ca) / zoom; // R(-a)*d
    vec2 histUv = (gTC - drift + rd) / res;
    vec3 col = texture(prev_tex, histUv).rgb * (0.74 + gKick * 0.08);

    // --- 2. Dark fade fill: rgba(3,0,5, 0.28+high*0.08), source-over --------
    float fadeA = 0.28 + gHigh * 0.08;
    col = col * (1.0 - fadeA) + vec3(3.0, 0.0, 5.0) / 255.0 * fadeA;

    // --- 3. Background wash (drawBackground) --------------------------------
    // Radial center wash, source-over.
    float washT = clamp01((length(gPx - gTC) - minDim * 0.06) / (minDim * 1.16 - minDim * 0.06));
    float washA;
    vec3 washC;
    float a0 = 0.008 + gBass * 0.008;
    float a1 = 0.006 + gHigh * 0.006;
    if (washT < 0.45) {
        float k = washT / 0.45;
        washC = mix(vec3(1.0), vec3(180.0 / 255.0), k);
        washA = mix(a0, a1, k);
    } else {
        float k = (washT - 0.45) / 0.55;
        washC = vec3(180.0 / 255.0) * (1.0 - k);
        washA = a1 * (1.0 - k);
    }
    col = mix(col, washC, washA);
    // Diagonal haze, soft-light (see the header for the formula choice).
    float hazeT = clamp01((gPx.x * res.x + gPx.y * res.y) / (res.x * res.x + res.y * res.y));
    float hazeA;
    vec3 hazeC;
    float h0 = 0.004 + gHigh * 0.005;
    float h1 = 0.005 + gBass * 0.007;
    if (hazeT < 0.5) {
        float k = hazeT * 2.0;
        hazeC = vec3(1.0 - k);
        hazeA = h0 * (1.0 - k);
    } else {
        float k = (hazeT - 0.5) * 2.0;
        hazeC = vec3(180.0 / 255.0) * k;
        hazeA = h1 * k;
    }
    col = mix(col, softLight(hazeC, col), hazeA);

    // --- 4. Rings (drawTunnelLayers, screen composite) ----------------------
    float travelRate = gPhase * (1.28 + gBass * 0.24 + gKick * 0.34);
    Ring prevRing;
    float prevJI[4];
    for (int i = 0; i < RING_COUNT; ++i) {
        Ring r = ringAt(i, travelRate);
        float jI[4];
        for (int k = 0; k < 4; ++k)
            jI[k] = cornerJitter(r.innerHalf, r.warpI, float(i) + 1.7, k);

        if (r.fade > 0.0008) {
            float jO[4];
            for (int k = 0; k < 4; ++k)
                jO[k] = cornerJitter(r.outerHalf, r.warpO, float(i) + 0.3, k);

            // Warped shell distances (Chebyshev + perimeter jitter).
            vec2 q = gPx - r.c;
            vec2 nqO = q / vec2(r.outerHalf * gTScale.x, r.outerHalf * gTScale.y);
            float jOP = perimJitter(nqO, jO[0], jO[1], jO[2], jO[3]);
            float chebO = max(abs(nqO.x), abs(nqO.y)) - 0.64 * jOP / r.outerHalf;
            vec2 nqI = q / vec2(r.innerHalf * gTScale.x, r.innerHalf * gTScale.y);
            float jIP = perimJitter(nqI, jI[0], jI[1], jI[2], jI[3]);
            float chebI = max(abs(nqI.x), abs(nqI.y)) - 0.64 * jIP / r.innerHalf;

            // Wall gradient quad (inner->outer): gray a*0.22 -> white a*1.16
            // at 44% -> transparent, masked to the shell.
            float rI = (r.innerHalf + 0.64 * jIP) / r.outerHalf;
            float gRing = (chebO - rI) / max(1.0 - rI, 1e-4);
            float alpha = r.fade * (0.2 + gBass * 0.08 + gKick * 0.06);
            float wallA;
            vec3 wallC;
            if (gRing < 0.44) {
                float k = clamp(gRing / 0.44, 0.0, 1.0);
                wallC = mix(vec3(210.0 / 255.0), vec3(1.0), k);
                wallA = mix(alpha * 0.22, min(alpha * 1.16, 1.0), k);
            } else {
                float k = clamp((gRing - 0.44) / 0.56, 0.0, 1.0);
                wallC = vec3(1.0);
                wallA = min(alpha * 1.16, 1.0) * (1.0 - k);
            }
            float aa = max(fwidth(gRing), 0.02);
            wallA *= smoothstep(0.0, aa, gRing);
            col = screenOver(col, wallC, wallA);

            // Sparse wall stains ((i + side) % 3 == 0), radial palette blobs.
            for (int side = 0; side < 4; ++side) {
                if ((i + side) - ((i + side) / 3) * 3 != 0)
                    continue;
                vec2 nrm = vec2(side == 1 ? 1.0 : (side == 3 ? -1.0 : 0.0),
                                side == 2 ? 1.0 : (side == 0 ? -1.0 : 0.0));
                vec2 tng = vec2(side == 0 ? 1.0 : (side == 2 ? -1.0 : 0.0),
                                side == 1 ? 1.0 : (side == 3 ? -1.0 : 0.0));
                float lane = sin(float(i) * 1.13 + float(side) * 1.7 + gT * 0.0012 + r.travel * 22.0);
                vec2 radial = vec2(r.outerHalf * gTScale.x, r.outerHalf * gTScale.y);
                vec2 bc = gTC + nrm * radial * 0.82 + tng * radial * 0.58 * lane;
                float blobSize = minDim * (0.008 + r.fade * (0.018 + gBass * 0.01));
                float bd = length(gPx - bc);
                float bt = clamp01((bd - blobSize * 0.14) / (blobSize * 0.94));
                vec3 bcC;
                float bcA;
                if (bt < 0.62) {
                    float k = bt / 0.62;
                    bcC = mix(pal(i + side).rgb, pal(i + side + 1).rgb, k);
                    bcA = mix(alpha * (0.56 + gMid * 0.18), alpha * (0.3 + gHigh * 0.12), k);
                } else {
                    float k = (bt - 0.62) / 0.38;
                    bcC = pal(i + side + 1).rgb;
                    bcA = alpha * (0.3 + gHigh * 0.12) * (1.0 - k);
                }
                float edge = 1.0 - smoothstep(blobSize - max(fwidth(bd), 0.6),
                                              blobSize + max(fwidth(bd), 0.6), bd);
                col = screenOver(col, bcC, bcA * edge);
            }

            // Palette-colored edge stroke on the inner square.
            float edgeAlpha = r.fade * (0.34 + gBass * 0.12 + gKick * 0.08);
            float edgeWidth = 0.86 + r.fade * (1.78 + gBass * 0.72 + gKick * 0.56);
            vec3 edgeColor = pal(i).rgb;
            float pxScale = abs(nqI.x) >= abs(nqI.y) ? r.innerHalf * gTScale.x
                                                     : r.innerHalf * gTScale.y;
            float edgeDist = abs(chebI - 1.0) * pxScale;
            col = screenOver(col, edgeColor,
                             edgeAlpha * strokeCov(edgeDist, edgeWidth * WIDTH_COMP));

            // Corner connectors to the previous ring's inner square (same
            // stroke style, width x0.88, min 0.74).
            if (i > 0) {
                float connW = max(0.74, edgeWidth * 0.88) * WIDTH_COMP;
                for (int k = 0; k < 4; ++k) {
                    vec2 pa = cornerPos(prevRing.c, prevRing.innerHalf, prevJI[k], k);
                    vec2 pb = cornerPos(r.c, r.innerHalf, jI[k], k);
                    col = screenOver(col, edgeColor,
                                     edgeAlpha * strokeCov(segDist(gPx, pa, pb), connW));
                }
            }
        }

        prevRing = r;
        for (int k = 0; k < 4; ++k)
            prevJI[k] = jI[k];
    }

    // --- Struts (the sorted cross-ring lines) -------------------------------
    // The sort is a modular rotation (see the header); all gate math verbatim.
    {
        float audioDrive = clamp01(gBass * 0.38 + gHigh * 0.28 + gKick * 1.04);
        int linesPerSide = 1 + int(floor(audioDrive * 1.6));
        float eventStepMs = max(92.0, 172.0 - audioDrive * 84.0);
        float bucket = floor(gT / eventStepMs);
        // i0: the ring with the smallest travel (sorted rank 0).
        float w = fract(travelRate) * float(RING_COUNT);
        int i0 = int(ceil(float(RING_COUNT) - w)) % RING_COUNT;
        for (int side = 0; side < 4; ++side) {
            int nextSide = (side + 1) % 4;
            for (int tr = 0; tr < 2; ++tr) {
                if (tr >= linesPerSide)
                    break;
                float seed = float(side) * 11.37 + float(tr) * 3.83 + bucket * 0.41;
                float gate = (sin(bucket * 0.47 + gPhase * 2.2 + seed) + 1.0) * 0.5;
                if (gate < 0.6 - audioDrive * 0.32)
                    continue;
                int span = gate > 0.76 ? 3 : 2;
                int maxStart = RING_COUNT - span - 1;
                float startSel = (sin(gT * 0.0014 + gPhase * 1.8 + seed * 0.9) + 1.0) * 0.5;
                int startIndex = min(maxStart, int(floor(startSel * float(maxStart + 1))));
                int endIndex = min(RING_COUNT - 1, startIndex + span);
                float laneSel = clamp01((sin(bucket * 0.73 + seed * 1.3) + 1.0) * 0.5);
                float lane = laneSel < 0.333333 ? 0.2 : (laneSel < 0.666667 ? 0.5 : 0.8);
                Ring rs = ringAt((i0 + startIndex) % RING_COUNT, travelRate);
                Ring re = ringAt((i0 + endIndex) % RING_COUNT, travelRate);
                float lineFade = clamp01((rs.fade + re.fade) * 0.7);
                vec2 a0 = cornerPos(rs.c, rs.innerHalf,
                                    cornerJitter(rs.innerHalf, rs.warpI,
                                                 float((i0 + startIndex) % RING_COUNT) + 1.7, side),
                                    side);
                vec2 b0 = cornerPos(rs.c, rs.innerHalf,
                                    cornerJitter(rs.innerHalf, rs.warpI,
                                                 float((i0 + startIndex) % RING_COUNT) + 1.7, nextSide),
                                    nextSide);
                vec2 a1 = cornerPos(re.c, re.innerHalf,
                                    cornerJitter(re.innerHalf, re.warpI,
                                                 float((i0 + endIndex) % RING_COUNT) + 1.7, side),
                                    side);
                vec2 b1 = cornerPos(re.c, re.innerHalf,
                                    cornerJitter(re.innerHalf, re.warpI,
                                                 float((i0 + endIndex) % RING_COUNT) + 1.7, nextSide),
                                    nextSide);
                vec2 p0 = mix(a0, b0, lane);
                vec2 p1 = mix(a1, b1, lane);
                float strutAlpha = (0.32 + audioDrive * 0.34) * lineFade;
                col = screenOver(col, pal(side * 3 + tr + startIndex).rgb,
                                 strutAlpha * strokeCov(segDist(gPx, p0, p1),
                                                        (1.5 + audioDrive * 1.4) * WIDTH_COMP));
            }
        }
    }

    // --- 5. Portal (drawPortal) ----------------------------------------------
    {
        float portalHalf = minDim * 0.057;
        float throatOuterHalf = portalHalf * 1.58;
        float warpO = 0.008 + gHigh * 0.006;
        float warpI = 0.004 + gHigh * 0.004;
        // Throat ring (screen), gradient white a -> gray a*0.24 at 50% -> 0.
        float jO0 = cornerJitter(throatOuterHalf, warpO, 941.0, 0);
        float jO1 = cornerJitter(throatOuterHalf, warpO, 941.0, 1);
        float jO2 = cornerJitter(throatOuterHalf, warpO, 941.0, 2);
        float jO3 = cornerJitter(throatOuterHalf, warpO, 941.0, 3);
        float jI0 = cornerJitter(portalHalf, warpI, 942.0, 0);
        float jI1 = cornerJitter(portalHalf, warpI, 942.0, 1);
        float jI2 = cornerJitter(portalHalf, warpI, 942.0, 2);
        float jI3 = cornerJitter(portalHalf, warpI, 942.0, 3);
        vec2 q = gPx - gTC;
        vec2 nqO = q / vec2(throatOuterHalf * gTScale.x, throatOuterHalf * gTScale.y);
        float chebO = max(abs(nqO.x), abs(nqO.y))
            - 0.64 * perimJitter(nqO, jO0, jO1, jO2, jO3) / throatOuterHalf;
        vec2 nqI = q / vec2(portalHalf * gTScale.x, portalHalf * gTScale.y);
        float jIP = perimJitter(nqI, jI0, jI1, jI2, jI3);
        float chebI = max(abs(nqI.x), abs(nqI.y)) - 0.64 * jIP / portalHalf;
        float sideAlpha = 0.032 + gHigh * 0.015;
        float rI = portalHalf / throatOuterHalf;
        float gT2 = (chebO - rI) / (1.0 - rI);
        float throatA;
        vec3 throatC;
        if (gT2 < 0.5) {
            float k = clamp(gT2 * 2.0, 0.0, 1.0);
            throatC = mix(vec3(1.0), vec3(185.0 / 255.0), k);
            throatA = mix(sideAlpha, sideAlpha * 0.24, k);
        } else {
            float k = clamp((gT2 - 0.5) * 2.0, 0.0, 1.0);
            throatC = vec3(185.0 / 255.0);
            throatA = sideAlpha * 0.24 * (1.0 - k);
        }
        throatA *= smoothstep(0.0, max(fwidth(gT2), 0.02), gT2);
        col = screenOver(col, throatC, throatA);

        // The black hole (0.82x the inner square), source-over a=0.98.
        float holeDist = (chebI - 0.82) * min(portalHalf * gTScale.x, portalHalf * gTScale.y);
        float holeCov = 1.0 - smoothstep(-max(fwidth(holeDist), 0.6), max(fwidth(holeDist), 0.6), holeDist);
        col = mix(col, vec3(0.0), 0.98 * holeCov);

        // Multiply dark halo out to 4.8x portal (radial, circular).
        float haloT = clamp01((length(q) - portalHalf * 0.6) / (portalHalf * 4.8 - portalHalf * 0.6));
        float haloA;
        if (haloT < 0.32) {
            haloA = mix(0.72, 0.48, haloT / 0.32);
        } else if (haloT < 0.72) {
            haloA = mix(0.48, 0.18, (haloT - 0.32) / 0.4);
        } else {
            haloA = 0.18 * (1.0 - (haloT - 0.72) / 0.28);
        }
        col *= 1.0 - haloA;

        // Directional front shadow on the curve-leading side (multiply).
        float portalEdgeWidth = 0.96 + gBass * 0.26;
        float lateralAbs = abs(lateralCurve);
        if (lateralAbs > 0.06) {
            float shadowStrength = clamp01((lateralAbs - 0.06) / 0.58);
            // The curve-leading SIDE of the portal band: x-dominant (for the
            // square, the side region is |x| >= |y|) on the sign the tunnel
            // center wanders toward.
            bool lead = abs(nqI.x) >= abs(nqI.y) && nqI.x * sign(lateralCurve) > 0.0;
            float gSh = clamp01((chebI - 0.82) / 0.18);
            float shA;
            if (gSh < 0.72) {
                shA = mix(0.56 + shadowStrength * 0.22, 0.22 + shadowStrength * 0.18, gSh / 0.72);
            } else {
                shA = (0.22 + shadowStrength * 0.18) * (1.0 - (gSh - 0.72) / 0.28);
            }
            float inBand = smoothstep(0.0, max(fwidth(gSh), 0.02), gSh)
                * (1.0 - smoothstep(1.0 - max(fwidth(gSh), 0.02), 1.0, gSh));
            col *= 1.0 - shA * (lead ? inBand : 0.0);
            // Dark stroke on the hole edge, leading side only.
            float holeEdge = abs(chebI - 0.82) * min(portalHalf * gTScale.x, portalHalf * gTScale.y);
            col *= 1.0 - (0.42 + shadowStrength * 0.22)
                * (lead ? strokeCov(holeEdge, max(1.0, portalEdgeWidth * 1.2) * WIDTH_COMP) : 0.0);
        }

        // Portal edge highlight + hole rim (source-over, palette0).
        float pxScale = abs(nqI.x) >= abs(nqI.y) ? portalHalf * gTScale.x
                                                 : portalHalf * gTScale.y;
        float edgeDist = abs(chebI - 1.0) * pxScale;
        col = mix(col, pal(0).rgb,
                  (0.18 + gHigh * 0.06) * strokeCov(edgeDist, portalEdgeWidth * WIDTH_COMP));
        float rimDist = abs(chebI - 0.82) * pxScale;
        col = mix(col, pal(0).rgb,
                  (0.14 + gHigh * 0.04) * strokeCov(rimDist, max(0.76, portalEdgeWidth * 0.8) * WIDTH_COMP));
    }

    // --- 6. Corner vignette (drawCornerVignette, multiply) -------------------
    for (int c = 0; c < 4; ++c) {
        vec2 corner = vec2((c == 1 || c == 3) ? res.x : 0.0, (c < 2) ? 0.0 : res.y);
        float vt = clamp01(length(gPx - corner) / (minDim * 0.86));
        float va;
        if (vt < 0.44) {
            va = mix(0.72, 0.38, vt / 0.44);
        } else {
            va = 0.38 * (1.0 - (vt - 0.44) / 0.56);
        }
        col *= 1.0 - va;
    }

    fragColor = vec4(col, 1.0);
}
