#version 300 es
// Preset 64: nearest upscale of the low-res world, Bayer 4×4 dithered down
// to 5 bits per channel. The dither pattern lives on the low-res grid, so
// every big pixel stays one flat colour.
precision highp float;

uniform sampler2D u_src;
uniform float u_div; // integer upscale factor

out vec4 o_color;

const float BAYER[16] = float[16](0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);

void main() {
    ivec2 p = ivec2(floor(gl_FragCoord.xy / u_div));
    vec3 c = texelFetch(u_src, p, 0).rgb;
    ivec2 b = p % 4;
    float t = (BAYER[b.y * 4 + b.x] + 0.5) / 16.0;
    o_color = vec4(floor(c * 31.0 + t) / 31.0, 1.0);
}
