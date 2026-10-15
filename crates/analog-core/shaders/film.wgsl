// Film Color Shift pass.
//
// Parametric film emulation: lift/gamma/gain, saturation,
// shadow/highlight tint (an approximation of the C-41 orange mask) and, for
// black-and-white stocks, a channel mixer plus paper tone. Per-stock parameters
// live in Rust (src/film.rs). No third-party LUTs are used.
//
// Binding convention: 0 = image, 1 = output, 2 = uniform.

struct Params {
    width: u32,
    height: u32,
    intensity: f32,
    is_bw: u32,
    sat: f32,
    gain_r: f32,
    gain_g: f32,
    gain_b: f32,
    lift_r: f32,
    lift_g: f32,
    lift_b: f32,
    gamma: f32,
    shadow_r: f32,
    shadow_g: f32,
    shadow_b: f32,
    high_r: f32,
    high_g: f32,
    high_b: f32,
    bw_r: f32,
    bw_g: f32,
    bw_b: f32,
    tone_r: f32,
    tone_g: f32,
    tone_b: f32,
};

@group(0) @binding(0) var<storage, read> src: array<u32>;
@group(0) @binding(1) var<storage, read_write> dst: array<u32>;
@group(0) @binding(2) var<uniform> params: Params;

const LUMA = vec3<f32>(0.2126, 0.7152, 0.0722);

//@IMAGE_LOAD
//@IMAGE_STORE

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.width || gid.y >= params.height) {
        return;
    }
    let idx = gid.y * params.width + gid.x;
    let c = load_image(idx);

    var rgb = c.rgb;
    // Lift / gain, then gamma.
    rgb = rgb * vec3<f32>(params.gain_r, params.gain_g, params.gain_b)
        + vec3<f32>(params.lift_r, params.lift_g, params.lift_b);
    rgb = pow(max(rgb, vec3<f32>(0.0)), vec3<f32>(1.0 / max(params.gamma, 0.01)));

    // Saturation (colour stocks).
    let luma = dot(rgb, LUMA);
    rgb = mix(vec3<f32>(luma), rgb, params.sat);

    // Shadow and highlight tint: a warm shadow tint approximates the C-41
    // orange mask.
    let sh = 1.0 - luma;
    let hi = luma;
    rgb = rgb
        + vec3<f32>(params.shadow_r, params.shadow_g, params.shadow_b) * sh * 0.5
        + vec3<f32>(params.high_r, params.high_g, params.high_b) * hi * 0.5;

    if (params.is_bw == 1u) {
        let mono = dot(rgb, vec3<f32>(params.bw_r, params.bw_g, params.bw_b));
        rgb = vec3<f32>(mono) * vec3<f32>(params.tone_r, params.tone_g, params.tone_b);
    }

    let intensity = params.intensity / 100.0;
    store_image(idx, vec4<f32>(mix(c.rgb, rgb, intensity), c.a));
}
