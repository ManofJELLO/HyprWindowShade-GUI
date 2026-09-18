#version 320 es
precision highp float;
// @label Wobble
// @desc Jelly deformation while a window is dragged or thrown

in  vec2 v_texcoord;
out vec4 fragColor;
uniform sampler2D tex;
uniform vec2  peak_velocity;       // fastest speed this gesture, px/s
uniform vec2  peak_size_velocity;  // and the resize equivalent
uniform float settle;              // 0 -> 1 across the tail after motion stops

// Measured peaks, from the plugin's README: a keybind move reaches ~32000 px/s,
// a mouse drag ~9000, a resize ~1400 on size_velocity. One gain cannot serve all
// three, which is why move and resize are scaled separately.

// @param 0.0 0.00004 0.000001 "Move gain"
const float MOVE_GAIN = 0.000012;

// @param 0.0 0.0004 0.00001 "Resize gain"
const float RESIZE_GAIN = 0.00009;

// @param 2.0 30.0 0.5 "Ripple frequency"
const float FREQ = 11.0;

// @param 1.0 14.0 0.5 "Decay"
const float DECAY = 5.0;

void main() {
    // Motion uniforms are always zero on a layer surface, so this shader is a
    // silent no-op there. Drive a layer from progress instead.
    vec2 drive = peak_velocity * MOVE_GAIN + peak_size_velocity * RESIZE_GAIN;

    float envelope = exp(-settle * DECAY) * (1.0 - settle);
    vec2 offset = drive * sin(v_texcoord.yx * FREQ * 3.14159) * envelope;

    vec4 col = texture(tex, clamp(v_texcoord + offset, 0.0, 1.0));
    fragColor = col;
}
