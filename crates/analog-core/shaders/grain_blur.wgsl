// Grain pass 2: separable Gaussian blur of the 16-bit grain field.
//
// Run twice (horizontal then vertical). `radius` is the half-width in pixels;
// sigma is radius / 2. A radius of 0 copies the field through. The final
// composite compensates a little of the contrast lost to the blur.
//
// Binding convention: 0 = input, 1 = output, 2 = uniform.

struct Params {
    width: u32,
    height: u32,
    radius: i32,
    dir: u32,
};

@group(0) @binding(0) var<storage, read> src: array<u32>;
@group(0) @binding(1) var<storage, read_write> dst: array<u32>;
@group(0) @binding(2) var<uniform> params: Params;

fn unpack_rgba16(p0: u32, p1: u32) -> vec4<f32> {
    return vec4<f32>(
        f32(p0 & 0xFFFFu),
        f32((p0 >> 16u) & 0xFFFFu),
        f32(p1 & 0xFFFFu),
        f32((p1 >> 16u) & 0xFFFFu),
    ) / 65535.0;
}

fn pack_rgba16(c: vec4<f32>) -> vec2<u32> {
    let r = u32(round(clamp(c.r, 0.0, 1.0) * 65535.0));
    let g = u32(round(clamp(c.g, 0.0, 1.0) * 65535.0));
    let b = u32(round(clamp(c.b, 0.0, 1.0) * 65535.0));
    let a = u32(round(clamp(c.a, 0.0, 1.0) * 65535.0));
    return vec2<u32>(r | (g << 16u), b | (a << 16u));
}

fn fetch(x: i32, y: i32) -> vec4<f32> {
    let cx = min(max(x, 0), i32(params.width) - 1);
    let cy = min(max(y, 0), i32(params.height) - 1);
    let i = (u32(cy) * params.width + u32(cx)) * 2u;
    return unpack_rgba16(src[i], src[i + 1u]);
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.width || gid.y >= params.height) {
        return;
    }
    let x = i32(gid.x);
    let y = i32(gid.y);
    let idx = gid.y * params.width + gid.x;

    if (params.radius <= 0) {
        let p = pack_rgba16(fetch(x, y));
        dst[idx * 2u] = p.x;
        dst[idx * 2u + 1u] = p.y;
        return;
    }

    let sigma = f32(params.radius) * 0.5;
    let denom = 2.0 * sigma * sigma;
    var sum = vec4<f32>(0.0);
    var weight_sum = 0.0;
    for (var k = -params.radius; k <= params.radius; k = k + 1) {
        let weight = exp(-f32(k * k) / denom);
        if (params.dir == 0u) {
            sum = sum + fetch(x + k, y) * weight;
        } else {
            sum = sum + fetch(x, y + k) * weight;
        }
        weight_sum = weight_sum + weight;
    }
    let p = pack_rgba16(sum / weight_sum);
    dst[idx * 2u] = p.x;
    dst[idx * 2u + 1u] = p.y;
}
