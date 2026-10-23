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
    /// Long side of the source image. Grain coordinates are normalised to this
    /// so a 900 px preview matches the full-resolution export at fit.
    original_long_side: u32,
    /// Luminance range of the source image, used by the adaptive zones.
    luma_min: f32,
    luma_max: f32,
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

// Three octaves at non-integer ratios. Rotating and offsetting each octave
// breaks lattice alignment, so the sum does not read as a grid. The finest
// octave is roughly one third of the cell size, which gives the per-pixel
// sparkle that smooth two-octave value noise lacks.
fn fbm(p: vec2<f32>, seed: u32) -> f32 {
    var v = 0.0;
    var norm = 0.0;
    var amp = 0.55;
    var q = p;
    for (var o = 0u; o < 3u; o = o + 1u) {
        let a = 0.7 + f32(o) * 1.9;
        let c = cos(a);
        let s = sin(a);
        let r = vec2<f32>(c * q.x - s * q.y, s * q.x + c * q.y);
        let offset = vec2<f32>(f32(o) * 37.1 + 11.3, f32(o) * 17.9 + 5.7);
        v = v + amp * value_noise(r + offset, seed + o * 101u);
        norm = norm + amp;
        q = q * 1.9;
        amp = amp * 0.48;
    }
    return v / norm;
}

//@IMAGE_LOAD

fn pack_rgba16(c: vec4<f32>) -> vec2<u32> {
    let r = u32(round(clamp(c.r, 0.0, 1.0) * 65535.0));
    let g = u32(round(clamp(c.g, 0.0, 1.0) * 65535.0));
    let b = u32(round(clamp(c.b, 0.0, 1.0) * 65535.0));
    let a = u32(round(clamp(c.a, 0.0, 1.0) * 65535.0));
    return vec2<u32>(r | (g << 16u), b | (a << 16u));
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.width || gid.y >= params.height) {
        return;
    }
    let idx = gid.y * params.width + gid.x;

    let amount = params.grain_amount / 100.0;
    // Cell size in reference pixels at a 3000 px long side. Fine is the
    // tightest; these ranges replace the old 3-12 px lattice.
    var cell = 1.0;
    if (params.grain_type == 1u) {
        cell = mix(0.6, 6.8, amount);
    } else if (params.grain_type == 2u) {
        cell = mix(0.3, 3.4, amount);
    } else {
        cell = mix(0.75, 6.0, amount);
    }

    // Normalise coordinates to a 3000 px reference so grain keeps the same
    // frame-relative size at every resolution. `original_long_side` is the
    // source image's long side, which lets the GUI's proxy preview match the
    // full-resolution export.
    let max_dim = max(f32(params.width), f32(params.height));
    let reference = min(3000.0, max(f32(params.original_long_side), 1.0));
    let reference_pos = (vec2<f32>(f32(gid.x), f32(gid.y)) / max_dim) * reference;
    let uv = (reference_pos + vec2<f32>(COORD_OFFSET)) / cell;

    let seed = params.seed_lo ^ (params.seed_hi * 0x9e3779b9u);

    let chroma = params.grain_color / 100.0;
    let shift = (params.color_shift - 50.0) / 50.0;
    let angle = 0.6 + shift * 1.2;
    let dir = vec2<f32>(cos(angle), sin(angle));
    let off = dir * 0.5;

    let ng = fbm(uv, seed);
    var n = vec3<f32>(ng);
    if (chroma > 0.0) {
        // Opposite samples of the same field. The green deviation is chosen so
        // the three channel deviations sum to zero under BT.709 luma, which
        // keeps chroma grain from changing local brightness.
        let d1 = fbm(uv + off, seed) - ng;
        let d2 = fbm(uv - off, seed) - ng;
        let dg = -(0.2126 * d1 + 0.0722 * d2) / 0.7152;
        n = vec3<f32>(ng + d1 * chroma, ng + dg * chroma, ng + d2 * chroma);
    }

    let hf = value_noise(uv * 3.0, seed ^ 0x9e3779b9u) - 0.5;
    n = n + vec3<f32>(hf) * (params.fine_grain / 100.0) * 0.5;

    // Dissolution clumps the grain with a low-frequency density field. Even at
    // zero it leaves a little variation, so the texture does not read as
    // uniform digital noise.
    let clump = value_noise(uv * 0.25, seed ^ 0xdeadbeefu) - 0.5;
    let clump_amount = 0.15 + 0.35 * (params.dissolution / 100.0);
    n = vec3<f32>(0.5) + (n - vec3<f32>(0.5)) * (1.0 + clump * clump_amount);

    // Transparent pixels carry no grain (the image is read for its alpha).
    let alpha = load_image(idx).a;
    n = mix(vec3<f32>(0.5), n, alpha);

    let packed = pack_rgba16(vec4<f32>(n, 1.0));
    dst[idx * 2u] = packed.x;
    dst[idx * 2u + 1u] = packed.y;
}
