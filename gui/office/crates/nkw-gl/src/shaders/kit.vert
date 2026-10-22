#version 300 es
// Kit meshes, instanced: one draw per MeshId.
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec4 a_normal; // i8 normalised
layout(location = 2) in vec4 a_color;  // u8 normalised
layout(location = 3) in vec4 i_m0;
layout(location = 4) in vec4 i_m1;
layout(location = 5) in vec4 i_m2;
layout(location = 6) in vec4 i_m3;
layout(location = 7) in vec4 i_tint;   // u8 normalised
layout(location = 8) in vec2 i_params; // emissive, fade

uniform mat4 u_view_proj;

out vec3 v_world;
out vec3 v_normal;
out vec3 v_color;
out float v_emissive;
flat out float v_visible;

void main() {
    mat4 model = mat4(i_m0, i_m1, i_m2, i_m3);
    vec4 world = model * vec4(a_pos, 1.0);
    v_world = world.xyz;
    // inverse transpose: walls and floors are boxes scaled non-uniformly
    v_normal = transpose(inverse(mat3(model))) * a_normal.xyz;
    v_color = a_color.rgb * i_tint.rgb;
    v_emissive = i_params.x;
    v_visible = (1.0 - clamp(i_params.y, 0.0, 1.0)) * i_tint.a;
    gl_Position = u_view_proj * world;
}
