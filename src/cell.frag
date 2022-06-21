#version 140

uniform sampler2D tex;
in vec2 v_tex_coords;
flat in uint v_is_bg;
flat in uvec2 v_color_idx;


vec3 get_color(uint idx) {
    // Base16: gruvbox-dark-hard
    if (idx == 0u) {
        return vec3(0.11372549019607843, 0.12549019607843137, 0.12941176470588237);
    } else if (idx == 1u) {
        return vec3(0.984313725490196, 0.28627450980392155, 0.20392156862745098);
    } else if (idx == 2u) {
        return vec3(0.7215686274509804, 0.7333333333333333, 0.14901960784313725);
    } else if (idx == 3u) {
        return vec3(0.9803921568627451, 0.7411764705882353, 0.1843137254901961);
    } else if (idx == 4u) {
        return vec3(0.5137254901960784, 0.6470588235294118, 0.596078431372549);
    } else if (idx == 5u) {
        return vec3(0.8274509803921568, 0.5254901960784314, 0.6078431372549019);
    } else if (idx == 6u) {
        return vec3(0.5568627450980392, 0.7529411764705882, 0.48627450980392156);
    } else if (idx == 7u) {
        return vec3(0.8352941176470589, 0.7686274509803922, 0.6313725490196078);
    } else {
        return vec3(1.0, 0.0, 0.0);
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
