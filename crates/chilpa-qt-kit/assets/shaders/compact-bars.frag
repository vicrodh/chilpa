#version 440
// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.

    layout(location = 0) in vec2 qt_TexCoord0;
    layout(location = 0) out vec4 fragColor;

    layout(std140, binding = 0) uniform buf {
        mat4 qt_Matrix;
        vec4 topColor;
        vec4 bottomColor;
        float qt_Opacity;
        float bandW;
        float bandH;
        float b0;
        float b1;
        float b2;
        float b3;
        float b4;
        float b5;
        float b6;
        float b7;
        float b8;
        float b9;
        float b10;
        float b11;
        float b12;
        float b13;
    };

    float amp(int i) {
        if (i == 0) return b0;
        if (i == 1) return b1;
        if (i == 2) return b2;
        if (i == 3) return b3;
        if (i == 4) return b4;
        if (i == 5) return b5;
        if (i == 6) return b6;
        if (i == 7) return b7;
        if (i == 8) return b8;
        if (i == 9) return b9;
        if (i == 10) return b10;
        if (i == 11) return b11;
        if (i == 12) return b12;
        return b13;
    }

    float rrectAlpha(vec2 p, float w, float h, float r) {
        float rr = min(r, 0.5 * min(w, h));
        vec2 q = abs(p - 0.5 * vec2(w, h)) - 0.5 * vec2(w, h) + rr;
        float d = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - rr;
        return 1.0 - smoothstep(-0.5, 0.5, d);
    }

    void main() {
        vec2 px = qt_TexCoord0 * vec2(bandW, bandH);
        float slot = bandW / 28.0;
        int col = int(clamp(floor(px.x / slot), 0.0, 27.0));
        int m = col < 14 ? col : 27 - col;
        float h = max(2.0, amp(m) * bandH);
        float w = slot - 2.0;
        vec2 lp = vec2(px.x - (float(col) * slot + 1.0), px.y - (bandH - h));
        float shape = rrectAlpha(lp, w, h, 1.0);
        if (shape <= 0.0) {
            fragColor = vec4(0.0);
            return;
        }
        vec4 g = mix(topColor, bottomColor, clamp(lp.y / h, 0.0, 1.0));
        float a = g.a * shape * qt_Opacity;
        fragColor = vec4(g.rgb * a, a);
    }
