//! Shader templating for the image buffers.
//!
//! Passes that read or write the full-resolution image must work with either
//! 8-bit (one `u32` per pixel) or 16-bit (two `u32` per pixel) buffers. Each such
//! shader marks where the image loader and storer go with `//@IMAGE_LOAD` and
//! `//@IMAGE_STORE`; [`image_shader`] substitutes the right implementation for a
//! given [`PixelFormat`]. The shader bodies then call `load_image(idx)` and
//! `store_image(idx, colour)`.
//!
//! Auxiliary buffers (the grain field, the bloom chain, the depth map) are always
//! 8-bit and keep their own inline loaders.

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

/// Substitute the image loader and storer for the given format.
pub fn image_shader(wgsl: &str, format: PixelFormat) -> String {
    let (load, store) = match format {
        PixelFormat::Rgba8 => (LOAD8, STORE8),
        PixelFormat::Rgba16 => (LOAD16, STORE16),
    };
    wgsl.replace("//@IMAGE_LOAD", load)
        .replace("//@IMAGE_STORE", store)
}
