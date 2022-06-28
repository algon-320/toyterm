#version 140

in vec2 position;
in vec2 tex_coords;
in uint is_bg;
in uvec2 color;

out vec2 v_tex_coords;
flat out uint v_is_bg;
flat out uvec2 v_color;

void main() {
    gl_Position = vec4(position, 0, 1);
    v_tex_coords = tex_coords;
    v_is_bg = is_bg;
    v_color = color;
}
