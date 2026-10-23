//! Shader templating for the image buffers.
//!
//! Passes that read or write the full-resolution image must work with either
//! 8-bit (one `u32` per pixel) or 16-bit (two `u32` per pixel) buffers. Each such
//! shader marks where the image loader and storer go with `//@IMAGE_LOAD` and
//! `//@IMAGE_STORE`; [`image_shader`] substitutes the right implementation for a
//! given [`PixelFormat`]. The shader bodies then call `load_image(idx)` and
//! `store_image(idx, colour)`.
//!
//! Auxiliary buffers (the grain field, the bloom chain, the depth map) keep
//! their own inline loaders; the grain field is 16-bit since Phase 2. Shaders
//! that want output dithering mark the spot with `//@DITHER`; it is active for
//! 8-bit output and compiled to zero for 16-bit output.

use crate::image::PixelFormat;

/// 8-bit image loader for a binding named `src`.
const LOAD8: &str = r#"fn load_image(i: u32) -> vec4<f32> {
    let p = src[i];
    return vec4<f32>(
        f32(p & 0xFFu), f32((p >> 8u) & 0xFFu),
        f32((p >> 16u) & 0xFFu), f32((p >> 24u) & 0xFFu)) / 255.0;
}"#;

/// 8-bit image storer for a binding named `dst`.
const STORE8: &str = r#"fn store_image(i: u32, c: vec4<f32>) {
    let r = u32(round(clamp(c.r, 0.0, 1.0) * 255.0));
    let g = u32(round(clamp(c.g, 0.0, 1.0) * 255.0));
    let b = u32(round(clamp(c.b, 0.0, 1.0) * 255.0));
    let a = u32(round(clamp(c.a, 0.0, 1.0) * 255.0));
    dst[i] = r | (g << 8u) | (b << 16u) | (a << 24u);
}"#;

/// 16-bit image loader for a binding named `src` (two words per pixel).
const LOAD16: &str = r#"fn load_image(i: u32) -> vec4<f32> {
    let w0 = src[i * 2u];
    let w1 = src[i * 2u + 1u];
    return vec4<f32>(
        f32(w0 & 0xFFFFu), f32((w0 >> 16u) & 0xFFFFu),
        f32(w1 & 0xFFFFu), f32((w1 >> 16u) & 0xFFFFu)) / 65535.0;
}"#;

/// 16-bit image storer for a binding named `dst` (two words per pixel).
const STORE16: &str = r#"fn store_image(i: u32, c: vec4<f32>) {
    let r = u32(round(clamp(c.r, 0.0, 1.0) * 65535.0));
    let g = u32(round(clamp(c.g, 0.0, 1.0) * 65535.0));
    let b = u32(round(clamp(c.b, 0.0, 1.0) * 65535.0));
    let a = u32(round(clamp(c.a, 0.0, 1.0) * 65535.0));
    dst[i * 2u] = r | (g << 16u);
    dst[i * 2u + 1u] = b | (a << 16u);
}"#;

/// Half an 8-bit step of deterministic output dither, keyed by pixel index.
const DITHER8: &str = r#"fn output_dither(i: u32) -> f32 {
    var h = i * 0x9e3779b1u;
    h = h ^ (h >> 16u);
    h = h * 0x7feb352du;
    h = h ^ (h >> 15u);
    h = h * 0x846ca68bu;
    h = h ^ (h >> 16u);
    return (f32(h) * (1.0 / 4294967295.0) - 0.5) / 255.0;
}"#;

/// 16-bit output already has enough precision; dithering would only add noise.
const DITHER16: &str = r#"fn output_dither(_i: u32) -> f32 {
    return 0.0;
}"#;

/// Substitute the image loader, storer and output dither for the given format.
pub fn image_shader(wgsl: &str, format: PixelFormat) -> String {
    let (load, store, dither) = match format {
        PixelFormat::Rgba8 => (LOAD8, STORE8, DITHER8),
        PixelFormat::Rgba16 => (LOAD16, STORE16, DITHER16),
    };
    wgsl.replace("//@IMAGE_LOAD", load)
        .replace("//@IMAGE_STORE", store)
        .replace("//@DITHER", dither)
}
