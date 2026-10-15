// Halation pass 3: tent upsample of the coarse mip, added into the fine mip.
//
// Binding convention: 0 = fine (out-sized), 1 = coarse (half), 2 = output, 3 = uniform.

struct Params {
    out_w: u32,
    out_h: u32,
    in_coarse_w: u32,
    in_coarse_h: u32,
    weight: f32,
    pad0: u32,
    pad1: u32,
    pad2: u32,
};

@group(0) @binding(0) var<storage, read> fine: array<u32>;
@group(0) @binding(1) var<storage, read> coarse: array<u32>;
@group(0) @binding(2) var<storage, read_write> dst: array<u32>;
@group(0) @binding(3) var<uniform> params: Params;

fn unpack_rgba(p: u32) -> vec4<f32> {
    let r = f32(p & 0xFFu) / 255.0;
    let g = f32((p >> 8u) & 0xFFu) / 255.0;
    let b = f32((p >> 16u) & 0xFFu) / 255.0;
    let a = f32((p >> 24u) & 0xFFu) / 255.0;
    return vec4<f32>(r, g, b, a);
}

fn pack_rgba(c: vec4<f32>) -> u32 {
    let r = u32(round(clamp(c.r, 0.0, 1.0) * 255.0));
    let g = u32(round(clamp(c.g, 0.0, 1.0) * 255.0));
    let b = u32(round(clamp(c.b, 0.0, 1.0) * 255.0));
    let a = u32(round(clamp(c.a, 0.0, 1.0) * 255.0));
    return r | (g << 8u) | (b << 16u) | (a << 24u);
}

fn coarse_at(x: i32, y: i32) -> vec4<f32> {
    let cx = min(max(x, 0), i32(params.in_coarse_w) - 1);
    let cy = min(max(y, 0), i32(params.in_coarse_h) - 1);
    return unpack_rgba(coarse[u32(cy) * params.in_coarse_w + u32(cx)]);
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.out_w || gid.y >= params.out_h) {
        return;
    }
    let idx = gid.y * params.out_w + gid.x;
    let f = unpack_rgba(fine[idx]);

    // Bilinear (tent) sample of the coarse mip at this pixel's centre.
    let uv = (vec2<f32>(f32(gid.x), f32(gid.y)) + 0.5) * 0.5 - 0.5;
    let p = clamp(uv, vec2<f32>(0.0), vec2<f32>(f32(params.in_coarse_w) - 1.0, f32(params.in_coarse_h) - 1.0));
    let i0 = floor(p);
    let frac = p - i0;
    let x0 = i32(i0.x);
    let y0 = i32(i0.y);
    let a = coarse_at(x0, y0);
    let b = coarse_at(x0 + 1, y0);
    let c = coarse_at(x0, y0 + 1);
    let d = coarse_at(x0 + 1, y0 + 1);
    let up = mix(mix(a, b, frac.x), mix(c, d, frac.x), frac.y);

    dst[idx] = pack_rgba(f + up * params.weight);
}
