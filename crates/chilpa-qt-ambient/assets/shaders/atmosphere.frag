#version 440

// atmosphere.frag: the blurred-cover background in ONE full-window pass, as
// QBZ's Qt shell draws it. The four-Image build of the same picture is
// seven full-window nodes, six of them blended, re-executed on every
// present; since Qt Quick repaints the whole window on any dirty, every
// other animation paid those six passes too. The fragment math below is the
// exact same composite, so the background is one opaque draw.
//
// THE COMPOSITE, in the Image stack's order (all arithmetic in the same sRGB
// space the scene graph blends in):
//   c = #0a0a0b                                  (base Rectangle)
//   c = tex_i.rgb*o_i + c*(1 - a_i*o_i)  x4      (Image src-over, textures
//                                                 premultiplied; a_i = 1 for
//                                                 the opaque bitmap, where
//                                                 this is plain mix)
//   c *= 1 - dim                                 (black dim overlay)
//   c *= 1 - scrim(y)                            (black alpha gradient:
//                                                 .4 top -> 0 @35% -> ~.5
//                                                 bottom, linear in alpha)
// The output is opaque, so the item sets `blending: false` and lands in the
// opaque pass (front-to-back, early-z) instead of the alpha stack.
//
// PreserveAspectCrop of a SQUARE source into a layer rect R = (rw, rh) at
// origin o: cover scale s = max(rw, rh), the crop is centred, so a fragment
// at item-pixel px samples uv = (px - (o + R/2)) / s + 0.5. QML packs
// (cx, cy, s, opacity) per layer: AtmosphereField.qml packLayer().

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float resX;
    float resY;
    float dim;
    vec4 l1;   // xy: crop centre in item px, z: cover scale (px per uv), w: opacity
    vec4 l2;
    vec4 l3;
    vec4 l4;
};

layout(binding = 1) uniform sampler2D tex;

vec3 overLayer(vec3 base, vec2 px, vec4 l) {
    vec2 uv = (px - l.xy) / l.z + 0.5;
    vec4 t = texture(tex, uv);
    return t.rgb * l.w + base * (1.0 - t.a * l.w);
}

void main() {
    vec2 px = qt_TexCoord0 * vec2(resX, resY);
    vec3 c = vec3(10.0 / 255.0, 10.0 / 255.0, 11.0 / 255.0); // #0a0a0b
    c = overLayer(c, px, l1);
    c = overLayer(c, px, l2);
    c = overLayer(c, px, l3);
    c = overLayer(c, px, l4);
    c *= 1.0 - dim;
    float y = qt_TexCoord0.y;
    float scrim = y < 0.35
        ? (102.0 / 255.0) * (1.0 - y / 0.35)          // #66000000 -> transparent
        : (128.0 / 255.0) * ((y - 0.35) / 0.65);      // transparent -> #80000000
    c *= 1.0 - scrim;
    // Opaque on purpose (blending: false on the item): the base rect makes
    // alpha 1 everywhere, and an opaque node leaves the alpha pass entirely.
    fragColor = vec4(c, 1.0);
}
