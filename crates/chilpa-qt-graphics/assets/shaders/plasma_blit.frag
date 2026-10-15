// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
#version 440

// Plasma (immersive scene, mode 1) — BLIT stage: copy the just-rendered
// history texture into the item's render target. The feedback pass renders
// into an owned ping-pong texture (never into the item target directly),
// so the item never has to be read back — this 1:1 textured triangle is the
// composite step. Same vertex stage as plasma.vert.
//
// QSB-SKIP-GLES100 — paired with the plasma passes (the tier gate keeps the
// scene off ES 2.0 hardware; a blit this small needs no ES 2.0 variant).

layout(binding = 1) uniform sampler2D src_tex;

layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 fragColor;

void main() {
    fragColor = texture(src_tex, uv);
}
