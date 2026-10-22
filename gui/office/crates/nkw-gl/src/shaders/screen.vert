#version 300 es
// A client window: the unit quad (centre origin, +y up, read along -z),
// grown by `u_margin` (uv units) when it carries the selection outline.
uniform mat4 u_model;
uniform mat4 u_view_proj;
uniform vec2 u_margin;

out vec2 v_uv;
out vec3 v_world;

void main() {
    vec2 corner = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
    vec2 uv = mix(-u_margin, vec2(1.0) + u_margin, corner);
    vec4 world = u_model * vec4(uv - 0.5, 0.0, 1.0);
    v_uv = uv;
    v_world = world.xyz;
    gl_Position = u_view_proj * world;
}
