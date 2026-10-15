//! The Grain block.
//!
//! A three-stage pipeline: `scatter` generates the deterministic grain field,
//! `blur` (separable box, horizontal then vertical) softens it, and `apply`
//! composites it with adaptive luminance zones and depth-based focus control.
//! The integer-hash noise means a given `grain_seed` reproduces identical grain.

use crate::error::Result;
use crate::gpu::GpuContext;
use crate::image::{Image, PixelFormat};
use crate::params::{GrainParams, GrainType};
use crate::passes::{GpuImage, ImagePassPair, LazyPass};

/// Maximum box-blur half-width for the softness control.
const MAX_BLUR_RADIUS: f32 = 12.0;

/// The grain pipeline, reusable across images.
pub struct GrainPass {
    scatter: ImagePassPair,
    blur: LazyPass,
    apply: ImagePassPair,
}

impl GrainPass {
    /// Create the (lazily compiled) grain pipeline.
    pub fn new() -> Self {
        Self {
            scatter: ImagePassPair::new(
                "grain.scatter",
                include_str!("../shaders/grain_scatter.wgsl"),
            ),
            blur: LazyPass::new("grain.blur", include_str!("../shaders/grain_blur.wgsl")),
            apply: ImagePassPair::new("grain.apply", include_str!("../shaders/grain_apply.wgsl")),
        }
    }

    /// Apply grain to `image`.
    ///
    /// When `depth` is `Some`, `depth_of_field_intensity` keeps in-focus areas
    /// sharp and softens defocused areas; when `None`, that control is ignored.
    pub fn run(
        &self,
        ctx: &GpuContext,
        image: &GpuImage,
        params: &GrainParams,
        seed: u64,
        depth: Option<&GpuImage>,
    ) -> Result<GpuImage> {
        let (w, h) = (image.width(), image.height());
        let depth_intensity = if depth.is_some() {
            params.depth_of_field_intensity
        } else {
            0.0
        };
        let radius = (params.roughness / 100.0 * MAX_BLUR_RADIUS).round() as u32;

        let uniform = GrainUniform::new(w, h, params, seed, depth_intensity, radius);
        let bytes = uniform.as_bytes();

        // 1. Scatter the grain field (8-bit).
        let field = self.scatter.select(ctx, image.format()).run_gpu(
            ctx,
            &[image],
            w,
            h,
            PixelFormat::Rgba8,
            bytes,
        )?;

        // 2. Separable box blur.
        let blur = self.blur.get(ctx);
        let horizontal = blur.run_gpu(
            ctx,
            &[&field],
            w,
            h,
            PixelFormat::Rgba8,
            bytemuck::cast_slice(&blur_uniform(w, h, radius, 0)),
        )?;
        let blurred = blur.run_gpu(
            ctx,
            &[&horizontal],
            w,
            h,
            PixelFormat::Rgba8,
            bytemuck::cast_slice(&blur_uniform(w, h, radius, 1)),
        )?;

        // 3. Composite (with an optional depth map).
        let dummy = GpuImage::upload(ctx, &Image::from_rgba8(1, 1, vec![255, 255, 255, 255])?)?;
        let depth_ref = depth.unwrap_or(&dummy);
        self.apply.select(ctx, image.format()).run_gpu(
            ctx,
            &[image, &field, &blurred, depth_ref],
            w,
            h,
            image.format(),
            bytes,
        )
    }
}

impl Default for GrainPass {
    fn default() -> Self {
        Self::new()
    }
}

/// The blur pass uniform: dimensions, half-width and axis.
fn blur_uniform(width: u32, height: u32, radius: u32, dir: u32) -> [u32; 4] {
    [width, height, (radius as i32) as u32, dir]
}

/// The 80-byte uniform block shared with the scatter and apply shaders.
struct GrainUniform([u32; 20]);

impl GrainUniform {
    fn new(
        width: u32,
        height: u32,
        p: &GrainParams,
        seed: u64,
        depth_intensity: f32,
        blur_radius: u32,
    ) -> Self {
        let grain_type = match p.grain_type {
            GrainType::Coarse => 0,
            GrainType::Smooth => 1,
            GrainType::Fine => 2,
        };
        Self([
            width,
            height,
            seed as u32,
            (seed >> 32) as u32,
            p.intensity.to_bits(),
            p.grain_amount.to_bits(),
            p.roughness.to_bits(),
            p.grain_color.to_bits(),
            p.color_shift.to_bits(),
            p.dissolution_amount.to_bits(),
            p.luminance_highlights.to_bits(),
            p.luminance_mids.to_bits(),
            p.luminance_shadows.to_bits(),
            p.fine_grain_intensity.to_bits(),
            grain_type,
            depth_intensity.to_bits(),
            blur_radius,
            0,
            0,
            0,
        ])
    }

    fn as_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Recipe;
    use crate::image::Image;
    use crate::params::GrainParams;

    fn ctx_or_skip() -> Option<GpuContext> {
        match GpuContext::new_blocking() {
            Ok(ctx) => Some(ctx),
            Err(e) => {
                eprintln!("skipping GPU test: {e}");
                None
            }
        }
    }

    fn sample_image(w: u32, h: u32) -> Image {
        let data: Vec<u8> = (0..w * h * 4).map(|i| ((i * 29 + 7) % 256) as u8).collect();
        Image::from_rgba8(w, h, data).unwrap()
    }

    fn recipe_with(intensity: f32, seed: u64) -> Recipe {
        Recipe {
            grain_seed: seed,
            grain: GrainParams {
                intensity,
                ..Default::default()
            },
            ..Recipe::default()
        }
    }

    #[test]
    fn zero_intensity_is_identity() {
        let Some(ctx) = ctx_or_skip() else { return };
        let img = sample_image(24, 16);
        let out = crate::render::render(&ctx, &img, &recipe_with(0.0, 0)).unwrap();
        assert_eq!(out, img);
    }

    #[test]
    fn grain_is_deterministic() {
        let Some(ctx) = ctx_or_skip() else { return };
        let img = sample_image(24, 16);
        let recipe = recipe_with(50.0, 0x1234_5678_9abc_def0);
        let a = crate::render::render(&ctx, &img, &recipe).unwrap();
        let b = crate::render::render(&ctx, &img, &recipe).unwrap();
        assert_eq!(a, b, "same seed must give identical grain");
    }

    #[test]
    fn different_seeds_differ() {
        let Some(ctx) = ctx_or_skip() else { return };
        let img = sample_image(24, 16);
        let a = crate::render::render(&ctx, &img, &recipe_with(50.0, 1)).unwrap();
        let b = crate::render::render(&ctx, &img, &recipe_with(50.0, 2)).unwrap();
        assert_ne!(a, b, "different seeds should change the grain");
    }

    #[test]
    fn grain_changes_the_image() {
        let Some(ctx) = ctx_or_skip() else { return };
        let img = sample_image(32, 32);
        let out = crate::render::render(&ctx, &img, &recipe_with(100.0, 42)).unwrap();
        assert_ne!(out, img, "full-intensity grain should alter pixels");
        // Alpha must be preserved.
        for (i, px) in out.data().chunks_exact(4).enumerate() {
            assert_eq!(px[3], img.data()[i * 4 + 3]);
        }
    }

    #[test]
    fn softness_changes_the_grain() {
        let Some(ctx) = ctx_or_skip() else { return };
        let img = sample_image(32, 32);
        let mut sharp = recipe_with(80.0, 42);
        sharp.grain.roughness = 0.0;
        let mut soft = recipe_with(80.0, 42);
        soft.grain.roughness = 100.0;
        let a = crate::render::render(&ctx, &img, &sharp).unwrap();
        let b = crate::render::render(&ctx, &img, &soft).unwrap();
        assert_ne!(a, b, "softness should change the grain");
    }

    #[test]
    fn focus_control_uses_the_depth_map() {
        let Some(ctx) = ctx_or_skip() else { return };
        let img = sample_image(32, 32);
        let mut recipe = recipe_with(100.0, 42);
        recipe.grain.depth_of_field_intensity = 100.0;
        let depth = crate::depth::DepthMap::from_bytes(
            32,
            32,
            (0..32 * 32).map(|i| ((i * 5) % 256) as u8).collect(),
        )
        .unwrap();

        let renderer = crate::render::Renderer::new();
        let flat = renderer.render(&ctx, &img, &recipe, None).unwrap();
        let focused = renderer.render(&ctx, &img, &recipe, Some(&depth)).unwrap();
        assert_ne!(
            flat, focused,
            "focus control should change the grain when a depth map is present"
        );
    }
}
