#version 320 es
precision highp float;
// @label Reading mode
// @desc Warm paper tint with a gentle dim, for long reads

in  vec2 v_texcoord;
out vec4 fragColor;
uniform sampler2D tex;
uniform float is_active;

// Every const below is exposed in the GUI. The @param comment gives it a real
// range and a label; without one the range is guessed from the value.

// @param 0.0 1.0 0.01 "Warmth"
const float WARMTH = 0.35;

// @param 0.0 0.6 0.01 "Desaturate"
const float DESATURATE = 0.25;

// @param 0.4 1.0 0.01 "Brightness when unfocused"
const float UNFOCUSED_DIM = 0.75;

// @color "Paper tint"
const vec3 PAPER_COLOR = vec3(0.99, 0.95, 0.87);

void main() {
    vec4 col = texture(tex, v_texcoord);

    float luma = dot(col.rgb, vec3(0.2126, 0.7152, 0.0722));
    col.rgb = mix(col.rgb, vec3(luma), DESATURATE);
    col.rgb = mix(col.rgb, col.rgb * PAPER_COLOR, WARMTH);
    col.rgb *= mix(UNFOCUSED_DIM, 1.0, is_active);

    fragColor = col;
}
