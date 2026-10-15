// Halation pass 1: bright extraction with a soft knee.
//
// Binding convention: 0 = source image, 1 = output, 2 = uniform.

struct Params {
    width: u32,
    height: u32,
    threshold: f32,
    knee: f32,
};

@group(0) @binding(0) var<storage, read> src: array<u32>;
@group(0) @binding(1) var<storage, read_write> dst: array<u32>;
@group(0) @binding(2) var<uniform> params: Params;

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
    let c = load_image(idx);
    let l = max(c.r, max(c.g, c.b));
    let t = params.threshold / 255.0;
    let w = smoothstep(t, t + params.knee, l);
    dst[idx] = pack_rgba(vec4<f32>(c.rgb * w, 1.0));
}
