// Grain pass 3: composite the grain field onto the image.
//
// Modulates by the adaptive luminance zones, mixes the sharp field toward the
// blurred field for depth-based focus control, and compensates the contrast lost
// to the blur.
//
// Binding convention: 0 = image, 1 = sharp field, 2 = blurred field,
// 3 = depth, 4 = output, 5 = uniform.

struct Params {
    width: u32,
    height: u32,
    seed_lo: u32,
    seed_hi: u32,
    intensity: f32,
    grain_amount: f32,
    softness: f32,
    grain_color: f32,
    color_shift: f32,
    dissolution: f32,
    lum_highlights: f32,
    lum_mids: f32,
    lum_shadows: f32,
    fine_grain: f32,
    grain_type: u32,
    depth_intensity: f32,
    blur_radius: u32,
    pad0: u32,
    pad1: u32,
    pad2: u32,
};

@group(0) @binding(0) var<storage, read> src: array<u32>;
@group(0) @binding(1) var<storage, read> sharp: array<u32>;
@group(0) @binding(2) var<storage, read> blurred: array<u32>;
@group(0) @binding(3) var<storage, read> depth: array<u32>;
@group(0) @binding(4) var<storage, read_write> dst: array<u32>;
@group(0) @binding(5) var<uniform> params: Params;

const LUMA = vec3<f32>(0.2126, 0.7152, 0.0722);
const GRAIN_GAIN: f32 = 0.30;

fn zone_weights(l: f32) -> vec3<f32> {
    let ws = smoothstep(0.05, 0.25, l) * (1.0 - smoothstep(0.25, 0.50, l));
    let wm = smoothstep(0.25, 0.50, l) * (1.0 - smoothstep(0.55, 0.80, l));
    let wh = smoothstep(0.55, 0.80, l) * (1.0 - smoothstep(0.85, 0.95, l));
    return vec3<f32>(ws, wm, wh);
}

//@IMAGE_LOAD
//@IMAGE_STORE

// 8-bit loader for the auxiliary field and depth buffers.
fn unpack8(p: u32) -> vec4<f32> {
    let r = f32(p & 0xFFu) / 255.0;
    let g = f32((p >> 8u) & 0xFFu) / 255.0;
    let b = f32((p >> 16u) & 0xFFu) / 255.0;
    let a = f32((p >> 24u) & 0xFFu) / 255.0;
    return vec4<f32>(r, g, b, a);
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.width || gid.y >= params.height) {
        return;
    }
    let idx = gid.y * params.width + gid.x;
    let c = load_image(idx);

    let luma = dot(c.rgb, LUMA);
    let weights = zone_weights(luma);
    let mult = vec3<f32>(params.lum_shadows, params.lum_mids, params.lum_highlights) / 50.0;
    let zone = dot(weights, mult);

    // Focus: 1 in focus (near) uses the sharp field; 0 uses the blurred field.
    var focus = 0.0;
    if (params.depth_intensity > 0.0) {
        let d = f32(depth[idx] & 0xFFu) / 255.0;
        focus = (params.depth_intensity / 100.0) * d;
    }

    let n = unpack8(sharp[idx]).rgb;
    let b = unpack8(blurred[idx]).rgb;
    let field = mix(b, n, focus);

    // Mild contrast compensation for the blur: the grain field is already
    // low-frequency, so a full white-noise compensation would over-boost it.
    let comp = 1.0 + 0.5 * f32(params.blur_radius) / 12.0;
    let grain = (field - vec3<f32>(0.5)) * 2.0 * comp;
    let amp = (params.intensity / 100.0) * zone * GRAIN_GAIN;
    let grained = c.rgb + grain * amp;

    store_image(idx, vec4<f32>(grained, c.a));
}
