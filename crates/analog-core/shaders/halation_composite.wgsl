// Halation pass 4: composite the bloom onto the image.
//
// 70 percent additive plus 30 percent screen, hue-tinted yellow-to-red, with
// highlight protection, an optional depth-aware background control, and a small
// dither to break banding.
//
// Binding convention: 0 = image, 1 = bloom, 2 = depth, 3 = output, 4 = uniform.

struct Params {
    width: u32,
    height: u32,
    intensity: f32,
    tint_r: f32,
    tint_g: f32,
    tint_b: f32,
    background_focus: f32,
    background_gain: f32,
    pad0: u32,
    pad1: u32,
    pad2: u32,
    pad3: u32,
};

@group(0) @binding(0) var<storage, read> src: array<u32>;
@group(0) @binding(1) var<storage, read> bloom: array<u32>;
@group(0) @binding(2) var<storage, read> depth: array<u32>;
@group(0) @binding(3) var<storage, read_write> dst: array<u32>;
@group(0) @binding(4) var<uniform> params: Params;

const LUMA = vec3<f32>(0.2126, 0.7152, 0.0722);

//@IMAGE_LOAD
//@IMAGE_STORE

// 8-bit loader for the bloom and depth buffers.
fn unpack8(p: u32) -> vec4<f32> {
    let r = f32(p & 0xFFu) / 255.0;
    let g = f32((p >> 8u) & 0xFFu) / 255.0;
    let b = f32((p >> 16u) & 0xFFu) / 255.0;
    let a = f32((p >> 24u) & 0xFFu) / 255.0;
    return vec4<f32>(r, g, b, a);
}

fn hash_u32(x: u32) -> u32 {
    var h = x;
    h = h ^ (h >> 16u);
    h = h * 0x7feb352du;
    h = h ^ (h >> 15u);
    h = h * 0x846ca68bu;
    h = h ^ (h >> 16u);
    return h;
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.width || gid.y >= params.height) {
        return;
    }
    let idx = gid.y * params.width + gid.x;
    let base = load_image(idx);
    let bl = unpack8(bloom[idx]);

    let tint = vec3<f32>(params.tint_r, params.tint_g, params.tint_b);
    var glow = bl.rgb * tint * params.intensity;

    // Depth-aware background control: suppress the glow on the in-focus subject
    // and boost it in the defocused background. Only reads depth when
    // background_focus is non-zero (the host forces 0 when no depth is bound).
    if (params.background_focus > 0.0) {
        let d = f32(depth[idx] & 0xFFu) / 255.0;
        let suppress = 1.0 - (params.background_focus / 100.0) * smoothstep(0.25, 0.70, d);
        let gain = 1.0 + (params.background_gain - 50.0) / 50.0 * (1.0 - d) * 0.5;
        glow = glow * suppress * max(gain, 0.0);
    }

    // Highlight protection: fade the glow where the image is already bright.
    let lum = dot(base.rgb, LUMA);
    let protect = smoothstep(0.6, 1.0, lum);
    glow = glow * (1.0 - 0.7 * protect);

    let additive = base.rgb + glow;
    let screen = vec3<f32>(1.0) - (vec3<f32>(1.0) - base.rgb) * (vec3<f32>(1.0) - glow);
    let rgb = mix(additive, screen, 0.3);

    let dither = (f32(hash_u32(idx + 1u)) * (1.0 / 4294967296.0) - 0.5) / 255.0;
    store_image(idx, vec4<f32>(rgb + vec3<f32>(dither), base.a));
}
