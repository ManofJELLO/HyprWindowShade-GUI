#version 320 es
precision highp float;
// @label Smoke close
// @desc Drifts upward and thins out as the window closes
// @duration 0.45
// @overlay

// @overlay above means this composites with Hyprland's own close animation
// rather than replacing it — right for an effect that distorts and fades rather
// than driving the whole disappearance itself.

in  vec2 v_texcoord;
out vec4 fragColor;
uniform sampler2D tex;
uniform float progress;
uniform float seed;

// @param 0.0 0.3 0.005 "Rise"
const float RISE = 0.09;

// @param 0.0 0.08 0.001 "Waver"
const float WAVER = 0.02;

// @param 2.0 24.0 0.5 "Waver frequency"
const float FREQ = 9.0;

void main() {
    float t = progress;

    vec2 uv = v_texcoord;
    uv.y += t * RISE;
    uv.x += sin(uv.y * FREQ + seed * 6.28318 + t * 3.0) * WAVER * t;

    vec4 col = texture(tex, uv);

    // A close shader has to reach fully transparent by progress 1.0, or the
    // snapshot is still on screen when the plugin lets go of it.
    float fade = 1.0 - smoothstep(0.0, 1.0, t);
    fragColor = col * fade;
}
