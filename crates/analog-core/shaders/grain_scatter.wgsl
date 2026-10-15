// Grain pass 1: scatter the raw grain field.
//
// Generates the per-channel grain field (value noise with chromatic offsets,
// fine grain, dissolution clumping, dust specks and scratches) into an RGBA8
// buffer where each channel is the noise in 0..1. A later pass blurs it and the
// final pass composites it. Deterministic: the same seed reproduces the field.
//
// Binding convention: 0 = image (dimensions only), 1 = output, 2 = uniform.

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
@group(0) @binding(1) var<storage, read_write> dst: array<u32>;
@group(0) @binding(2) var<uniform> params: Params;

const COORD_OFFSET: f32 = 4096.0;

fn hash_u32(x: u32) -> u32 {
    var h = x;
    h = h ^ (h >> 16u);
    h = h * 0x7feb352du;
    h = h ^ (h >> 15u);
    h = h * 0x846ca68bu;
    h = h ^ (h >> 16u);
    return h;
}

fn hash2(ix: i32, iy: i32, seed: u32) -> f32 {
    var h = u32(ix) * 0x1f1f1f1fu + u32(iy) * 0x85ebca6bu + seed;
    h = hash_u32(h);
    return f32(h) * (1.0 / 4294967296.0);
}

fn value_noise(p: vec2<f32>, seed: u32) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let ix = i32(i.x);
    let iy = i32(i.y);
    let a = hash2(ix, iy, seed);
    let b = hash2(ix + 1, iy, seed);
    let c = hash2(ix, iy + 1, seed);
    let d = hash2(ix + 1, iy + 1, seed);
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm(p: vec2<f32>, seed: u32) -> f32 {
    var v = 0.0;
    var amp = 0.5;
    var q = p;
    for (var o = 0u; o < 2u; o = o + 1u) {
        v = v + amp * value_noise(q, seed + o * 101u);
        q = q * 2.0;
        amp = amp * 0.5;
    }
    return v * (1.0 / 0.75);
}

//@IMAGE_LOAD

fn pack_rgba(c: vec4<f32>) -> u32 {
    let r = u32(round(clamp(c.r, 0.0, 1.0) * 255.0));
    let g = u32(round(clamp(c.g, 0.0, 1.0) * 255.0));
    let b = u32(round(clamp(c.b, 0.0, 1.0) * 255.0));
    let a = u32(round(clamp(c.a, 0.0, 1.0) * 255.0));
    return r | (g << 8u) | (b << 16u) | (a << 24u);
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.width || gid.y >= params.height) {
        return;
    }
    let idx = gid.y * params.width + gid.x;

    let amount = params.grain_amount / 100.0;
    var cell = 3.0;
    if (params.grain_type == 1u) {
        cell = mix(2.5, 10.0, amount);
    } else if (params.grain_type == 2u) {
        cell = mix(1.0, 4.0, amount);
    } else {
        cell = mix(3.0, 12.0, amount);
    }
    let uv = (vec2<f32>(f32(gid.x), f32(gid.y)) + vec2<f32>(COORD_OFFSET)) / cell;

    let seed = params.seed_lo ^ (params.seed_hi * 0x9e3779b9u);

    let chroma = params.grain_color / 100.0;
    let shift = (params.color_shift - 50.0) / 50.0;
    let co = vec2<f32>(1.0, 0.0) * shift * 0.5 + vec2<f32>(0.0, 0.37);
    let ng = fbm(uv, seed);
    let mono = vec3<f32>(ng);
    let chroma_noise = vec3<f32>(fbm(uv + co, seed), ng, fbm(uv - co, seed));
    var n = mix(mono, chroma_noise, chroma);

    let hf = value_noise(uv * 3.0, seed ^ 0x9e3779b9u) - 0.5;
    n = n + vec3<f32>(hf) * (params.fine_grain / 100.0) * 0.5;

    // Dissolution clumps the grain with a low-frequency field.
    let clump = fbm(uv * 0.25, seed ^ 0xdeadbeefu);
    let clump_mod = mix(1.0, clump * 2.0, params.dissolution / 100.0);
    n = vec3<f32>(0.5) + (n - vec3<f32>(0.5)) * clump_mod;

    // Dust specks above 10 and scratches above 20.
    if (params.dissolution > 10.0) {
        let density = (params.dissolution - 10.0) / 100.0 * 0.01;
        if (hash2(i32(gid.x), i32(gid.y), seed ^ 0x51u) < density) {
            n = vec3<f32>(1.0);
        }
    }
    if (params.dissolution > 20.0) {
        let scratch = value_noise(vec2<f32>(f32(gid.x) * 6.0, f32(gid.y) * 0.08), seed ^ 0x77u);
        if (scratch > 0.996) {
            n = vec3<f32>(0.0);
        }
    }

    // Transparent pixels carry no grain (the image is read for its alpha).
    let alpha = load_image(idx).a;
    n = mix(vec3<f32>(0.5), n, alpha);

    dst[idx] = pack_rgba(vec4<f32>(n, 1.0));
}
