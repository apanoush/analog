// P2 plumbing pass: converts RGBA8 to BT.709 luma while preserving alpha.
//
// This pass exists only to prove texture upload, compute dispatch and readback
// end to end. It is not part of the final effect pipeline and will be joined by
// the real Film Stock, Grain, Halation and Print passes in later milestones.
//
// Buffers hold packed little-endian RGBA8 words: byte 0 (R) in the low bits.

struct Params {
    width: u32,
    height: u32,
    pad0: u32,
    pad1: u32,
};

@group(0) @binding(0) var<storage, read> src: array<u32>;
@group(0) @binding(1) var<storage, read_write> dst: array<u32>;
@group(0) @binding(2) var<uniform> params: Params;

const LUMA = vec3<f32>(0.2126, 0.7152, 0.0722);

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

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.width || gid.y >= params.height) {
        return;
    }
    let idx = gid.y * params.width + gid.x;
    let c = unpack_rgba(src[idx]);
    let luma = dot(c.rgb, LUMA);
    dst[idx] = pack_rgba(vec4<f32>(luma, luma, luma, c.a));
}
