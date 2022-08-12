#version 140

uniform sampler2D tex;
in vec2 v_tex_coords;

void main() {
    vec4 pixel = texture(tex, v_tex_coords);
    gl_FragColor = vec4(pixel.rgb, 1.0);
}
