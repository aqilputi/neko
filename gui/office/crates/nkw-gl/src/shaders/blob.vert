#version 300 es
// Blob shadows: one instanced quad on the floor per blob.
layout(location = 0) in vec4 i_blob;  // centre xyz, radius
layout(location = 1) in float i_alpha;

uniform mat4 u_view_proj;

out vec2 v_p;
out float v_alpha;
out vec3 v_world;

void main() {
    vec2 c = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1)) * 2.0 - 1.0;
    // 4 mm above the floor (plus polygon offset) so it never z-fights
    vec3 world = i_blob.xyz + vec3(c.x, 0.0, c.y) * i_blob.w + vec3(0.0, 0.004, 0.0);
    v_p = c;
    v_alpha = i_alpha;
    v_world = world;
    gl_Position = u_view_proj * vec4(world, 1.0);
}
