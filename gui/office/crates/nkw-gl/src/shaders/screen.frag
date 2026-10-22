// (the #version line and SAMPLER come from screen_2d.frag / screen_ext.frag)
// Client textures are premultiplied; row 0 is the top of the window unless
// `u_yflip` (GL-origin textures, e.g. an FBO the compositor rendered).
precision highp float;

uniform SAMPLER u_tex;
uniform vec2 u_size_m;    // quad size, metres
uniform float u_yflip;
uniform float u_opaque;
uniform float u_dim;      // 0 = normal; colour × (1 - dim)
uniform float u_outline;  // outline width, metres (0 = none)
uniform vec3 u_eye;
uniform vec3 u_fog_color;
uniform vec2 u_fog;

in vec2 v_uv;
in vec3 v_world;

out vec4 o_color;

const vec3 OUTLINE = vec3(1.0, 0.88, 0.55);

void main() {
    vec2 outside = max(max(-v_uv, v_uv - 1.0), 0.0) * u_size_m;
    vec4 c;
    if (outside.x > 0.0 || outside.y > 0.0) {
        float d = length(outside) / max(u_outline, 1e-6);
        float a = (1.0 - smoothstep(0.0, 1.0, d)) * 0.9;
        c = vec4(OUTLINE * a, a);
    } else {
        vec2 st = vec2(v_uv.x, u_yflip > 0.5 ? v_uv.y : 1.0 - v_uv.y);
        c = texture(u_tex, st);
        if (u_opaque > 0.5) {
            c.a = 1.0;
        }
        c.rgb *= 1.0 - u_dim;
    }
    float f = clamp((distance(u_eye, v_world) - u_fog.x) / max(u_fog.y - u_fog.x, 1e-3), 0.0, 1.0);
    o_color = vec4(mix(c.rgb, u_fog_color * c.a, f), c.a);
}
