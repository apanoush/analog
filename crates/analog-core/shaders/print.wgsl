// Print simulation pass.
//
// Per-paper tone mapping: a shadow floor, an S-curve, a smooth highlight
// shoulder and a paper tint, blended by the simulation strength.
//
// Binding convention: 0 = image, 1 = output, 2 = uniform.

struct Params {
    width: u32,
    height: u32,
    simulation: f32,
    paper_type: u32,
    paper_intensity: f32,
    rolloff: f32,
    paper_tint: f32,
    pad0: u32,
};

@group(0) @binding(0) var<storage, read> src: array<u32>;
@group(0) @binding(1) var<storage, read_write> dst: array<u32>;
@group(0) @binding(2) var<uniform> params: Params;

//@IMAGE_LOAD
//@IMAGE_STORE

// Smooth shoulder: identity up to `knee`, smoothstep to `white`, gentle
// compression beyond.
fn shoulder(x: f32, knee: f32, white: f32) -> f32 {
    if (x <= knee) {
        return x;
    }
    if (x >= white) {
        return white + (x - white) * 0.25;
    }
    let t = (x - knee) / (white - knee);
    let s = t * t * (3.0 - 2.0 * t);
    return knee + (white - knee) * s;
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.width || gid.y >= params.height) {
        return;
    }
    let idx = gid.y * params.width + gid.x;
    let c = load_image(idx);

    // Per-paper constants: floor, S-curve amount, shoulder knee/white.
    var floor = 0.06;
    var s_amt = 0.15;
    var knee = 0.55;
    var white = 0.78;
    if (params.paper_type == 0u) {
        floor = 0.118;
        s_amt = 0.10;
        knee = 0.45;
        white = 0.72;
    } else if (params.paper_type == 2u) {
        floor = 0.005;
        s_amt = 0.20;
        knee = 0.65;
        white = 0.84;
    }

    let ps = params.paper_intensity / 100.0;
    let floor_eff = floor * ps;
    let s_eff = s_amt * ps;
    // More roll-off pulls the shoulder in earlier.
    let knee_eff = knee * (1.0 - 0.5 * params.rolloff / 100.0);
    let white_eff = min(white, 0.95);

    var rgb = c.rgb;
    // Shadow floor lifts the blacks.
    rgb = vec3<f32>(floor_eff) + (vec3<f32>(1.0) - vec3<f32>(floor_eff)) * rgb;
    // S-curve.
    let sc = rgb * rgb * (3.0 - 2.0 * rgb);
    rgb = mix(rgb, sc, s_eff);
    // Highlight shoulder.
    rgb = vec3<f32>(
        shoulder(rgb.r, knee_eff, white_eff),
        shoulder(rgb.g, knee_eff, white_eff),
        shoulder(rgb.b, knee_eff, white_eff),
    );

    // Paper tone: warm cream to cool cyan-blue.
    let warm = vec3<f32>(1.02, 1.0, 0.96);
    let cool = vec3<f32>(0.96, 0.99, 1.03);
    let tt = clamp(params.paper_tint / 100.0 + 0.5, 0.0, 1.0);
    rgb = rgb * mix(cool, warm, tt);

    let sim = params.simulation / 100.0;
    store_image(idx, vec4<f32>(mix(c.rgb, rgb, sim), c.a));
}
