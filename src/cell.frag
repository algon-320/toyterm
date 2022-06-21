#version 140

uniform sampler2D tex;
in vec2 v_tex_coords;
flat in uint v_is_bg;
flat in uvec2 v_color_idx;

vec3 get_color(uint idx) {
    // FIXME
    if (idx == 0u) {
        return vec3(0.1137, 0.1254, 0.1294);
    } else {
        return vec3(0.8352, 0.7686, 0.6313);
    }
}

void main() {
    vec3 back = get_color(v_color_idx[0]);
    vec3 fore = get_color(v_color_idx[1]);

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
