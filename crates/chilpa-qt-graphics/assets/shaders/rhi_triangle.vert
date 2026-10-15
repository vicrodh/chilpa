// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
#version 440

// Shared fullscreen-triangle VERTEX stage for the QQuickRhiItem scenes
// (Plasma A2: the feedback AND blit passes; Spectral Ribbon A3). The Slint
// references derive the triangle from @builtin(vertex_index)
// (plasma.wgsl:42-53, spectral_ribbon.wgsl:44-51); GLES 3.0 has no
// gl_VertexID, so the C++ side uploads the three clip positions as a vertex
// buffer (the linebed.vert idiom).
//
// QSB-SKIP-BATCH — a QQuickRhiItem shader, NOT a Qt Quick ShaderEffect
// stage: the `-b` batching rewrite is only valid for scene-graph batched
// items and would corrupt the custom attribute layout.

layout(location = 0) in vec2 pos;   // clip-space fullscreen triangle

layout(location = 0) out vec2 uv;

out gl_PerVertex { vec4 gl_Position; };

void main() {
    // plasma.wgsl:51 — uv = p * 0.5 + 0.5. Self-consistent for every pass
    // that renders and samples with this convention.
    uv = pos * 0.5 + vec2(0.5);
    gl_Position = vec4(pos, 0.0, 1.0);
}
