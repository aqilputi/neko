#version 300 es
// Cozy lighting: half-lambert in 3 soft bands, hemispheric ambient, a
// subtle rim, linear fog; fading instances dither out (screen-door, so
// they stay in the opaque pass and need no sorting).
precision highp float;

in vec3 v_world;
in vec3 v_normal;
in vec3 v_color;
in float v_emissive;
flat in float v_visible;

uniform vec3 u_eye;
uniform vec3 u_sun_dir;
uniform vec3 u_sun_color;
uniform vec3 u_amb_sky;
uniform vec3 u_amb_ground;
uniform vec3 u_fog_color;
uniform vec2 u_fog; // start, end (m)

out vec4 o_color;

const float BAYER[16] = float[16](0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);

float bayer4(vec2 p) {
    ivec2 i = ivec2(mod(p, 4.0));
    return (BAYER[i.y * 4 + i.x] + 0.5) / 16.0;
}

void main() {
    if (v_visible < bayer4(gl_FragCoord.xy)) {
        discard;
    }
    vec3 n = normalize(v_normal);
    float l = dot(n, u_sun_dir) * 0.5 + 0.5;
    // the ramp: shadow, mid, lit; edges softened a little so they do not crawl
    float band = 0.42 + 0.33 * smoothstep(0.40, 0.46, l) + 0.25 * smoothstep(0.70, 0.76, l);
    vec3 amb = mix(u_amb_ground, u_amb_sky, n.y * 0.5 + 0.5);
    vec3 v = normalize(u_eye - v_world);
    float rim = pow(1.0 - max(dot(n, v), 0.0), 3.0);
    vec3 lit = v_color * (amb * 0.8 + u_sun_color * band * 0.5) + rim * 0.10 * u_amb_sky;
    lit = mix(lit, v_color, clamp(v_emissive, 0.0, 1.0));
    float f = clamp((distance(u_eye, v_world) - u_fog.x) / max(u_fog.y - u_fog.x, 1e-3), 0.0, 1.0);
    o_color = vec4(mix(lit, u_fog_color, f), 1.0);
}
