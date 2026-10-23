// Grain pass 3: composite the grain field onto the image.
//
// Modulates by the adaptive luminance zones, mixes the sharp field toward the
// blurred field for depth-based focus control, and compensates the contrast lost
// to the blur.
//
// Binding convention: 0 = image, 1 = sharp field, 2 = blurred field,
// 3 = depth, 4 = output, 5 = uniform.

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
    /// Long side of the source image; kept in the layout shared by the grain
    /// shaders. The apply pass does not need it.
    original_long_side: u32,
    /// Luminance range of the source image, used by the adaptive zones.
    luma_min: f32,
    luma_max: f32,
};

@group(0) @binding(0) var<storage, read> src: array<u32>;
@group(0) @binding(1) var<storage, read> sharp: array<u32>;
@group(0) @binding(2) var<storage, read> blurred: array<u32>;
@group(0) @binding(3) var<storage, read> depth: array<u32>;
@group(0) @binding(4) var<storage, read_write> dst: array<u32>;
@group(0) @binding(5) var<uniform> params: Params;

const LUMA = vec3<f32>(0.2126, 0.7152, 0.0722);
const GRAIN_GAIN: f32 = 0.22;
const SHADOW_ZONE_END: f32 = 0.30;
const HIGHLIGHT_FADE_START: f32 = 0.55;
const HIGHLIGHT_FADE_END: f32 = 0.85;
const SHADOW_FADE: f32 = 0.15;

// Zone weights form a partition of unity over the image's adaptive tonal
// range: shadows below 0.30, a mids plateau to 0.55, then a fade to pure
// highlights at 0.85. The darkest 15 percent fades toward 0.05, relaxed when
// the Shadows slider is above neutral.
fn zone_multiplier(luma: f32, m_shadows: f32, m_mids: f32, m_highlights: f32) -> f32 {
    var p = luma;
    let range = params.luma_max - params.luma_min;
    if (range >= 0.01) {
        p = clamp((luma - params.luma_min) / range, 0.0, 1.0);
    }

    let ws = 1.0 - smoothstep(0.0, SHADOW_ZONE_END, p);
    let wh = smoothstep(HIGHLIGHT_FADE_START, HIGHLIGHT_FADE_END, p);
    let wm = max(1.0 - ws - wh, 0.0);
    var zone = ws * m_shadows + wm * m_mids + wh * m_highlights;

    if (p < SHADOW_FADE) {
        let fade = smoothstep(0.0, 1.0, p / SHADOW_FADE);
        let faded = mix(0.05, zone, fade);
        let relax = clamp(m_shadows - 1.0, 0.0, 1.0);
        zone = mix(faded, zone, relax);
    }
    return max(zone, 0.0);
}

//@IMAGE_LOAD
//@IMAGE_STORE
//@DITHER

// 16-bit loader for the auxiliary grain fields. Depth stays 8-bit.
fn unpack16(p0: u32, p1: u32) -> vec4<f32> {
    return vec4<f32>(
        f32(p0 & 0xFFFFu),
        f32((p0 >> 16u) & 0xFFFFu),
        f32(p1 & 0xFFFFu),
        f32((p1 >> 16u) & 0xFFFFu),
    ) / 65535.0;
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.width || gid.y >= params.height) {
        return;
    }
    let idx = gid.y * params.width + gid.x;
    let c = load_image(idx);

    let luma = dot(c.rgb, LUMA);
    let mult = vec3<f32>(params.lum_shadows, params.lum_mids, params.lum_highlights) / 50.0;
    let zone = zone_multiplier(luma, mult.x, mult.y, mult.z);

    // Focus: 1 in focus (near) uses the sharp field; 0 uses the blurred field.
    var focus = 0.0;
    if (params.depth_intensity > 0.0) {
        let d = f32(depth[idx] & 0xFFu) / 255.0;
        focus = (params.depth_intensity / 100.0) * d;
    }

    let n = unpack16(sharp[idx * 2u], sharp[idx * 2u + 1u]).rgb;
    let b = unpack16(blurred[idx * 2u], blurred[idx * 2u + 1u]).rgb;
    let field = mix(b, n, focus);

    // Mild contrast compensation for the blur, driven by the normalised
    // softness so preview and export compensate by the same amount.
    let comp = 1.0 + 0.25 * (params.softness / 100.0);
    let grain = (field - vec3<f32>(0.5)) * 2.0 * comp;
    let amp = (params.intensity / 100.0) * zone * GRAIN_GAIN;
    let grained = c.rgb + grain * amp + vec3<f32>(output_dither(idx));

    store_image(idx, vec4<f32>(grained, c.a));
}
