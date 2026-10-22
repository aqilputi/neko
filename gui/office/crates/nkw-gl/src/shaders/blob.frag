#version 300 es
// A soft radial gradient, premultiplied, in a warm dark (never pure black).
precision highp float;

in vec2 v_p;
in float v_alpha;
in vec3 v_world;

uniform vec3 u_eye;
uniform vec2 u_fog;

out vec4 o_color;

const vec3 SHADE = vec3(0.20, 0.13, 0.16);

void main() {
    float r = length(v_p);
    float a = v_alpha * (1.0 - smoothstep(0.15, 1.0, r));
    float f = clamp((distance(u_eye, v_world) - u_fog.x) / max(u_fog.y - u_fog.x, 1e-3), 0.0, 1.0);
    a *= 1.0 - f;
    o_color = vec4(SHADE * a, a);
}
