#version 140

uniform sampler2D tex;
uniform float timestamp;
in vec2 v_tex_coords;
flat in uint v_is_bg;
flat in uint v_blinking;
flat in uvec2 v_color;

vec3 hsv2rgb(float h, float s, float v) {
    return ((clamp(abs(fract(h + vec3(0.0, 2.0, 1.0) / 3.0) * 6.0 - 3.0) - 1.0, 0.0, 1.0) - 1.0) * s + 1.0) * v;
}

vec4 get_color(uint color) {
    if (color == 0xFFFFFF00u) {
        return vec4(hsv2rgb(timestamp / 3000.0, 1.0, 1.0), 1.0);
    } else {
        uint r = (color & 0xFF000000u) >> 24;
        uint g = (color & 0x00FF0000u) >> 16;
        uint b = (color & 0x0000FF00u) >> 8;
        uint a = (color & 0x000000FFu);
        return vec4(float(r) / 255.0, float(g) / 255.0, float(b) / 255.0, float(a) / 255.0);
    }
}

void main() {
    vec4 back = get_color(v_color[0]);
    vec4 fore = get_color(v_color[1]);

    uint phase = uint(timestamp / 250.0);
    if (v_blinking == 1u && phase % 8u < 4u) {
        fore = back;
    } else if (v_blinking == 2u && phase % 2u < 1u) {
        fore = back;
    }

    if (v_is_bg == 1u) {
        gl_FragColor = back;
    } else {
        float a = texture(tex, v_tex_coords).r;
        gl_FragColor = vec4(fore.rgb, a);
    }
}
