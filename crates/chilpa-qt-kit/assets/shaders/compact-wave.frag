#version 440
// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.

    layout(location = 0) in vec2 qt_TexCoord0;
    layout(location = 0) out vec4 fragColor;

    layout(std140, binding = 0) uniform buf {
        mat4 qt_Matrix;
        vec4 topColor;
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
        float b14;
        float b15;
        float b16;
        float b17;
        float b18;
        float b19;
        float b20;
        float b21;
        float b22;
        float b23;
        float b24;
        float b25;
        float b26;
        float b27;
        float b28;
        float b29;
        float b30;
        float b31;
        float b32;
        float b33;
        float b34;
        float b35;
        float b36;
        float b37;
        float b38;
        float b39;
        float b40;
        float b41;
        float b42;
        float b43;
        float b44;
        float b45;
        float b46;
        float b47;
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
        if (i == 13) return b13;
        if (i == 14) return b14;
        if (i == 15) return b15;
        if (i == 16) return b16;
        if (i == 17) return b17;
        if (i == 18) return b18;
        if (i == 19) return b19;
        if (i == 20) return b20;
        if (i == 21) return b21;
        if (i == 22) return b22;
        if (i == 23) return b23;
        if (i == 24) return b24;
        if (i == 25) return b25;
        if (i == 26) return b26;
        if (i == 27) return b27;
        if (i == 28) return b28;
        if (i == 29) return b29;
        if (i == 30) return b30;
        if (i == 31) return b31;
        if (i == 32) return b32;
        if (i == 33) return b33;
        if (i == 34) return b34;
        if (i == 35) return b35;
        if (i == 36) return b36;
        if (i == 37) return b37;
        if (i == 38) return b38;
        if (i == 39) return b39;
        if (i == 40) return b40;
        if (i == 41) return b41;
        if (i == 42) return b42;
        if (i == 43) return b43;
        if (i == 44) return b44;
        if (i == 45) return b45;
        if (i == 46) return b46;
        return b47;
    }

    float rrectAlpha(vec2 p, float w, float h, float r) {
        float rr = min(r, 0.5 * min(w, h));
        vec2 q = abs(p - 0.5 * vec2(w, h)) - 0.5 * vec2(w, h) + rr;
        float d = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - rr;
        return 1.0 - smoothstep(-0.5, 0.5, d);
    }

    void main() {
        vec2 px = qt_TexCoord0 * vec2(bandW, bandH);
        float slot = bandW / 48.0;
        int col = int(clamp(floor(px.x / slot), 0.0, 47.0));
        float h = max(2.0, amp(col) * bandH);
        float w = slot - 2.0;
        vec2 lp = vec2(px.x - (float(col) * slot + 1.0), px.y - (bandH - h) * 0.5);
        float shape = rrectAlpha(lp, w, h, 1.0);
        if (shape <= 0.0) {
            fragColor = vec4(0.0);
            return;
        }
        float a = topColor.a * shape * qt_Opacity;
        fragColor = vec4(topColor.rgb * a, a);
    }
