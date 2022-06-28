#version 140

uniform sampler2D tex;
uniform float timestamp;
in vec2 v_tex_coords;
flat in uint v_is_bg;
flat in uvec2 v_color;

vec3 hsv2rgb(float h, float s, float v) {
    return ((clamp(abs(fract(h + vec3(0.0, 2.0, 1.0) / 3.0) * 6.0 - 3.0) - 1.0, 0.0, 1.0) - 1.0) * s + 1.0) * v;
}

vec3 get_color(uint color) {
    if (color == 0xFFFFFF00u) {
        return hsv2rgb(timestamp / 3000.0, 1.0, 1.0);
    } else {
        uint r = (color & 0xFF000000u) >> 24;
        uint g = (color & 0x00FF0000u) >> 16;
        uint b = (color & 0x0000FF00u) >> 8;
        return vec3(float(r) / 256.0, float(g) / 256.0, float(b) / 256.0);
    }
}

void main() {
    vec3 back = get_color(v_color[0]);
    vec3 fore = get_color(v_color[1]);

    if (v_is_bg == 1u) {
        gl_FragColor = vec4(back, 1.0);
    } else {
        float a = texture(tex, v_tex_coords).r;

        float gamma = 1.8;
        vec3 fore_lin = pow(fore, vec3(gamma));
        vec3 back_lin = pow(back, vec3(gamma));
        vec3 rgb_lin = vec3(a) * fore_lin + vec3(1 - a) * back_lin;
        vec3 rgb = pow(rgb_lin, vec3(1.0 / gamma));

        gl_FragColor = vec4(rgb, 1.0);
    }
}
