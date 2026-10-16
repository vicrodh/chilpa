// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
#version 440

// Plasma (immersive scene, mode 1) — FRAGMENT stage. Line-faithful GLSL 440
// port of `fs_main` in crates/qbz-ui/ui/shaders/plasma.wgsl (the MilkDrop-
// class feedback fluid: sample the previous frame at a warped UV, decay,
// inject orbiting ink + beat splats). The feedback history lives in two
// RGBA8 ping-pong textures owned by the C++ PlasmaItem (the reference keeps
// ONE history and copies the frame back — shader_underlay.rs:1003-1025 —
// which is the same recurrence; ping-pong is how you say "sample last frame,
// write this frame" without a surface readback).
//
// Mechanical differences from the WGSL, enumerated:
//   * textureSample(prev_tex, prev_samp, uv) -> texture(prev_tex, uv)
//     (combined image sampler, binding 1).
//   * bitcast<u32>(i32(x)) -> uint(int(x)): int->uint is a modulo-2^32
//     conversion in GLSL, i.e. the same bit pattern the WGSL bitcast keeps.
//   * The uniform block is the FULL 144-byte std140 scene block (spec 01
//     §1) — this scene reads time/resolution/energy_lo/energy_hi.x/beat/
//     level/level_smooth/primary/secondary/accent; the remaining fields
//     arrive zeroed.
//
// QSB-SKIP-GLES100 — the lattice hash is UINT arithmetic (plasma.wgsl:55-67
// records why a float hash draws "wallpaper join" seams); ES 2.0 has no
// uint. The tier gate keeps the scene off that hardware.

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

layout(binding = 1) uniform sampler2D prev_tex;

layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 fragColor;

// INTEGER lattice hash (bit-exact) — see the WGSL header above.
float hash2(vec2 p) {
    uint h = uint(int(p.x)) * 0x8da6b343u + uint(int(p.y)) * 0xd8163841u;
    h = (h ^ (h >> 15u)) * 0x2c1b3c6du;
    h = (h ^ (h >> 12u)) * 0x297a2d39u;
    h = h ^ (h >> 15u);
    return float(h >> 8u) * (1.0 / 16777216.0);
}

float vnoise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    float a = hash2(i);
    float b = hash2(i + vec2(1.0, 0.0));
    float c = hash2(i + vec2(0.0, 1.0));
    float d = hash2(i + vec2(1.0, 1.0));
    vec2 uf = f * f * (3.0 - 2.0 * f);
    return mix(mix(a, b, uf.x), mix(c, d, uf.x), uf.y);
}

// Divergence-free flow from a scalar potential: curl = (dphi/dy, -dphi/dx).
vec2 curl_noise(vec2 p) {
    float e = 0.05;
    float dx = vnoise(p + vec2(e, 0.0)) - vnoise(p - vec2(e, 0.0));
    float dy = vnoise(p + vec2(0.0, e)) - vnoise(p - vec2(0.0, e));
    return vec2(dy, -dx) / (2.0 * e);
}

// Rodrigues rotation about the grey axis — a hue turn that needs no RGB<->HSV
// round trip. The palette arrives as ONE album-derived triad, so without this
// every element in the scene is a shade of the same colour; turning a copy of
// it is how the worms and the beat arcs end up in different families whatever
// album is playing.
vec3 hueRot(vec3 c, float a) {
    const vec3 k = vec3(0.57735026);
    float ca = cos(a);
    float sa = sin(a);
    return c * ca + cross(k, c) * sa + k * dot(k, c) * (1.0 - ca);
}

// Soft gaussian blob — an ink emitter.
float blob(vec2 p, vec2 c, float radius) {
    vec2 d = p - c;
    return exp(-dot(d, d) / max(radius * radius, 1e-4));
}

// -- WHY THIS LOOKS THE WAY IT DOES (2026-08-19 rework) --------------------
//
// The reference is Butterchurn (MilkDrop in JS) on a percussion-heavy track,
// and the frame-by-frame gives the recipe in three parts:
//
//   1. CONCENTRIC RINGS. They are not drawn as rings. ONE thin annulus is
//      injected per frame, and the feedback warp marches it inward — so the
//      rings you see ARE the last N frames of audio, laid out in space. Time
//      becomes geometry. That only works if a trail survives long enough to
//      stack, which is the second part.
//   2. HIGH RETENTION. MilkDrop presets hold ~0.97+ per frame. This shader
//      held 0.85: a trail died in ~15 frames, so nothing ever accumulated and
//      four soft blobs orbited an empty field — "a Windows 98 wallpaper",
//      accurately.
//   3. ANGULAR STRUCTURE. The spokes come from reading the spectrum BY ANGLE,
//      so each direction carries its own band, and from folding the plane into
//      mirrored sectors.
//
// Deliberately NOT here: a preset engine, per-preset equations, custom
// shapes/waves. This is Plasma made intense, not MilkDrop reimplemented.

// The spectrum sampled AT AN ANGLE (0..1 around the circle), interpolated
// between the 8 bands so a spoke sweeps rather than steps. This is what turns
// a flat field into radial structure.
float bandAt(float a01, vec4 lo, vec4 hi) {
    float x = fract(a01) * 8.0;
    int i = int(x) % 8;
    int j = (i + 1) % 8;
    float f = fract(x);
    float b[8] = float[8](lo.x, lo.y, lo.z, lo.w, hi.x, hi.y, hi.z, hi.w);
    return mix(b[i], b[j], f * f * (3.0 - 2.0 * f));
}

void main() {
    float t = u.time;
    float aspect = u.resolution.x / max(u.resolution.y, 1.0);

    // SLOW (energy, x0.85/frame ~ 473 ms) drives STRUCTURE; FAST (bands,
    // x0.65 ~ 178 ms) drives MOTION. The bands used to arrive zeroed — they
    // were published and parsed but never handed to this item — which is why
    // everything ran off the slowest signals in the pipeline.
    float sub   = clamp(u.energyLo.x, 0.0, 1.0);
    float bass  = clamp(u.energyLo.y, 0.0, 1.0);
    float mids  = clamp(u.energyLo.z, 0.0, 1.0);
    float pres  = clamp(u.energyLo.w, 0.0, 1.0);
    float air   = clamp(u.energyHi.x, 0.0, 1.0);

    vec4 fLo = clamp(u.bandsLo, vec4(0.0), vec4(1.0));
    vec4 fHi = clamp(u.bandsHi, vec4(0.0), vec4(1.0));
    // Sharpened copies: squaring drops the quiet floor without touching the
    // peaks, so a dense mix reads as PEAKS instead of a lit plateau.
    vec4 sLo = fLo * fLo;
    vec4 sHi = fHi * fHi;

    // `beat` is the AC-COUPLED onset (PlasmaScene.qml): 0 at the local density
    // floor, 1 on a hit above it, so it still reads as a hit at any tempo.
    // `transientAmp` is the raw, shorter spike — the crack over the punch.
    float beat  = clamp(u.beat, 0.0, 1.0);
    float hit   = clamp(u.transientAmp, 0.0, 1.0);
    float level = clamp(u.level, 0.0, 1.0);
    float lvl   = clamp(u.levelSmooth, 0.0, 1.0);

    vec2 c = (uv - vec2(0.5)) * vec2(aspect, 1.0);
    float r = length(c);
    float ang = atan(c.y, c.x);

    // TWO COLOUR FAMILIES out of the one album triad. The worms keep the
    // palette as published; the beat arcs are turned ~130 degrees off it and
    // the ambient wash sits opposite both. One triad rendered everything in a
    // single hue before this — the scene was a teal disc with a purple hairline
    // and nothing else.
    vec3 wormA = u.primary.rgb;
    vec3 wormB = u.secondary.rgb;
    vec3 arcA  = hueRot(u.accent.rgb, 2.27);
    vec3 arcB  = hueRot(u.secondary.rgb, 1.85);
    vec3 ambA  = hueRot(u.primary.rgb, 3.9);

    // === Feedback advection — the ring conveyor ============================
    // A firm inward march: this is what turns one injected annulus per frame
    // into a stack of concentric rings. Bass widens the step (the tunnel
    // breathes) and an onset shoves it.
    float zoom = 1.018 + (sub + bass) * 0.020 + beat * 0.030 + hit * 0.014;
    // Rotation on the FAST mid — the term that follows a line instead of
    // averaging it — plus a radius-dependent shear so the rings twist into
    // spirals rather than sitting concentric and dead.
    float rot = (0.004 + mids * 0.020 + sLo.w * 0.045) * sin(t * 0.4 + r * 5.0)
              + beat * 0.05;
    float ca = cos(rot);
    float sa = sin(rot);
    vec2 w = c * zoom;
    w = vec2(w.x * ca - w.y * sa, w.x * sa + w.y * ca);
    // Curl octaves keep the rings from being perfect circles — the lattice
    // texture in the reference. Coarse follows fast presence, fine follows
    // fast brilliance, so filaments appear ON the attack.
    w += curl_noise(c * 2.5 + vec2(t * 0.12, -t * 0.10)) * (0.002 + sHi.y * 0.014);
    w += curl_noise(c * 6.5 - vec2(t * 0.09, t * 0.08)) * (0.001 + sHi.z * 0.010);
    vec2 warpUv = w / vec2(aspect, 1.0) + vec2(0.5);

    vec3 field = texture(prev_tex, warpUv).rgb;

    // RETENTION — the single biggest change. 0.85 -> 0.945..0.975. Below ~0.93
    // nothing stacks and there is no structure to look at; the loud end stays
    // short of 1.0 so the field always eventually forgets.
    field *= 0.945 + lvl * 0.030;

    // === Mirrored sectors — the kaleidoscope ===============================
    // Fold the angle into N mirrored wedges. The count rides the SLOW mids so
    // it changes with the section instead of flickering per frame.
    float sectors = floor(5.0 + mids * 5.0);
    float aFold = abs(fract(ang / 6.2831853 * sectors) * 2.0 - 1.0);

    // === BEAT ARCS — not a circumference ===================================
    // The Tauri reference this borrows from was BEAT DETECTION, and a closed
    // ring is not that: a perfect inner circumference reads as a drawn shape,
    // hard-edges the field, and at this retention the successive rings packed
    // into one solid disc — which is exactly what killed the ambient feel.
    //
    // So the annulus is GATED BY ANGLE. Only the sectors whose gate opens emit,
    // so a hit paints a few arcs that the conveyor then carries inward; the
    // circle never closes and the space between arcs stays open for the
    // background to show through. The gate threshold falls as the onset rises,
    // so a big hit opens more of the circle and a quiet passage barely any.
    float gateN = vnoise(vec2(ang * 2.9 - 3.0, t * 0.6));
    float gate = smoothstep(0.72 - beat * 0.34, 0.86 - beat * 0.30, gateN);

    float ringR = 0.20 + bass * 0.10 + beat * 0.18 + sub * 0.06;
    float spoke = bandAt(ang / 6.2831853 + 0.5 + t * 0.02, fLo, fHi);
    float sharp = spoke * spoke;

    // MADE OF PLASMA, NOT OF COMPASS: a slow wobble bends the arc off-round, a
    // fast one frays its edge, and the thickness breathes around the circle so
    // it is never a uniform band.
    float wobLo = vnoise(vec2(ang * 1.7, t * 0.30)) - 0.5;
    float wobHi = vnoise(vec2(ang * 6.5, t * 0.90)) - 0.5;
    ringR += wobLo * (0.045 + mids * 0.070) + wobHi * (0.010 + sHi.x * 0.024);
    float ringW = 0.005 + level * 0.008 + hit * 0.010;
    ringW *= 0.55 + vnoise(vec2(ang * 3.1 + 11.0, t * 0.55)) * 1.1;
    float ring = exp(-pow(r - ringR, 2.0) / max(ringW * ringW, 1e-5));
    // Injection is well down from the closed-ring version: it no longer has to
    // carry the scene, and at 0.945 retention a strong one silts up into a disc.
    field += mix(arcA, arcB, aFold) * ring * gate * (0.04 + sharp * 0.75 + beat * 0.65);

    // The outer filigree, gated the same way and on the high bands.
    float gate2 = smoothstep(0.66, 0.88, vnoise(vec2(ang * 4.7 + 9.0, t * 0.85)));
    float ring2R = 0.40 + air * 0.10 + sHi.w * 0.08
                 + (vnoise(vec2(ang * 3.3 + 21.0, t * 0.45)) - 0.5) * 0.060;
    float ring2 = exp(-pow(r - ring2R, 2.0) / 1.225e-5);
    field += arcB * ring2 * gate2 * (sHi.x * 0.8 + sHi.z * 0.6);

    // === THE WORMS — all four emitters, back at full strength ==============
    // The filaments are these blobs stretched by the curl advection over many
    // frames. The rework demoted them to faint accents and the rings then
    // buried them; they are the ORIGINAL character of this scene and they run
    // at full amplitude again. The higher retention actually makes their
    // trails longer than they ever were — the only thing that had gone wrong
    // was how little ink they were injecting.
    //
    // Each orbit angle carries a fast band term, so a worm accelerates on its
    // own voice instead of all four drifting at one rate.
    float p1 = t * 0.50 + sLo.z * 2.4;
    float p2 = -t * 0.42 + 2.1 - sLo.w * 2.0;
    float p3 = t * 0.74 + 1.0 + sHi.y * 2.8;
    float p4 = t * 0.33 + 4.0 + sHi.z * 3.0;
    // ORBITS REACH PAST THE ARCS on purpose. They used to sit inside the ring
    // radius, so the worms were trapped in the disc and never crossed its edge;
    // now they run from ~0.30 out to ~0.62, i.e. through the arc band and
    // beyond it, and the field outside the arcs stops being empty.
    vec2 e1 = vec2(sin(p1), cos(p1 * 1.2)) * (0.34 + fLo.y * 0.24);
    vec2 e2 = vec2(sin(p2), cos(p2 * 0.9)) * (0.44 + sLo.w * 0.20);
    vec2 e3 = vec2(sin(p3), cos(-p3 * 0.9 + 3.0)) * (0.30 + sHi.y * 0.26);
    vec2 e4 = vec2(cos(p4), sin(p4 * 1.75 + 0.5)) * (0.50 + sHi.z * 0.20);
    // DEFINITION: tighter cores and a smoothstep contrast curve. A raw gaussian
    // is all shoulder — it reads as haze at any brightness. Squeezing the
    // falloff and then pushing the mid-range gives an edge the advection can
    // stretch into an actual filament instead of a smear.
    float w1 = blob(c, e1, 0.022 + fLo.y * 0.024);
    float w2 = blob(c, e2, 0.024 + sLo.w * 0.022);
    float w3 = blob(c, e3, 0.018 + sHi.y * 0.020);
    float w4 = blob(c, e4, 0.015 + sHi.z * 0.018);
    w1 = smoothstep(0.06, 0.75, w1);
    w2 = smoothstep(0.06, 0.75, w2);
    w3 = smoothstep(0.05, 0.70, w3);
    w4 = smoothstep(0.05, 0.70, w4);
    field += wormA                     * w1 * (0.16 + sLo.x * 0.95);
    field += wormB                     * w2 * (0.16 + sLo.z * 0.90);
    field += hueRot(wormA, 0.9)        * w3 * (0.13 + sHi.x * 0.95);
    field += hueRot(wormB, -0.8)       * w4 * (0.11 + sHi.z * 0.95);

    // === Onset flash — a bright core the conveyor drags outward ============
    // Injected at the CENTRE on purpose: the inward warp then carries it as a
    // shrinking ring, so every hit leaves a visible wavefront.
    float b1 = 0.03 + beat * 0.09;
    float b2 = 0.014 + hit * 0.04;
    field += u.accent.rgb    * exp(-r * r / max(b1 * b1, 1e-5)) * beat * 1.9;
    field += u.secondary.rgb * exp(-r * r / max(b2 * b2, 1e-5)) * hit * 1.4;

    // === AMBIENT WASH — the background stops being black ===================
    // A slow, low-amplitude curl field across the WHOLE frame, in the hue
    // turned furthest from both foreground families. Two jobs: it gives the
    // arcs and worms something to read against (they were bright objects on
    // void, which is what made the disc look like a cut-out), and it fills the
    // space the arc gate deliberately leaves open. The owner wants the Plasma
    // field materially darker than the other scenes, so this wash stays around
    // half its original injection while the foreground arcs/worms retain their
    // full music-reactive range.
    vec2 aw = c * 1.6 + vec2(t * 0.045, -t * 0.038);
    float amb = vnoise(aw + curl_noise(aw) * 0.35);
    float vign = 1.0 - smoothstep(0.30, 0.95, r);
    field += ambA * (0.016 + lvl * 0.030) * (0.35 + amb * 0.65) * (0.25 + vign * 0.75);

    // Treble sparkle on the crests — fast brilliance, so it fires with the
    // cymbals instead of glowing with the average.
    float luma = dot(field, vec3(0.33, 0.34, 0.33));
    field += u.accent.rgb * (sHi.z * 0.26 + sHi.w * 0.18) * smoothstep(0.35, 0.9, luma);

    // SOFT LIMITER, and it is load-bearing at this retention. A hard clamp
    // against 0.945+ feedback pins the middle to white and the structure
    // disappears into a flat blob; this compresses instead, so bright areas
    // keep their gradient and the rings stay readable through a loud passage.
    field = field / (vec3(1.0) + field * 0.35);
    field = clamp(field, vec3(0.0), vec3(1.0));
    fragColor = vec4(field, 1.0);
}
