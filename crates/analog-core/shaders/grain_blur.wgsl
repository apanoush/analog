// Grain pass 2: separable box blur of the grain field.
//
// Run twice (horizontal then vertical) to approximate a Gaussian, which softens
// the grain. `dir` selects the axis. The final composite compensates contrast.
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

fn fetch(x: i32, y: i32) -> vec4<f32> {
    let cx = min(max(x, 0), i32(params.width) - 1);
    let cy = min(max(y, 0), i32(params.height) - 1);
    return unpack_rgba(src[u32(cy) * params.width + u32(cx)]);
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.width || gid.y >= params.height) {
        return;
    }
    let x = i32(gid.x);
    let y = i32(gid.y);
    var sum = vec4<f32>(0.0);
    var count = 0.0;
    for (var k = -params.radius; k <= params.radius; k = k + 1) {
        if (params.dir == 0u) {
            sum = sum + fetch(x + k, y);
        } else {
            sum = sum + fetch(x, y + k);
        }
        count = count + 1.0;
    }
    dst[gid.y * params.width + gid.x] = pack_rgba(sum / count);
}
