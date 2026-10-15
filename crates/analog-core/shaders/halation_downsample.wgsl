// Halation pass 2: 2x2 box downsample for the bloom mip chain.
//
// Binding convention: 0 = source (larger), 1 = output (half), 2 = uniform.

struct Params {
    in_w: u32,
    in_h: u32,
    out_w: u32,
    out_h: u32,
};

@group(0) @binding(0) var<storage, read> src: array<u32>;
@group(0) @binding(1) var<storage, read_write> dst: array<u32>;
@group(0) @binding(2) var<uniform> params: Params;

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

fn fetch(x: u32, y: u32) -> vec4<f32> {
    let cx = min(x, params.in_w - 1u);
    let cy = min(y, params.in_h - 1u);
    return unpack_rgba(src[cy * params.in_w + cx]);
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.out_w || gid.y >= params.out_h) {
        return;
    }
    let x0 = gid.x * 2u;
    let y0 = gid.y * 2u;
    let sum = fetch(x0, y0) + fetch(x0 + 1u, y0) + fetch(x0, y0 + 1u) + fetch(x0 + 1u, y0 + 1u);
    dst[gid.y * params.out_w + gid.x] = pack_rgba(sum * 0.25);
}
