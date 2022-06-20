#version 140

uniform sampler2D tex;
in  vec2 v_tex_coords;

void main() {
    float a = texture(tex, v_tex_coords).r;

    // FIXME
    vec3 fore = vec3(0.8352, 0.7686, 0.6313);
    vec3 back = vec3(0.1137, 0.1254, 0.1294);

    float gamma = 1.8;
    vec3 fore_lin = pow(fore, vec3(gamma));
    vec3 back_lin = pow(back, vec3(gamma));
    vec3 rgb_lin = vec3(a) * fore_lin + vec3(1 - a) * back_lin;
    vec3 rgb = pow(rgb_lin, vec3(1.0 / gamma));

    gl_FragColor = vec4(rgb, 1.0);
}
